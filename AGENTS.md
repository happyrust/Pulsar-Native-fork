# AGENTS.md

## Cursor Cloud specific instructions

### Project Overview
Pulsar Engine is a Rust-based experimental game engine. It is a Cargo workspace with ~37 crates under `crates/` and `ui-crates/`. The default member is `crates/engine` (the main binary).

### Known Linux Build Limitation
The full workspace does **not** compile on Linux. The GPUI dependency (Zed fork at `tristanpoland/zed`) references `vk_device()`, `vk_image()`, `vk_texture_memory()`, and `vk_external_memory_fd()` methods on `blade_graphics::Context`/`Texture` in `dmabuf_export.rs`, which do not exist in the pinned `blade-graphics` fork. This code is gated behind `#[cfg(target_os = "linux")]`, so it only fails on Linux. The README explicitly notes "Non-Windows support is temporarily limited while core architectural changes are underway." The project CI (`ci.yml`) is also currently failing on all platforms due to a missing `script/setup-dev-environment.sh` referenced by `script/bootstrap`.

### Crates That Build on Linux
The following workspace crates compile successfully on Linux and can be used for development:
- `blueprint_compiler`
- `helio-feature-skies`
- `profiling@0.1.0` (use version qualifier to disambiguate from `profiling@1.0.17` third-party crate)
- `ui_field_registry`
- `ui_gen_macros`

### Build & Test Commands
- **Build buildable crates:** `cargo build -p blueprint_compiler -p helio-feature-skies -p profiling@0.1.0 -p ui_field_registry -p ui_gen_macros`
- **Test buildable crates:** `cargo test -p blueprint_compiler -p helio-feature-skies -p profiling@0.1.0 -p ui_field_registry`
- **Lint buildable crates:** `cargo clippy -p blueprint_compiler -p helio-feature-skies -p profiling@0.1.0 -p ui_field_registry -p ui_gen_macros`
- **Full workspace build (currently Linux-broken):** `cargo build --workspace`
- **Full test suite (currently Linux-broken):** `cargo test --all`

### System Dependencies
Required Linux packages are listed in `script/install-linux-deps`. Key packages: `gcc`, `g++`, `clang`, `libfontconfig-dev`, `libwayland-dev`, `libwebkit2gtk-4.1-dev`, `libxkbcommon-x11-dev`, `libx11-xcb-dev`, `libssl-dev`, `libzstd-dev`, `vulkan-validationlayers`, `libvulkan1`. Also needs `cmake` and `pkg-config`.

### Notes
- The `.cargo/config.toml` sets `rustflags = ["-A", "warnings"]` globally and `jobs = 4`.
- Rust stable toolchain is required (`rust-toolchain` file).
- This is a GPU-accelerated GUI application requiring a display server and Vulkan driver to run the engine binary.
- The `multiuser_server` crate has a missing `libc` dependency for Linux builds.
