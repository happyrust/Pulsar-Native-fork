//! Plant Model Viewer — Bevy 0.18 + plant-model-gen API
//!
//! 从 plant-model-gen 的 Web API 获取模型树和 glTF/GLB 模型数据，
//! 在 Bevy 3D 场景中渲染显示。
//!
//! ## 功能
//! - 通过 `/api/e3d/children/{refno}` 获取模型树层级
//! - 通过 `/api/export/glb` 导出指定节点的 GLB 模型
//! - 在 Bevy 中加载并显示 3D 模型
//! - UI 面板显示模型树，点击节点加载模型
//!
//! ## 使用方法
//! 1. 确保 plant-model-gen 服务已在运行
//! 2. 设置环境变量 `PLANT_API_URL`（默认 http://localhost:8080）
//! 3. 运行: `cargo run --bin plant_model_viewer`
//!
//! 若 API 不可用，将使用内置的 demo 模型展示功能。

use bevy::prelude::*;
use bevy::ecs::message::MessageReader;
use bevy::input::mouse::MouseMotion;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  plant-model-gen API 数据结构
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// E3D 树节点（来自 /api/e3d/children/{refno}）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNodeDto {
    pub refno: String,
    pub name: String,
    pub noun: String,
    pub owner: Option<String>,
    pub children_count: Option<i32>,
}

/// E3D children 响应
#[derive(Debug, Deserialize)]
pub struct ChildrenResponse {
    pub success: bool,
    pub parent_refno: String,
    pub children: Vec<TreeNodeDto>,
    pub truncated: bool,
    pub error_message: Option<String>,
}

/// E3D 世界根节点响应
#[derive(Debug, Deserialize)]
pub struct NodeResponse {
    pub success: bool,
    pub node: Option<TreeNodeDto>,
    pub error_message: Option<String>,
}

/// 导出请求
#[derive(Debug, Serialize)]
pub struct ExportRequest {
    pub refnos: Vec<String>,
    pub format: String,
    pub include_descendants: Option<bool>,
}

/// 导出状态响应
#[derive(Debug, Deserialize)]
pub struct ExportStatusResponse {
    pub task_id: String,
    pub status: String,
    pub progress: Option<u8>,
    pub message: Option<String>,
    pub result_url: Option<String>,
    pub error: Option<String>,
}

/// 导出创建响应
#[derive(Debug, Deserialize)]
pub struct ExportResponse {
    pub success: bool,
    pub task_id: String,
    pub message: String,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  API 客户端
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Clone)]
pub struct PlantApiClient {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl PlantApiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// 获取世界根节点
    pub fn get_world_root(&self) -> Result<Option<TreeNodeDto>, String> {
        let url = format!("{}/api/e3d/world-root", self.base_url);
        let resp: NodeResponse = self.client.get(&url)
            .send().map_err(|e| format!("Request failed: {}", e))?
            .json().map_err(|e| format!("Parse failed: {}", e))?;
        if resp.success {
            Ok(resp.node)
        } else {
            Err(resp.error_message.unwrap_or_else(|| "Unknown error".into()))
        }
    }

    /// 获取子节点列表
    pub fn get_children(&self, refno: &str) -> Result<Vec<TreeNodeDto>, String> {
        let url = format!("{}/api/e3d/children/{}", self.base_url, refno);
        let resp: ChildrenResponse = self.client.get(&url)
            .send().map_err(|e| format!("Request failed: {}", e))?
            .json().map_err(|e| format!("Parse failed: {}", e))?;
        if resp.success {
            Ok(resp.children)
        } else {
            Err(resp.error_message.unwrap_or_else(|| "Unknown error".into()))
        }
    }

    /// 提交 GLB 导出任务
    pub fn export_glb(&self, refnos: Vec<String>) -> Result<String, String> {
        let url = format!("{}/api/export/glb", self.base_url);
        let req = ExportRequest {
            refnos,
            format: "glb".to_string(),
            include_descendants: Some(true),
        };
        let resp: ExportResponse = self.client.post(&url)
            .json(&req)
            .send().map_err(|e| format!("Export request failed: {}", e))?
            .json().map_err(|e| format!("Parse export response failed: {}", e))?;
        if resp.success {
            Ok(resp.task_id)
        } else {
            Err(resp.message)
        }
    }

    /// 查询导出任务状态
    pub fn get_export_status(&self, task_id: &str) -> Result<ExportStatusResponse, String> {
        let url = format!("{}/api/export/status/{}", self.base_url, task_id);
        self.client.get(&url)
            .send().map_err(|e| format!("Status request failed: {}", e))?
            .json().map_err(|e| format!("Parse status failed: {}", e))
    }

    /// 下载导出结果
    pub fn download_export(&self, task_id: &str) -> Result<Vec<u8>, String> {
        let url = format!("{}/api/export/download/{}", self.base_url, task_id);
        let bytes = self.client.get(&url)
            .send().map_err(|e| format!("Download failed: {}", e))?
            .bytes().map_err(|e| format!("Read bytes failed: {}", e))?;
        Ok(bytes.to_vec())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Bevy Resources & Components
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 模型树 — 已加载的节点
#[derive(Resource, Default)]
struct ModelTree {
    nodes: Vec<TreeNodeDto>,
    selected: Option<usize>,
    expanded_refnos: Vec<String>,
    loading: bool,
    error: Option<String>,
    api_available: bool,
}

/// API 客户端资源
#[derive(Resource)]
struct ApiClientRes(Arc<PlantApiClient>);

/// 已加载的 GLB 模型
#[derive(Resource, Default)]
struct LoadedModels {
    current_entity: Option<Entity>,
    loading_task_id: Option<String>,
}

/// 飞行相机控制
#[derive(Component)]
struct FlyCamera {
    speed: f32,
    sensitivity: f32,
    yaw: f32,
    pitch: f32,
}

impl Default for FlyCamera {
    fn default() -> Self {
        Self {
            speed: 10.0,
            sensitivity: 0.003,
            yaw: -std::f32::consts::FRAC_PI_4,
            pitch: -0.3,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Main
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn main() {
    let api_url = std::env::var("PLANT_API_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());

    info!("Plant Model Viewer starting...");
    info!("API URL: {}", api_url);

    let client = Arc::new(PlantApiClient::new(&api_url));

    // 测试 API 连接
    let api_available = match client.get_world_root() {
        Ok(Some(root)) => {
            info!("✅ API connected! World root: {} ({})", root.name, root.noun);
            true
        }
        Ok(None) => {
            warn!("⚠️  API connected but no world root found");
            true
        }
        Err(e) => {
            warn!("⚠️  API not available: {}. Running in demo mode.", e);
            false
        }
    };

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Plant Model Viewer — Bevy 0.18 × plant-model-gen".into(),
                resolution: (1280, 800).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ApiClientRes(client))
        .insert_resource(ModelTree {
            api_available,
            ..default()
        })
        .init_resource::<LoadedModels>()
        .add_systems(Startup, (setup_scene, load_initial_tree))
        .add_systems(Update, (
            fly_camera_controller,
            render_tree_ui,
            handle_tree_click,
            poll_export_status,
        ))
        .run();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Scene Setup
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground grid
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(100.0, 100.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.18, 0.15),
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));

    // Coordinate axes
    let axis_length = 5.0;
    let axis_radius = 0.03;

    // X axis (red)
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(axis_radius, axis_length))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.1, 0.1),
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(axis_length / 2.0, 0.0, 0.0)
            .with_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)),
    ));

    // Y axis (green)
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(axis_radius, axis_length))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.1, 0.9, 0.1),
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(0.0, axis_length / 2.0, 0.0),
    ));

    // Z axis (blue)
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(axis_radius, axis_length))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.1, 0.1, 0.9),
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, axis_length / 2.0)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
    ));

    // Lighting
    commands.spawn((
        DirectionalLight {
            illuminance: light_consts::lux::OVERCAST_DAY,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::ZYX, 0.0, 0.8, -std::f32::consts::FRAC_PI_4,
        )),
    ));

    commands.spawn((
        PointLight {
            intensity: 500_000.0,
            range: 100.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(10.0, 20.0, 10.0),
    ));

    // Camera with fly controller
    commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(0.12, 0.12, 0.16)),
            ..default()
        },
        Transform::from_xyz(15.0, 10.0, 15.0).looking_at(Vec3::ZERO, Vec3::Y),
        FlyCamera::default(),
    ));

    info!("✅ 3D scene initialized with coordinate axes and lighting");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Tree Loading
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn load_initial_tree(
    api: Res<ApiClientRes>,
    mut tree: ResMut<ModelTree>,
) {
    if !tree.api_available {
        // Demo mode: populate with fake data
        tree.nodes = vec![
            TreeNodeDto { refno: "1_0".into(), name: "WORLD".into(), noun: "WORL".into(), owner: None, children_count: Some(2) },
            TreeNodeDto { refno: "1_1".into(), name: "SITE-001".into(), noun: "SITE".into(), owner: Some("1_0".into()), children_count: Some(3) },
            TreeNodeDto { refno: "1_2".into(), name: "ZONE-PIPE".into(), noun: "ZONE".into(), owner: Some("1_1".into()), children_count: Some(5) },
            TreeNodeDto { refno: "1_3".into(), name: "EQUI-PUMP-01".into(), noun: "EQUI".into(), owner: Some("1_2".into()), children_count: Some(0) },
            TreeNodeDto { refno: "1_4".into(), name: "PIPE-001".into(), noun: "PIPE".into(), owner: Some("1_2".into()), children_count: Some(2) },
            TreeNodeDto { refno: "1_5".into(), name: "VALV-001".into(), noun: "VALV".into(), owner: Some("1_4".into()), children_count: Some(0) },
        ];
        info!("📋 Demo tree loaded with {} nodes", tree.nodes.len());
        return;
    }

    tree.loading = true;

    // Fetch world root
    match api.0.get_world_root() {
        Ok(Some(root)) => {
            let root_refno = root.refno.clone();
            tree.nodes.push(root);

            // Fetch first level children
            match api.0.get_children(&root_refno) {
                Ok(children) => {
                    info!("📋 Loaded {} top-level children", children.len());
                    tree.nodes.extend(children);
                }
                Err(e) => {
                    warn!("Failed to load children: {}", e);
                    tree.error = Some(e);
                }
            }
        }
        Ok(None) => {
            tree.error = Some("No world root found".into());
        }
        Err(e) => {
            tree.error = Some(e);
        }
    }

    tree.loading = false;
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  UI Rendering
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn render_tree_ui(
    tree: Res<ModelTree>,
    mut commands: Commands,
    existing_ui: Query<Entity, With<Node>>,
) {
    if !tree.is_changed() {
        return;
    }

    // Clear existing UI
    for entity in existing_ui.iter() {
        commands.entity(entity).despawn();
    }

    // Build tree panel
    let panel = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            top: Val::Px(10.0),
            width: Val::Px(320.0),
            max_height: Val::Percent(90.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(12.0)),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.92)),
    )).id();

    // Title
    let title = commands.spawn((
        Text::new(if tree.api_available {
            "🌿 Plant Model Tree"
        } else {
            "🌿 Plant Model Tree (Demo)"
        }),
        TextFont { font_size: 18.0, ..default() },
        TextColor(Color::srgb(0.9, 0.95, 0.9)),
        Node { margin: UiRect::bottom(Val::Px(8.0)), ..default() },
    )).id();
    commands.entity(panel).add_child(title);

    // Error message
    if let Some(ref err) = tree.error {
        let err_node = commands.spawn((
            Text::new(format!("⚠ {}", err)),
            TextFont { font_size: 12.0, ..default() },
            TextColor(Color::srgb(1.0, 0.6, 0.3)),
            Node { margin: UiRect::bottom(Val::Px(6.0)), ..default() },
        )).id();
        commands.entity(panel).add_child(err_node);
    }

    // Tree nodes
    for (i, node) in tree.nodes.iter().enumerate() {
        let is_selected = tree.selected == Some(i);
        let indent = compute_indent(node, &tree.nodes);
        let has_children = node.children_count.unwrap_or(0) > 0;

        let icon = match node.noun.as_str() {
            "WORL" => "🌍",
            "SITE" => "🏗",
            "ZONE" => "📦",
            "EQUI" => "⚙",
            "PIPE" => "🔧",
            "BRAN" => "🔀",
            "VALV" => "🔴",
            "NOZZ" => "🔌",
            "STRU" => "🏛",
            "PUMP" => "💧",
            "TANK" => "🛢",
            "INST" => "📐",
            _ => "📄",
        };

        let label = format!(
            "{}{} {} [{}]{}",
            if has_children { "▸ " } else { "  " },
            icon,
            node.name,
            node.noun,
            if is_selected { " ◀" } else { "" }
        );

        let bg = if is_selected {
            Color::srgba(0.2, 0.4, 0.6, 0.7)
        } else {
            Color::NONE
        };

        let text_node = commands.spawn((
            Text::new(label),
            TextFont { font_size: 13.0, ..default() },
            TextColor(if is_selected {
                Color::srgb(1.0, 1.0, 1.0)
            } else {
                Color::srgb(0.75, 0.8, 0.75)
            }),
            Node {
                padding: UiRect::new(Val::Px(indent), Val::Px(4.0), Val::Px(3.0), Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(bg),
            TreeNodeIndex(i),
            Interaction::default(),
        )).id();
        commands.entity(panel).add_child(text_node);
    }

    // Status bar
    let status = commands.spawn((
        Text::new(format!("Nodes: {} | Click to select, Enter to load model", tree.nodes.len())),
        TextFont { font_size: 11.0, ..default() },
        TextColor(Color::srgb(0.5, 0.55, 0.5)),
        Node { margin: UiRect::top(Val::Px(8.0)), ..default() },
    )).id();
    commands.entity(panel).add_child(status);
}

#[derive(Component)]
struct TreeNodeIndex(usize);

fn compute_indent(node: &TreeNodeDto, all_nodes: &[TreeNodeDto]) -> f32 {
    let mut depth = 0;
    let mut current_owner = node.owner.clone();
    while let Some(ref owner) = current_owner {
        if all_nodes.iter().any(|n| n.refno == *owner) {
            depth += 1;
            current_owner = all_nodes.iter()
                .find(|n| n.refno == *owner)
                .and_then(|n| n.owner.clone());
        } else {
            break;
        }
    }
    depth as f32 * 16.0
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Interaction
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn handle_tree_click(
    interactions: Query<(&Interaction, &TreeNodeIndex), Changed<Interaction>>,
    mut tree: ResMut<ModelTree>,
    api: Res<ApiClientRes>,
    mut loaded: ResMut<LoadedModels>,
) {
    for (interaction, idx) in interactions.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let i = idx.0;
        tree.selected = Some(i);

        // Clone data we need before mutating tree
        let node_name = tree.nodes[i].name.clone();
        let node_noun = tree.nodes[i].noun.clone();
        let node_refno = tree.nodes[i].refno.clone();
        let children_count = tree.nodes[i].children_count.unwrap_or(0);

        info!("Selected: {} ({}) refno={}", node_name, node_noun, node_refno);

        // Expand children if API available
        if tree.api_available && children_count > 0 {
            if !tree.expanded_refnos.contains(&node_refno) {
                match api.0.get_children(&node_refno) {
                    Ok(children) => {
                        info!("Loaded {} children for {}", children.len(), node_refno);
                        let insert_pos = i + 1;
                        for (j, child) in children.into_iter().enumerate() {
                            if !tree.nodes.iter().any(|n| n.refno == child.refno) {
                                tree.nodes.insert(insert_pos + j, child);
                            }
                        }
                        tree.expanded_refnos.push(node_refno.clone());
                    }
                    Err(e) => warn!("Failed to load children: {}", e),
                }
            }
        }

        // Try to export GLB for leaf nodes
        if tree.api_available && children_count == 0 && loaded.loading_task_id.is_none() {
            match api.0.export_glb(vec![node_refno.clone()]) {
                Ok(task_id) => {
                    info!("Export task created: {} for refno {}", task_id, node_refno);
                    loaded.loading_task_id = Some(task_id);
                }
                Err(e) => warn!("Export failed: {}", e),
            }
        }
    }
}

fn poll_export_status(
    api: Res<ApiClientRes>,
    mut loaded: ResMut<LoadedModels>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    tree: Res<ModelTree>,
) {
    let Some(ref task_id) = loaded.loading_task_id else { return };
    if !tree.api_available { return; }

    match api.0.get_export_status(task_id) {
        Ok(status) => {
            match status.status.as_str() {
                "completed" => {
                    info!("Export completed! Downloading GLB...");
                    match api.0.download_export(task_id) {
                        Ok(glb_bytes) => {
                            let file_path = format!("/tmp/plant_model_{}.glb", task_id);
                            if std::fs::write(&file_path, &glb_bytes).is_ok() {
                                info!("GLB saved to {}, loading into Bevy...", file_path);

                                // Remove previous model
                                if let Some(prev) = loaded.current_entity {
                                    commands.entity(prev).despawn();
                                }

                                // Use a 'static string for asset loading
                                let static_path: String = file_path.clone();
                                let asset_path = bevy::gltf::GltfAssetLabel::Scene(0)
                                    .from_asset(static_path);
                                let entity = commands.spawn(SceneRoot(
                                    asset_server.load(asset_path),
                                )).id();
                                loaded.current_entity = Some(entity);
                            }
                        }
                        Err(e) => warn!("Download failed: {}", e),
                    }
                    loaded.loading_task_id = None;
                }
                "failed" => {
                    warn!("Export task failed: {:?}", status.error);
                    loaded.loading_task_id = None;
                }
                _ => {} // still running
            }
        }
        Err(e) => {
            warn!("Status check failed: {}", e);
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Camera Controller
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn fly_camera_controller(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut query: Query<(&mut Transform, &mut FlyCamera)>,
) {
    let dt = time.delta_secs();

    for (mut transform, mut cam) in query.iter_mut() {
        // Mouse look (right-click held)
        if mouse_buttons.pressed(MouseButton::Right) {
            for ev in mouse_motion.read() {
                cam.yaw -= ev.delta.x * cam.sensitivity;
                cam.pitch = (cam.pitch - ev.delta.y * cam.sensitivity)
                    .clamp(-1.5, 1.5);
            }
        } else {
            mouse_motion.clear();
        }

        let forward = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0) * Vec3::NEG_Z;
        let right = Quat::from_rotation_y(cam.yaw) * Vec3::X;

        let mut velocity = Vec3::ZERO;
        if keys.pressed(KeyCode::KeyW) { velocity += forward; }
        if keys.pressed(KeyCode::KeyS) { velocity -= forward; }
        if keys.pressed(KeyCode::KeyA) { velocity -= right; }
        if keys.pressed(KeyCode::KeyD) { velocity += right; }
        if keys.pressed(KeyCode::Space) { velocity += Vec3::Y; }
        if keys.pressed(KeyCode::ShiftLeft) { velocity -= Vec3::Y; }

        let speed_mult = if keys.pressed(KeyCode::ControlLeft) { 3.0 } else { 1.0 };

        if velocity.length_squared() > 0.0 {
            transform.translation += velocity.normalize() * cam.speed * speed_mult * dt;
        }

        transform.rotation = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0);
    }
}
