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
