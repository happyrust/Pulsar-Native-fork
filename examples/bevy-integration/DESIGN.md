# Bevy 0.18 × Pulsar Engine 集成设计

## 概述

本文档分析如何将 Bevy 0.18 集成到 Pulsar Engine 中，替代现有的 Helio 渲染器，同时保留 GPUI 编辑器 UI 层。

## 当前架构 vs 目标架构

### 当前架构（Helio + GPUI）

```
Winit (窗口)
  └─ WinitGpuiApp (事件循环)
       ├─ GPUI Application (2D 编辑器 UI)
       │    └─ BevyViewport (透明 "洞")
       │
       ├─ Helio Renderer (blade-graphics/Vulkan)
       │    └─ SceneDb (无锁原子场景数据库)
       │
       └─ D3D11 Compositor (Windows)
            ├─ Layer 1: Helio 3D 输出 (不透明)
            └─ Layer 2: GPUI UI 输出 (Alpha 混合)
```

### 目标架构（Bevy + GPUI）

```
┌─────────────────────────────────────────────────────────┐
│                    Pulsar Engine                        │
│                                                         │
│  ┌───────────────────┐    ┌──────────────────────────┐ │
│  │     Bevy App       │    │      GPUI Application    │ │
│  │  (独立线程/异步)    │    │   (编辑器 UI 主线程)     │ │
│  │                    │    │                          │ │
│  │  ┌──────────────┐ │    │  ┌────────────────────┐  │ │
│  │  │ 3D Scene     │ │    │  │ Level Editor Panel │  │ │
│  │  │ ECS World    │ │    │  │ Hierarchy Panel    │  │ │
│  │  │ Physics      │ │    │  │ Properties Panel   │  │ │
│  │  │ Render Graph │ │    │  │ Settings / etc.    │  │ │
│  │  └──────┬───────┘ │    │  └────────┬───────────┘  │ │
│  │         │         │    │           │              │ │
│  │    RenderTarget   │    │     BevyViewport         │ │
│  │    (offscreen)    │    │     (透明容器)            │ │
│  │         │         │    │           │              │ │
│  └─────────┼─────────┘    └───────────┼──────────────┘ │
│            │                          │                │
│            ▼                          ▼                │
│  ┌──────────────────────────────────────────────────┐  │
│  │            共享 GPU 纹理层                        │  │
│  │                                                  │  │
│  │  Windows: DXGI NT Handle (D3D12 → D3D11)         │  │
│  │  Linux:   DMA-BUF fd  (Vulkan → Vulkan)          │  │
│  │  macOS:   IOSurface    (Metal → Metal)            │  │
│  └──────────────────────────────────────────────────┘  │
│                         │                              │
│                         ▼                              │
│              ┌──────────────────┐                      │
│              │    Compositor    │                      │
│              │  Bevy 3D + GPUI │                      │
│              └──────────────────┘                      │
└─────────────────────────────────────────────────────────┘
```

## 集成方案分析

### 方案 A：Bevy 替代 Helio 渲染器（推荐）

**思路**：Bevy 以无头模式运行，替代 Helio 作为 3D 渲染后端。

**修改点**：

| 模块 | 变更 |
|------|------|
| `engine_backend/src/subsystems/render/` | 用 `BevyRenderer` 替代 `HelioRenderer` |
| `engine_backend/src/services/gpu_renderer.rs` | 适配 Bevy 的 `RenderTarget::Image` |
| `engine_backend/src/scene/mod.rs` | `SceneDb` 同步到 Bevy `World` |
| `ui/src/bevy_viewport.rs` | 不需要修改（已是透明容器） |
| `engine/src/window/rendering/compositor.rs` | 从 Bevy 输出纹理替代 Helio 纹理 |

**优点**：
- 最小化改动——只替换渲染后端
- GPUI 编辑器 UI 完全不变
- 获得 Bevy 完整渲染能力（PBR、阴影、Bloom、后处理、Solari 光追）

**缺点**：
- 需要处理 Bevy 和 blade-graphics (GPUI 内部) 可能的 wgpu 版本冲突
- 需要 SceneDb ↔ Bevy World 的双向同步

### 方案 B：Bevy 完整替代（ECS + 渲染）

**思路**：用 Bevy 的 ECS 替代 SceneDb，用 Bevy 的 World 作为真正的数据源。

**修改点**：

| 模块 | 变更 |
|------|------|
| `engine_backend/src/scene/` | 用 Bevy `World` 的查询替代 |
| `engine_backend/src/subsystems/` | Physics/GameThread 改为 Bevy Systems |
| 所有 UI 面板 | 通过 bridge 查询 Bevy World |

**优点**：
- 统一数据模型，无需同步
- 可利用 Bevy 整个生态（物理用 bevy_rapier、场景用 bevy_scene）

**缺点**：
- 改动巨大，几乎重写引擎后端
- GPUI 和 Bevy 运行在不同线程，World 访问需要仔细设计

### 方案 C：Bevy 作为游戏运行时（推荐用于 Play 模式）

**思路**：编辑模式使用现有 Helio，Play 模式启动 Bevy App 运行完整游戏。

**修改点**：最小——只在 Play 按钮时 spawn Bevy。

## 推荐实施路径

采用 **方案 A 分阶段实施**：

### Phase 1：Bevy 渲染器封装（本示例）

```rust
pub struct BevyRendererSubsystem {
    app: App,
    world: World,
    render_target: Handle<Image>,
    shared_texture: Arc<SharedFrameBuffer>,
}

impl BevyRendererSubsystem {
    pub fn new(width: u32, height: u32) -> Self {
        let mut app = App::new();
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            // 无头模式：不创建窗口
            primary_window: None,
            ..default()
        }));
        // ... 设置 RenderTarget::Image
        todo!()
    }

    /// 每帧调用，推进 Bevy 模拟
    pub fn tick(&mut self) {
        self.app.update();
    }

    /// 获取当前帧的共享纹理句柄
    pub fn get_native_texture_handle(&self) -> Option<GpuTextureHandle> {
        // 从 Bevy 的 wgpu texture 获取平台原生句柄
        todo!()
    }
}
```

### Phase 2：SceneDb ↔ Bevy World 同步

```rust
/// 将 SceneDb 的变更同步到 Bevy World
fn sync_scene_to_bevy(scene_db: &SceneDb, world: &mut World) {
    scene_db.for_each_entry(|entry| {
        let entity = world.spawn((
            Transform::from_translation(entry.position().into()),
            // ... 映射 ObjectType 到 Bevy 组件
        ));
    });
}

/// 将 Bevy 物理模拟结果写回 SceneDb
fn sync_bevy_to_scene(world: &World, scene_db: &SceneDb) {
    for (entity, transform) in world.query::<(&SceneId, &Transform)>() {
        scene_db.set_position(&entity.id, transform.translation.into());
    }
}
```

### Phase 3：零拷贝 GPU 纹理共享

```rust
// Linux: 从 Bevy 的 wgpu 获取 VkImage → 导出 DMA-BUF fd
// 然后 GPUI 的 blade-graphics 导入这个 fd

// 这需要 wgpu 暴露底层 Vulkan handle（wgpu-hal API）
fn export_bevy_texture_as_dmabuf(
    render_device: &RenderDevice,
    texture: &GpuImage,
) -> Option<i32> {  // DMA-BUF fd
    // wgpu::hal::vulkan::Device::export_memory_as_fd(...)
    todo!()
}
```

## 关键技术挑战

### 1. wgpu 版本冲突

Bevy 0.18 使用自己的 wgpu 版本。GPUI 通过 blade-graphics 使用另一个版本。
两者可能冲突。

**解决方案**：
- 使用 CPU 回读作为临时桥接（本示例的方案）
- 长期：通过 DMA-BUF/NT Handle 在 GPU 驱动层面共享

### 2. 线程模型

Bevy 有自己的调度器（多线程 ECS），GPUI 有自己的事件循环。

**解决方案**：
- Bevy 在独立线程运行 `app.update()` 循环
- 通过 `Arc<SharedFrameBuffer>` 或 channel 通信
- GPUI 以 ~60Hz 轮询新帧

### 3. 输入路由

鼠标/键盘事件需要从 GPUI 的 BevyViewport 转发到 Bevy。

**解决方案**：
- 现有的 `CameraInput` 结构已经在做类似的事
- 改为向 Bevy 发送 `bevy::input` 事件

## 文件结构建议

```
crates/
  bevy_bridge/           ← 新 crate
    src/
      lib.rs             ← BevyRendererSubsystem
      scene_sync.rs      ← SceneDb ↔ Bevy World 同步
      texture_share.rs   ← 平台纹理共享
      input_forward.rs   ← GPUI → Bevy 输入转发
      camera_control.rs  ← 编辑器相机控制
```
