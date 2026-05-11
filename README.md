# Xcode

A high-performance, offline-first code editor built in Rust.
...
## Architecture
- `core`: Text buffer, piece table, file system abstractions.
- `renderer`: GPU-accelerated rendering (`wgpu`), layout engine, split-pane support.
- `terminal`: Integrated terminal emulator (`alacritty_terminal`, `portable-pty`).
- `ui`: Tabs, file tree, command palette, settings UI.
- `services`: Git/GitHub integration, LSP client implementation.
- `plugin-runtime`: Wasm sandbox for extensions.

## Configuration
The editor uses a `.xcode` file (TOML) for project-specific workspace state and settings.
# Xcoder
# Xcoder

## Optimization & Build Management

To maintain a small disk footprint and fast compilation times:

1. **Clean Builds**: Use `cargo clean` periodically to remove accumulated intermediate artifacts.
2. **Release Profile**: The codebase is optimized for production. Builds with `cargo build --release` generate a compact binary (~13MB) with symbol stripping and Thin LTO.
3. **Workspace Dependencies**: All shared dependencies are managed in the root `Cargo.toml` under `[workspace.dependencies]`.
4. **Targeted Builds**: If working on a specific crate, use `cargo build -p <crate_name>` to skip unnecessary work.
