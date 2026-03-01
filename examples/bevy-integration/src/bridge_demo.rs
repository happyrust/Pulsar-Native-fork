//! Bevy ↔ GPUI Bridge Demo
//!
//! Demonstrates the integration pattern: Bevy renders to an offscreen texture,
//! a shared buffer transfers frames to a simulated "GPUI consumer" thread.
//!
//! In Pulsar production, SharedFrameBuffer would be replaced by zero-copy
//! GPU texture sharing (DXGI/DMA-BUF/IOSurface).
//!
//! Run: cargo run --bin bevy_bridge_demo

use bevy::prelude::*;
use bevy::camera::{RenderTarget, ScalingMode, visibility::RenderLayers};
use bevy::render::render_resource::TextureFormat;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  SharedFrameBuffer — The bridge between Bevy and GPUI
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Double-buffered frame transfer between Bevy render thread and GPUI.
///
/// This is the CPU-fallback path. In production, use platform GPU sharing:
///   Windows: DXGI NT Handle (D3D12 ↔ D3D11)
///   Linux:   DMA-BUF fd (Vulkan ↔ Vulkan)
///   macOS:   IOSurface (Metal ↔ Metal)
pub struct SharedFrameBuffer {
    buffers: [RwLock<Vec<u8>>; 2],
    write_index: AtomicU64,
    pub width: u32,
    pub height: u32,
    frame_ready: AtomicBool,
    pub frame_count: AtomicU64,
}

impl SharedFrameBuffer {
    pub fn new(width: u32, height: u32) -> Arc<Self> {
        let buf_size = (width * height * 4) as usize;
        Arc::new(Self {
            buffers: [
                RwLock::new(vec![0u8; buf_size]),
                RwLock::new(vec![0u8; buf_size]),
            ],
            write_index: AtomicU64::new(0),
            width,
            height,
            frame_ready: AtomicBool::new(false),
            frame_count: AtomicU64::new(0),
        })
    }

    pub fn submit_frame(&self, pixels: &[u8]) {
        let write_idx = self.write_index.load(Ordering::Relaxed) as usize % 2;
        if let Ok(mut buf) = self.buffers[write_idx].write() {
            let len = buf.len().min(pixels.len());
            buf[..len].copy_from_slice(&pixels[..len]);
        }
        self.write_index.fetch_add(1, Ordering::Release);
        self.frame_ready.store(true, Ordering::Release);
        self.frame_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn read_frame<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&[u8]) -> R,
    {
        if !self.frame_ready.swap(false, Ordering::Acquire) {
            return None;
        }
        let read_idx = (self.write_index.load(Ordering::Acquire) as usize + 1) % 2;
        self.buffers[read_idx].read().ok().map(|buf| f(&buf))
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Bevy-side resources and components
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Resource)]
struct BevyBridgeState {
    shared_buffer: Arc<SharedFrameBuffer>,
    render_target: Handle<Image>,
}

#[derive(Component)]
struct RotatingCube;

const RENDER_WIDTH: u32 = 640;
const RENDER_HEIGHT: u32 = 480;

fn main() {
    let shared_buffer = SharedFrameBuffer::new(RENDER_WIDTH, RENDER_HEIGHT);
    let consumer_buffer = shared_buffer.clone();

    // Spawn "GPUI consumer" simulation thread
    std::thread::Builder::new()
        .name("gpui-consumer-sim".into())
        .spawn(move || gpui_consumer_loop(consumer_buffer))
        .expect("Failed to spawn consumer thread");

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Pulsar × Bevy 0.18 — Bridge Demo".into(),
                resolution: (800, 600).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(BridgeInit(shared_buffer))
        .add_systems(Startup, setup_scene)
        .add_systems(Update, (rotate_cube, extract_frame))
        .run();
}

#[derive(Resource)]
struct BridgeInit(Arc<SharedFrameBuffer>);

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    init: Res<BridgeInit>,
) {
    let render_target_image = Image::new_target_texture(
        RENDER_WIDTH,
        RENDER_HEIGHT,
        TextureFormat::Bgra8UnormSrgb,
        None,
    );
    let render_target_handle = images.add(render_target_image);

    commands.insert_resource(BevyBridgeState {
        shared_buffer: init.0.clone(),
        render_target: render_target_handle.clone(),
    });

    let offscreen_layer = RenderLayers::layer(1);

    // Ground
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(8.0, 8.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.25, 0.3, 0.25),
            ..default()
        })),
        offscreen_layer.clone(),
    ));

    // Cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.2, 1.2, 1.2))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.1, 0.5, 0.85),
            metallic: 0.9,
            perceptual_roughness: 0.2,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.8, 0.0),
        offscreen_layer.clone(),
        RotatingCube,
    ));

    // Light
    commands.spawn((
        DirectionalLight {
            illuminance: light_consts::lux::OVERCAST_DAY,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::ZYX, 0.0, 0.5, -std::f32::consts::FRAC_PI_4,
        )),
        offscreen_layer.clone(),
    ));

    // Offscreen camera
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: -1,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.08, 0.08, 0.12)),
            ..default()
        },
        RenderTarget::Image(render_target_handle.clone().into()),
        Transform::from_xyz(3.0, 3.5, 5.0).looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y),
        offscreen_layer,
    ));

    // On-screen preview
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(8.0, 6.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(render_target_handle),
            unlit: true,
            ..default()
        })),
    ));

    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: 8.0,
                min_height: 6.0,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 8.0, 0.0).looking_at(Vec3::ZERO, Vec3::Z),
    ));

    info!("✅ Bridge demo ready ({}x{})", RENDER_WIDTH, RENDER_HEIGHT);
}

fn rotate_cube(time: Res<Time>, mut query: Query<&mut Transform, With<RotatingCube>>) {
    for mut t in &mut query {
        t.rotate_y(time.delta_secs() * 0.6);
        t.rotate_x(time.delta_secs() * 0.2);
    }
}

/// Extract pixels from the offscreen Image and push to SharedFrameBuffer.
/// NOTE: CPU-readback fallback — production would use zero-copy GPU sharing.
fn extract_frame(bridge: Option<Res<BevyBridgeState>>, images: Res<Assets<Image>>) {
    let Some(bridge) = bridge else { return };
    let Some(image) = images.get(&bridge.render_target) else { return };
    if let Some(ref data) = image.data {
        if !data.is_empty() {
            bridge.shared_buffer.submit_frame(data);
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  GPUI consumer simulation
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn gpui_consumer_loop(buffer: Arc<SharedFrameBuffer>) {
    info!("[GPUI-SIM] Consumer thread started, polling for frames...");
    let mut last_report = std::time::Instant::now();
    let mut frames_consumed = 0u64;

    loop {
        let got_frame = buffer.read_frame(|data| {
            frames_consumed += 1;
            if last_report.elapsed().as_secs() >= 3 {
                let total = buffer.frame_count.load(Ordering::Relaxed);
                let sample = if data.len() >= 4 {
                    format!("BGRA({},{},{},{})", data[0], data[1], data[2], data[3])
                } else {
                    "N/A".into()
                };
                info!(
                    "[GPUI-SIM] consumed={}, produced={}, pixel[0]={}, size={}KB",
                    frames_consumed, total, sample, data.len() / 1024
                );
                last_report = std::time::Instant::now();
            }
        });

        if got_frame.is_none() {
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    }
}
