//! Bevy 0.18 Headless Rendering Demo for Pulsar Engine Integration
//!
//! Demonstrates Bevy rendering a 3D scene to an offscreen texture,
//! then displaying it on a quad — the first step toward integrating
//! Bevy as the 3D backend for Pulsar's GPUI-based editor.
//!
//! Run: cargo run --bin bevy_headless_demo

use bevy::prelude::*;
use bevy::camera::{RenderTarget, ScalingMode, visibility::RenderLayers};
use bevy::render::render_resource::TextureFormat;

/// Marker for the rotating cube
#[derive(Component)]
struct RotatingCube;

/// Frame counter
#[derive(Resource, Default)]
struct FrameCount(u32);

const RENDER_WIDTH: u32 = 800;
const RENDER_HEIGHT: u32 = 600;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Pulsar × Bevy 0.18 — Render-to-Texture Demo".into(),
                resolution: (1024, 768).into(),
                ..default()
            }),
            ..default()
        }))
        .init_resource::<FrameCount>()
        .add_systems(Startup, setup)
        .add_systems(Update, (rotate_scene, count_frames))
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // ── Step 1: Create offscreen render target using Bevy 0.18 API ──
    let render_target_image = Image::new_target_texture(
        RENDER_WIDTH,
        RENDER_HEIGHT,
        TextureFormat::Bgra8UnormSrgb,
        None,
    );
    let render_target_handle = images.add(render_target_image);

    // ── Step 2: 3D scene on RenderLayer 1 (offscreen only) ──
    let offscreen_layer = RenderLayers::layer(1);

    // Ground plane
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(10.0, 10.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.35, 0.3),
            ..default()
        })),
        offscreen_layer.clone(),
    ));

    // Rotating cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.5, 1.5, 1.5))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.6, 0.9),
            metallic: 0.8,
            perceptual_roughness: 0.3,
            ..default()
        })),
        Transform::from_xyz(0.0, 1.0, 0.0),
        offscreen_layer.clone(),
        RotatingCube,
    ));

    // Sphere
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(0.7).mesh().ico(5).unwrap())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.3, 0.2),
            metallic: 0.5,
            perceptual_roughness: 0.5,
            ..default()
        })),
        Transform::from_xyz(2.5, 0.7, -1.0),
        offscreen_layer.clone(),
    ));

    // Directional light
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

    // ── Step 3: Offscreen camera with RenderTarget component ──
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: -1,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.1, 0.1, 0.15)),
            ..default()
        },
        // RenderTarget is a Component in Bevy 0.18
        RenderTarget::Image(render_target_handle.clone().into()),
        Transform::from_xyz(4.0, 4.0, 6.0).looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y),
        offscreen_layer,
    ));

    // ── Step 4: Preview — display the rendered texture on a quad ──
    // This simulates what GPUI's BevyViewport would do
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(16.0, 12.0))),
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
            clear_color: ClearColorConfig::Custom(Color::srgb(0.05, 0.05, 0.05)),
            ..default()
        },
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: 16.0,
                min_height: 12.0,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 10.0, 0.0).looking_at(Vec3::ZERO, Vec3::Z),
    ));

    info!("✅ Bevy render-to-texture demo initialized ({}x{})", RENDER_WIDTH, RENDER_HEIGHT);
    info!("   Offscreen camera renders to Image → displayed on quad");
    info!("   In Pulsar integration, this texture would be shared with GPUI");
}

fn rotate_scene(time: Res<Time>, mut query: Query<&mut Transform, With<RotatingCube>>) {
    for mut transform in &mut query {
        transform.rotate_y(time.delta_secs() * 0.8);
        transform.rotate_x(time.delta_secs() * 0.3);
    }
}

fn count_frames(mut counter: ResMut<FrameCount>) {
    counter.0 += 1;
    if counter.0 % 300 == 0 {
        info!("Frame {}: render-to-texture running smoothly", counter.0);
    }
}
