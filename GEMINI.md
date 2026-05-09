# Xcode Project Documentation

## Vision
A super lightweight, ultra-fast, cross-platform code editor designed for maximum performance and a custom experimental UI experience.

## Technical Stack
- **Language:** Rust
- **Rendering:** GPU-accelerated (Targeting cross-platform support via Vulkan/Metal/DirectX abstractions)
- **Architecture:** 
    - Decoupled LSP (Language Server Protocol) support for language features.
    - Tree-sitter for incremental syntax parsing.
    - Custom experimental UI layer.

## Development Principles
- **Performance First:** Every UI action must be near-instant. Minimize main-thread blocking.
- **Native-First:** Avoid Electron/Web-based runtimes.
- **Modularity:** Maintain a clean separation between the text-buffer engine, the UI layer, and the LSP communication.

## Roadmap
1. **MVP Phase:** 
    - Text buffer implementation.
    - Basic cross-platform window management.
    - Minimalist text rendering (GPU-based).
    - File loading/saving.
2. **Feature Phase:**
    - Syntax highlighting (Tree-sitter).
    - LSP integration.
    - Basic UI elements (file tree, command palette).
