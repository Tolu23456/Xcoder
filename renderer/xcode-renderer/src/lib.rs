#![deny(warnings)]
#![allow(dead_code)]

mod rect;

use crate::rect::{RectRenderer, RectVertex};
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextAtlas, TextBounds, TextArea, TextRenderer, Viewport,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use taffy::prelude::*;
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey, ModifiersState},
    window::{Window, WindowId},
};
use xcode_services::{get_rust_lang, LspClient, SyntaxHighlighter};
use xcode_ui::{LayoutNode, PaneContent, Theme};

pub struct RenderState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    window: Arc<Window>,

    // Text rendering
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,

    // Rect rendering
    rect_renderer: RectRenderer,

    // UI Elements
    main_text_buffer: Buffer,
    sidebar_buffer: Buffer,
    tabs_buffer: Buffer,
    status_buffer: Buffer,
    terminal_buffer: Buffer,
    activity_buffer: Buffer,
    gutter_buffer: Buffer,
    palette_buffer: Buffer,
    hover_buffer: Buffer,
    completion_buffer: Buffer,

    // Layout
    taffy: taffy::TaffyTree<()>,
    root_node: NodeId,
    activity_node: NodeId,
    sidebar_node: NodeId,
    content_node: NodeId,
    tabs_node: NodeId,
    gutter_node: NodeId,
    editor_node: NodeId,
    editor_container_node: NodeId,
    terminal_node: NodeId,
    status_node: NodeId,

    // Workspace State
    workspace: xcode_ui::Workspace,
    sidebar_selected_index: usize,
    modifiers: ModifiersState,
    theme: Theme,

    // Command Palette
    palette_open: bool,
    palette_query: String,
    palette_anim: f32,

    // Hover Popup
    hover_open: bool,
    hover_text: String,
    hover_anim: f32,

    // Autocompletion & Diagnostics
    completion_open: bool,
    completion_items: Vec<String>,
    diagnostics: Vec<serde_json::Value>,

    // Services
    highlighter: SyntaxHighlighter,
    terminal: TerminalEmulatorWrapper,
    lsp: Option<LspClient>,
    lsp_version: i32,

    // Pending LSP requests
    pending_lsp_requests: Vec<u64>,

    // Animation/Cursor/Scrolling
    start_time: Instant,
    target_scroll_offset: f32,
    current_scroll_offset: f32,
}

struct TerminalEmulatorWrapper {
    inner: xcode_terminal::TerminalEmulator,
}

impl TerminalEmulatorWrapper {
    fn new() -> Self {
        Self {
            inner: xcode_terminal::TerminalEmulator::new(),
        }
    }
}

impl RenderState {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                    experimental_features: wgpu::ExperimentalFeatures::default(),
                    trace: wgpu::Trace::Off,
                },
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(&device);
        let mut text_atlas = TextAtlas::new(&device, &queue, &cache, surface_format);
        let text_renderer = TextRenderer::new(
            &mut text_atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );
        let mut viewport = Viewport::new(&device, &cache);
        viewport.update(
            &queue,
            Resolution {
                width: size.width,
                height: size.height,
            },
        );

        let rect_renderer = RectRenderer::new(&device, surface_format);

        let main_text_buffer = Buffer::new(&mut font_system, Metrics::new(13.0, 19.0));
        let sidebar_buffer = Buffer::new(&mut font_system, Metrics::new(12.0, 18.0));
        let tabs_buffer = Buffer::new(&mut font_system, Metrics::new(11.0, 24.0));
        let status_buffer = Buffer::new(&mut font_system, Metrics::new(11.0, 18.0));
        let terminal_buffer = Buffer::new(&mut font_system, Metrics::new(12.0, 18.0));
        let activity_buffer = Buffer::new(&mut font_system, Metrics::new(18.0, 40.0));
        let gutter_buffer = Buffer::new(&mut font_system, Metrics::new(13.0, 19.0));
        let palette_buffer = Buffer::new(&mut font_system, Metrics::new(13.0, 24.0));
        let hover_buffer = Buffer::new(&mut font_system, Metrics::new(12.0, 18.0));
        let completion_buffer = Buffer::new(&mut font_system, Metrics::new(12.0, 18.0));

        let mut taffy: taffy::TaffyTree<()> = taffy::TaffyTree::new();
        let activity_node = taffy
            .new_leaf(Style {
                size: Size {
                    width: length(48.0),
                    height: percent(1.0),
                },
                ..Default::default()
            })
            .unwrap();

        let sidebar_node = taffy
            .new_leaf(Style {
                size: Size {
                    width: length(220.0),
                    height: percent(1.0),
                },
                ..Default::default()
            })
            .unwrap();

        let tabs_node = taffy
            .new_leaf(Style {
                size: Size {
                    width: percent(1.0),
                    height: length(32.0),
                },
                ..Default::default()
            })
            .unwrap();

        let gutter_node = taffy
            .new_leaf(Style {
                size: Size {
                    width: length(40.0),
                    height: percent(1.0),
                },
                ..Default::default()
            })
            .unwrap();

        let editor_node = taffy
            .new_leaf(Style {
                size: Size {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            })
            .unwrap();

        let editor_container_node = taffy
            .new_with_children(
                Style {
                    flex_direction: FlexDirection::Row,
                    size: Size {
                        width: percent(1.0),
                        height: percent(0.65),
                    },
                    ..Default::default()
                },
                &[gutter_node, editor_node],
            )
            .unwrap();

        let terminal_node = taffy
            .new_leaf(Style {
                size: Size {
                    width: percent(1.0),
                    height: percent(0.35),
                },
                ..Default::default()
            })
            .unwrap();

        let content_node = taffy
            .new_with_children(
                Style {
                    flex_direction: FlexDirection::Column,
                    size: Size {
                        width: percent(0.8),
                        height: percent(1.0),
                    },
                    ..Default::default()
                },
                &[tabs_node, editor_container_node, terminal_node],
            )
            .unwrap();

        let status_node = taffy
            .new_leaf(Style {
                size: Size {
                    width: percent(1.0),
                    height: length(22.0),
                },
                ..Default::default()
            })
            .unwrap();

        let root_node = taffy
            .new_with_children(
                Style {
                    flex_direction: FlexDirection::Row,
                    size: Size {
                        width: percent(1.0),
                        height: percent(1.0),
                    },
                    ..Default::default()
                },
                &[activity_node, sidebar_node, content_node],
            )
            .unwrap();

        let workspace = xcode_ui::Workspace::new_home();
        let mut lsp = LspClient::new("rust-analyzer").ok();
        if let Some(client) = &mut lsp {
            let _ = client.initialize("file://.");
        }

        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            font_system,
            swash_cache,
            viewport,
            text_atlas,
            text_renderer,
            rect_renderer,
            main_text_buffer,
            sidebar_buffer,
            tabs_buffer,
            status_buffer,
            terminal_buffer,
            activity_buffer,
            gutter_buffer,
            palette_buffer,
            hover_buffer,
            completion_buffer,
            taffy,
            root_node,
            activity_node,
            sidebar_node,
            content_node,
            tabs_node,
            gutter_node,
            editor_node,
            editor_container_node,
            terminal_node,
            status_node,
            workspace,
            sidebar_selected_index: 0,
            modifiers: ModifiersState::default(),
            theme: Theme::one_dark(),
            palette_open: false,
            palette_query: String::new(),
            palette_anim: 0.0,
            hover_open: false,
            hover_text: String::new(),
            hover_anim: 0.0,
            completion_open: false,
            completion_items: Vec::new(),
            diagnostics: Vec::new(),
            highlighter: SyntaxHighlighter::new(get_rust_lang()),
            terminal: TerminalEmulatorWrapper::new(),
            lsp,
            lsp_version: 1,
            pending_lsp_requests: Vec::new(),
            start_time: Instant::now(),
            target_scroll_offset: 0.0,
            current_scroll_offset: 0.0,
        }
    }

    fn update_ui_buffers(&mut self) {
        let lerp_factor = 0.1;
        self.current_scroll_offset +=
            (self.target_scroll_offset - self.current_scroll_offset) * lerp_factor;
        if (self.target_scroll_offset - self.current_scroll_offset).abs() < 0.1 {
            self.current_scroll_offset = self.target_scroll_offset;
        }
        if self.palette_open {
            self.palette_anim = (self.palette_anim + 0.1).min(1.0);
        } else {
            self.palette_anim = (self.palette_anim - 0.1).max(0.0);
        }
        if self.hover_open {
            self.hover_anim = (self.hover_anim + 0.1).min(1.0);
        } else {
            self.hover_anim = (self.hover_anim - 0.1).max(0.0);
        }
        if let Some(lsp) = &self.lsp {
            if let LayoutNode::Leaf(pane) = &self.workspace.root {
                if let PaneContent::Editor(editor) = &pane.content {
                    let uri = format!("file://{}", editor.path.display());
                    if let Ok(diags) = lsp.diagnostics.lock() {
                        if let Some(file_diags) = diags.get(&uri) {
                            self.diagnostics = file_diags.clone();
                        }
                    }
                }
            }
            let mut i = 0;
            while i < self.pending_lsp_requests.len() {
                let id = self.pending_lsp_requests[i];
                if let Some(response) = lsp.get_response(id) {
                    if let Some(result) = response.get("result") {
                        if let Some(uri) = result
                            .get("uri")
                            .or_else(|| result.get(0).and_then(|r| r.get("uri")))
                        {
                            if let Some(path_str) =
                                uri.as_str().and_then(|s| s.strip_prefix("file://"))
                            {
                                self.workspace.open_editor(PathBuf::from(path_str));
                            }
                        }
                        if let Some(items) = result.get("items") {
                            self.completion_items = items
                                .as_array()
                                .unwrap_or(&vec![])
                                .iter()
                                .map(|it| it["label"].as_str().unwrap_or("").to_string())
                                .collect();
                            self.completion_open = !self.completion_items.is_empty();
                        }
                        if let Some(contents) = result.get("contents") {
                            if let Some(value) = contents.get("value") {
                                self.hover_text = value.as_str().unwrap_or("").to_string();
                                self.hover_open = true;
                            } else if let Some(s) = contents.as_str() {
                                self.hover_text = s.to_string();
                                self.hover_open = true;
                            }
                        }
                    }
                    self.pending_lsp_requests.remove(i);
                } else {
                    i += 1;
                }
            }
        }
        let mut sidebar_text = String::from(" PROJECT\n ───────\n");
        for (i, file) in self.workspace.files.iter().enumerate() {
            let icon = if file.is_dir { "📁" } else { "📄" };
            let prefix = if i == self.sidebar_selected_index { ">" } else { " " };
            sidebar_text.push_str(&format!("{} {} {}\n", prefix, icon, file.name));
        }
        self.sidebar_buffer.set_text(
            &mut self.font_system,
            &sidebar_text,
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
            None,
        );
        if let LayoutNode::Leaf(pane) = &self.workspace.root {
            if let PaneContent::Editor(editor) = &pane.content {
                let line_height = 19.0;
                let visible_lines = (self.size.height as f32 / line_height) as usize + 2;
                let start_line = (self.current_scroll_offset / line_height) as usize;
                let end_line =
                    (start_line + visible_lines).min(editor.document.buffer.line_count());
                let mut gutter_text = String::new();
                for i in start_line..end_line {
                    gutter_text.push_str(&format!("{:>3}\n", i + 1));
                }
                let comment_color = self.to_glyphon_color(self.theme.comment);
                self.gutter_buffer.set_text(
                    &mut self.font_system,
                    &gutter_text,
                    &Attrs::new().family(Family::Monospace).color(comment_color),
                    Shaping::Advanced,
                    None,
                );
            }
        }
        self.tabs_buffer.set_text(
            &mut self.font_system,
            "  main.rs  |  lib.rs  |  Cargo.toml  ",
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
            None,
        );
        let mut status_text = String::from(" [ LSP: ");
        if self.lsp.is_some() {
            status_text.push_str("Connected ] ");
        } else {
            status_text.push_str("Disconnected ] ");
        }
        status_text.push_str(" | Rust | UTF-8");
        self.status_buffer.set_text(
            &mut self.font_system,
            &status_text,
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
            None,
        );
        let display_output = {
            let terminal_output = self.terminal.inner.output.lock().unwrap();
            let lines: Vec<&str> = terminal_output.lines().rev().take(10).collect();
            let mut out = String::new();
            for line in lines.into_iter().rev() {
                out.push_str(line);
                out.push('\n');
            }
            out
        };
        self.terminal_buffer.set_text(
            &mut self.font_system,
            &display_output,
            &Attrs::new().family(Family::Monospace),
            Shaping::Advanced,
            None,
        );
        self.activity_buffer.set_text(
            &mut self.font_system,
            " 📂\n 🔍\n ⚙️",
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
            None,
        );
        if self.palette_anim > 0.0 {
            self.palette_buffer.set_text(
                &mut self.font_system,
                &format!(" > {}", self.palette_query),
                &Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
                None,
            );
        }
        if self.hover_anim > 0.0 {
            self.hover_buffer.set_text(
                &mut self.font_system,
                &self.hover_text,
                &Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
                None,
            );
            self.hover_buffer
                .set_size(&mut self.font_system, Some(300.0), None);
        }
        if self.completion_open {
            let text = self.completion_items.join("\n");
            self.completion_buffer.set_text(
                &mut self.font_system,
                &text,
                &Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
                None,
            );
        }
        if let LayoutNode::Leaf(pane) = &self.workspace.root {
            match &pane.content {
                PaneContent::Home => {
                    let home_text = "\n\n      [ Xcode ]\n\n   Welcome to Xcode\n\n   - Press Ctrl+N for New File\n   - Press Esc to Exit\n\n   (Offline-First Edition)";
                    self.main_text_buffer.set_text(
                        &mut self.font_system,
                        home_text,
                        &Attrs::new()
                            .family(Family::Monospace)
                            .weight(glyphon::Weight::BOLD),
                        Shaping::Advanced,
                        None,
                    );
                }
                PaneContent::Editor(editor) => {
                    let line_height = 19.0;
                    let visible_lines = (self.size.height as f32 / line_height) as usize + 2;
                    let start_line = (self.current_scroll_offset / line_height) as usize;
                    let end_line =
                        (start_line + visible_lines).min(editor.document.buffer.line_count());
                    let mut visible_text = String::new();
                    for i in start_line..end_line {
                        if let Some(line) = editor.document.buffer.get_line(i) {
                            visible_text.push_str(&line);
                            if !line.ends_with('\n') {
                                visible_text.push('\n');
                            }
                        }
                    }
                    let highlights = self.highlighter.highlight(&visible_text);
                    let mut rich_text = Vec::new();
                    let mut last_idx = 0;
                    let text_default = self.to_glyphon_color(self.theme.text_default);
                    let keyword_color = self.to_glyphon_color(self.theme.keyword);
                    let function_color = self.to_glyphon_color(self.theme.function);
                    let string_color = self.to_glyphon_color(self.theme.string);

                    for (start, end, kind) in highlights {
                        if start > last_idx {
                            rich_text.push((
                                &visible_text[last_idx..start],
                                Attrs::new().family(Family::Monospace).color(text_default),
                            ));
                        }
                        let color = match kind.as_str() {
                            "fn" | "let" | "pub" | "use" | "match" | "if" | "else" | "struct"
                            | "enum" | "impl" | "trait" | "type" | "mod" | "static" | "const"
                            | "return" | "break" | "continue" | "for" | "while" | "loop" => {
                                keyword_color
                            }
                            "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16"
                            | "i32" | "i64" | "i128" | "isize" | "f32" | "f64" | "str"
                            | "String" | "Vec" | "Option" | "Result" | "bool" | "true"
                            | "false" => function_color,
                            "string" | "string_literal" | "char_literal" => string_color,
                            _ => text_default,
                        };
                        rich_text.push((
                            &visible_text[start..end],
                            Attrs::new().family(Family::Monospace).color(color),
                        ));
                        last_idx = end;
                    }
                    if last_idx < visible_text.len() {
                        rich_text.push((
                            &visible_text[last_idx..],
                            Attrs::new().family(Family::Monospace).color(text_default),
                        ));
                    }
                    self.main_text_buffer.set_rich_text(
                        &mut self.font_system,
                        rich_text,
                        &Attrs::new().family(Family::Monospace),
                        Shaping::Advanced,
                        None,
                    );
                }
                _ => {}
            }
        }
        self.main_text_buffer
            .shape_until_scroll(&mut self.font_system, false);
        self.sidebar_buffer
            .shape_until_scroll(&mut self.font_system, false);
        self.tabs_buffer
            .shape_until_scroll(&mut self.font_system, false);
        self.status_buffer
            .shape_until_scroll(&mut self.font_system, false);
        self.terminal_buffer
            .shape_until_scroll(&mut self.font_system, false);
        self.activity_buffer
            .shape_until_scroll(&mut self.font_system, false);
        self.gutter_buffer
            .shape_until_scroll(&mut self.font_system, false);
        self.palette_buffer
            .shape_until_scroll(&mut self.font_system, false);
        self.hover_buffer
            .shape_until_scroll(&mut self.font_system, false);
        self.completion_buffer
            .shape_until_scroll(&mut self.font_system, false);
    }
    fn to_glyphon_color(&self, c: xcode_ui::Color) -> Color {
        Color::rgba(
            (c.r * 255.0) as u8,
            (c.g * 255.0) as u8,
            (c.b * 255.0) as u8,
            (c.a * 255.0) as u8,
        )
    }
    pub fn handle_input(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
                return true;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                match delta {
                    MouseScrollDelta::LineDelta(_, y) => {
                        self.target_scroll_offset = (self.target_scroll_offset - y * 40.0).max(0.0);
                    }
                    MouseScrollDelta::PixelDelta(pos) => {
                        self.target_scroll_offset =
                            (self.target_scroll_offset - pos.y as f32).max(0.0);
                    }
                }
                self.hover_open = false;
                self.update_ui_buffers();
                return true;
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if key_event.state == ElementState::Pressed {
                    if self.modifiers.control_key()
                        && self.modifiers.shift_key()
                        && matches!(key_event.logical_key, Key::Character(ref c) if c == "p")
                    {
                        self.palette_open = !self.palette_open;
                        self.palette_query.clear();
                        self.update_ui_buffers();
                        return true;
                    }
                    if !self.palette_open && !self.completion_open && !self.hover_open {
                        match &key_event.logical_key {
                            Key::Named(NamedKey::ArrowUp) if self.modifiers.alt_key() => {
                                self.sidebar_selected_index = self.sidebar_selected_index.saturating_sub(1);
                                self.update_ui_buffers();
                                return true;
                            }
                            Key::Named(NamedKey::ArrowDown) if self.modifiers.alt_key() => {
                                self.sidebar_selected_index = (self.sidebar_selected_index + 1).min(self.workspace.files.len().saturating_sub(1));
                                self.update_ui_buffers();
                                return true;
                            }
                            Key::Named(NamedKey::Enter) if self.modifiers.alt_key() => {
                                if let Some(file) = self.workspace.files.get(self.sidebar_selected_index) {
                                    if !file.is_dir {
                                        let path = file.path.clone();
                                        self.workspace.open_editor(path);
                                        self.update_ui_buffers();
                                    }
                                }
                                return true;
                            }
                            _ => {}
                        }
                    }
                    if self.palette_open {
                        match &key_event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                self.palette_open = false;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                self.palette_query.pop();
                            }
                            Key::Named(NamedKey::Enter) => {
                                self.palette_open = false;
                            }
                            Key::Character(text) => {
                                if !text.chars().any(|c| c.is_control()) {
                                    self.palette_query.push_str(text);
                                }
                            }
                            _ => {}
                        }
                        self.update_ui_buffers();
                        return true;
                    }
                    if self.hover_open && matches!(key_event.logical_key, Key::Named(NamedKey::Escape))
                    {
                        self.hover_open = false;
                        self.update_ui_buffers();
                        return true;
                    }
                    if self.completion_open {
                        match &key_event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                self.completion_open = false;
                            }
                            Key::Named(NamedKey::Enter) => {
                                self.completion_open = false;
                            }
                            _ => {}
                        }
                        self.update_ui_buffers();
                        return true;
                    }
                    if self.modifiers.control_key()
                        && matches!(key_event.logical_key, Key::Character(ref c) if c == "n")
                    {
                        self.workspace.open_editor(PathBuf::from("untitled"));
                        self.update_ui_buffers();
                        return true;
                    }
                    if self.modifiers.control_key()
                        && matches!(key_event.logical_key, Key::Character(ref c) if c == "s")
                    {
                        if let LayoutNode::Leaf(pane) = &self.workspace.root {
                            if let PaneContent::Editor(editor) = &pane.content {
                                let _ = editor.save();
                            }
                        }
                        return true;
                    }
                    if self.modifiers.control_key()
                        && matches!(key_event.logical_key, Key::Character(ref c) if c == "z")
                    {
                        if let LayoutNode::Leaf(pane) = &mut self.workspace.root {
                            if let PaneContent::Editor(editor) = &mut pane.content {
                                editor.document.undo();
                                editor.cursor_pos = editor.document.cursors[0].position;
                                self.update_ui_buffers();
                            }
                        }
                        return true;
                    }
                    if matches!(key_event.logical_key, Key::Named(NamedKey::F12)) {
                        let mut req_info = None;
                        if let LayoutNode::Leaf(pane) = &self.workspace.root {
                            if let PaneContent::Editor(editor) = &pane.content {
                                let (line, character) =
                                    editor.document.get_line_col(editor.cursor_pos);
                                req_info = Some((editor.path.display().to_string(), line, character));
                            }
                        }
                        if let Some((path, line, character)) = req_info {
                            if let Some(lsp) = &mut self.lsp {
                                if let Ok(id) = lsp.goto_definition(
                                    &format!("file://{}", path),
                                    line,
                                    character,
                                ) {
                                    self.pending_lsp_requests.push(id);
                                }
                            }
                        }
                        return true;
                    }
                    if self.modifiers.control_key()
                        && matches!(key_event.logical_key, Key::Character(ref c) if c == "h")
                    {
                        let mut req_info = None;
                        if let LayoutNode::Leaf(pane) = &self.workspace.root {
                            if let PaneContent::Editor(editor) = &pane.content {
                                let (line, character) =
                                    editor.document.get_line_col(editor.cursor_pos);
                                req_info = Some((editor.path.display().to_string(), line, character));
                            }
                        }
                        if let Some((path, line, character)) = req_info {
                            if let Some(lsp) = &mut self.lsp {
                                if let Ok(id) =
                                    lsp.hover(&format!("file://{}", path), line, character)
                                {
                                    self.pending_lsp_requests.push(id);
                                }
                            }
                        }
                        return true;
                    }
                    if let LayoutNode::Leaf(pane) = &mut self.workspace.root {
                        if let PaneContent::Editor(editor) = &mut pane.content {
                            let mut changed = false;
                            match &key_event.logical_key {
                                Key::Named(NamedKey::Backspace) => {
                                    editor.document.delete_at_cursors();
                                    changed = true;
                                }
                                Key::Named(NamedKey::ArrowLeft) => {
                                    for i in 0..editor.document.cursors.len() {
                                        editor.document.cursors[i].position = editor
                                            .document
                                            .find_prev_char_boundary(editor.document.cursors[i].position);
                                    }
                                    editor.cursor_pos = editor.document.cursors[0].position;
                                }
                                Key::Named(NamedKey::ArrowRight) => {
                                    for i in 0..editor.document.cursors.len() {
                                        editor.document.cursors[i].position = editor
                                            .document
                                            .find_next_char_boundary(editor.document.cursors[i].position);
                                    }
                                    editor.cursor_pos = editor.document.cursors[0].position;
                                }
                                Key::Named(NamedKey::Enter) => {
                                    editor.document.insert_at_cursors("\n");
                                    changed = true;
                                }
                                Key::Named(NamedKey::Space) => {
                                    editor.document.insert_at_cursors(" ");
                                    changed = true;
                                }
                                Key::Character(text) => {
                                    if !text.chars().any(|c| c.is_control()) {
                                        editor.document.insert_at_cursors(text);
                                        changed = true;
                                    }
                                }
                                _ => {}
                            }
                            if changed {
                                let uri = format!("file://{}", editor.path.display());
                                let (line, character) =
                                    editor.document.get_line_col(editor.cursor_pos);
                                self.lsp_version += 1;
                                if let Some(lsp) = &mut self.lsp {
                                    let _ = lsp.did_change(&uri, self.lsp_version, &editor.document.to_string());
                                    if let Ok(id) = lsp.completion(&uri, line, character) {
                                        self.pending_lsp_requests.push(id);
                                    }
                                }
                            }
                            self.hover_open = false;
                            self.update_ui_buffers();
                            return true;
                        }
                    }
                }
            }
            _ => {}
        }
        false
    }
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.viewport.update(
                &self.queue,
                Resolution {
                    width: new_size.width,
                    height: new_size.height,
                },
            );
            self.update_ui_buffers();
        }
    }
    pub fn render(&mut self) -> Result<(), &'static str> {
        self.taffy
            .compute_layout(
                self.root_node,
                taffy::Size {
                    width: taffy::AvailableSpace::Definite(self.size.width as f32),
                    height: taffy::AvailableSpace::Definite(self.size.height as f32),
                },
            )
            .unwrap();
        let activity_layout = *self.taffy.layout(self.activity_node).unwrap();
        let sidebar_layout = *self.taffy.layout(self.sidebar_node).unwrap();
        let tabs_layout = *self.taffy.layout(self.tabs_node).unwrap();
        let gutter_layout = *self.taffy.layout(self.gutter_node).unwrap();
        let editor_layout = *self.taffy.layout(self.editor_node).unwrap();
        let editor_container_layout = *self.taffy.layout(self.editor_container_node).unwrap();
        let terminal_layout = *self.taffy.layout(self.terminal_node).unwrap();
        self.update_ui_buffers();
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            _ => return Err("Surface Error"),
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        let mut vertices = Vec::new();
        let screen_width = self.size.width as f32;
        let screen_height = self.size.height as f32;
        fn push_rect(
            vertices: &mut Vec<RectVertex>,
            x: f32,
            y: f32,
            w: f32,
            h: f32,
            c: xcode_ui::Color,
            sw: f32,
            sh: f32,
            radius: f32,
            opacity: f32,
        ) {
            let x1 = (x / sw) * 2.0 - 1.0;
            let y1 = 1.0 - (y / sh) * 2.0;
            let x2 = ((x + w) / sw) * 2.0 - 1.0;
            let y2 = 1.0 - ((y + h) / sh) * 2.0;
            let color = [c.r, c.g, c.b, c.a * opacity];
            let rect_size = [w, h];
            vertices.push(RectVertex {
                position: [x1, y1],
                color,
                rect_pos: [-w / 2.0, h / 2.0],
                rect_size,
                corner_radius: radius,
                border_width: 0.0,
            });
            vertices.push(RectVertex {
                position: [x2, y1],
                color,
                rect_pos: [w / 2.0, h / 2.0],
                rect_size,
                corner_radius: radius,
                border_width: 0.0,
            });
            vertices.push(RectVertex {
                position: [x1, y2],
                color,
                rect_pos: [-w / 2.0, -h / 2.0],
                rect_size,
                corner_radius: radius,
                border_width: 0.0,
            });
            vertices.push(RectVertex {
                position: [x1, y2],
                color,
                rect_pos: [-w / 2.0, -h / 2.0],
                rect_size,
                corner_radius: radius,
                border_width: 0.0,
            });
            vertices.push(RectVertex {
                position: [x2, y1],
                color,
                rect_pos: [w / 2.0, h / 2.0],
                rect_size,
                corner_radius: radius,
                border_width: 0.0,
            });
            vertices.push(RectVertex {
                position: [x2, y2],
                color,
                rect_pos: [w / 2.0, -h / 2.0],
                rect_size,
                corner_radius: radius,
                border_width: 0.0,
            });
        }
        push_rect(
            &mut vertices,
            activity_layout.location.x,
            activity_layout.location.y,
            activity_layout.size.width,
            activity_layout.size.height,
            self.theme.activity_bar_background,
            screen_width,
            screen_height,
            0.0,
            1.0,
        );
        push_rect(
            &mut vertices,
            gutter_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width,
            gutter_layout.location.y + tabs_layout.size.height,
            gutter_layout.size.width,
            gutter_layout.size.height,
            self.theme.activity_bar_background,
            screen_width,
            screen_height,
            0.0,
            1.0,
        );
        push_rect(
            &mut vertices,
            sidebar_layout.location.x,
            sidebar_layout.location.y,
            sidebar_layout.size.width,
            sidebar_layout.size.height,
            self.theme.sidebar_background,
            screen_width,
            screen_height,
            0.0,
            1.0,
        );
        if let LayoutNode::Leaf(pane) = &self.workspace.root {
            if let PaneContent::Editor(editor) = &pane.content {
                let show_cursor = (self.start_time.elapsed().as_millis() / 500) % 2 == 0;
                let line_height = 19.0;
                let char_width = 7.8;
                let (active_line, active_col) = editor.document.get_line_col(editor.cursor_pos);
                let start_line_visible = (self.current_scroll_offset / line_height) as usize;
                let y_offset = self.current_scroll_offset % line_height;
                if active_line >= start_line_visible {
                    let y = editor_container_layout.location.y
                        + tabs_layout.size.height
                        + ((active_line - start_line_visible) as f32 * line_height)
                        - y_offset;
                    push_rect(
                        &mut vertices,
                        editor_container_layout.location.x + sidebar_layout.size.width + sidebar_layout.location.x,
                        y,
                        editor_container_layout.size.width,
                        line_height,
                        self.theme.active_line_bg,
                        screen_width,
                        screen_height,
                        0.0,
                        1.0,
                    );
                }
                if show_cursor && active_line >= start_line_visible {
                    let x = editor_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width + gutter_layout.size.width + (active_col as f32 * char_width);
                    let y = editor_container_layout.location.y
                        + tabs_layout.size.height
                        + ((active_line - start_line_visible) as f32 * line_height)
                        - y_offset;
                    push_rect(
                        &mut vertices,
                        x,
                        y,
                        2.0,
                        line_height,
                        self.theme.cursor_color,
                        screen_width,
                        screen_height,
                        1.0,
                        1.0,
                    );
                }
                if self.completion_open {
                    let x = editor_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width + gutter_layout.size.width + (active_col as f32 * char_width);
                    let y = editor_container_layout.location.y
                        + tabs_layout.size.height
                        + ((active_line - start_line_visible) as f32 * line_height)
                        - y_offset
                        + line_height;
                    push_rect(
                        &mut vertices,
                        x,
                        y,
                        200.0,
                        100.0,
                        self.theme.sidebar_background,
                        screen_width,
                        screen_height,
                        4.0,
                        1.0,
                    );
                }
                // Draw Diagnostics (Underlines)
                for diag in &self.diagnostics {
                    if let Some(range) = diag.get("range").and_then(|r| r.as_object()) {
                        let start_line = range["start"]["line"].as_u64().unwrap_or(0) as usize;
                        let start_col = range["start"]["character"].as_u64().unwrap_or(0) as usize;
                        let end_line = range["end"]["line"].as_u64().unwrap_or(0) as usize;
                        let end_col = range["end"]["character"].as_u64().unwrap_or(0) as usize;

                        let line_height = 19.0;
                        let char_width = 7.8;
                        let start_line_visible = (self.current_scroll_offset / line_height) as usize;
                        let y_offset = self.current_scroll_offset % line_height;

                        if start_line >= start_line_visible && start_line == end_line {
                            let x = editor_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width + gutter_layout.size.width + (start_col as f32 * char_width);
                            let y = editor_container_layout.location.y
                                + tabs_layout.size.height
                                + ((start_line - start_line_visible) as f32 * line_height)
                                - y_offset
                                + line_height
                                - 2.0;
                            let width = (end_col - start_col) as f32 * char_width;
                            push_rect(
                                &mut vertices,
                                x,
                                y,
                                width.max(4.0),
                                2.0,
                                xcode_ui::Color { r: 0.9, g: 0.1, b: 0.1, a: 1.0 },
                                screen_width,
                                screen_height,
                                0.0,
                                1.0,
                            );
                        }
                    }
                }
            }
        }
        if self.palette_anim > 0.0 {
            push_rect(
                &mut vertices,
                screen_width / 2.0 - 200.0,
                50.0 - (1.0 - self.palette_anim) * 20.0,
                400.0,
                30.0,
                self.theme.activity_bar_background,
                screen_width,
                screen_height,
                8.0,
                self.palette_anim,
            );
        }
        if self.hover_anim > 0.0 {
            push_rect(
                &mut vertices,
                screen_width / 2.0 - 150.0,
                screen_height / 2.0 - 100.0,
                300.0,
                200.0,
                self.theme.sidebar_background,
                screen_width,
                screen_height,
                12.0,
                self.hover_anim,
            );
        }
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: self.theme.background.r as f64,
                            g: self.theme.background.g as f64,
                            b: self.theme.background.b as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.rect_renderer
                .draw(&mut render_pass, &self.device, &vertices);
            let text_color = self.to_glyphon_color(self.theme.text_default);
            let line_height = 19.0;
            let char_width = 7.8;
            let y_offset = self.current_scroll_offset % line_height;
            let mut areas = vec![
                TextArea { buffer: &self.activity_buffer, left: activity_layout.location.x, top: activity_layout.location.y, scale: 1.0, bounds: TextBounds { left: activity_layout.location.x as i32, top: activity_layout.location.y as i32, right: (activity_layout.location.x + activity_layout.size.width) as i32, bottom: (activity_layout.location.y + activity_layout.size.height) as i32 }, default_color: text_color, custom_glyphs: &[] },
                TextArea { buffer: &self.sidebar_buffer, left: sidebar_layout.location.x, top: sidebar_layout.location.y, scale: 1.0, bounds: TextBounds { left: sidebar_layout.location.x as i32, top: sidebar_layout.location.y as i32, right: (sidebar_layout.location.x + sidebar_layout.size.width) as i32, bottom: (sidebar_layout.location.y + sidebar_layout.size.height) as i32 }, default_color: text_color, custom_glyphs: &[] },
                TextArea { buffer: &self.tabs_buffer, left: tabs_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width, top: tabs_layout.location.y, scale: 1.0, bounds: TextBounds { left: (tabs_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width) as i32, top: tabs_layout.location.y as i32, right: self.size.width as i32, bottom: (tabs_layout.location.y + tabs_layout.size.height) as i32 }, default_color: text_color, custom_glyphs: &[] },
                TextArea { buffer: &self.gutter_buffer, left: gutter_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width, top: gutter_layout.location.y + tabs_layout.size.height - y_offset, scale: 1.0, bounds: TextBounds { left: (gutter_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width) as i32, top: (gutter_layout.location.y + tabs_layout.size.height) as i32, right: (gutter_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width + gutter_layout.size.width) as i32, bottom: (gutter_layout.location.y + tabs_layout.size.height + gutter_layout.size.height) as i32 }, default_color: text_color, custom_glyphs: &[] },
                TextArea { buffer: &self.main_text_buffer, left: editor_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width + gutter_layout.size.width, top: editor_layout.location.y + tabs_layout.size.height - y_offset, scale: 1.0, bounds: TextBounds { left: (editor_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width + gutter_layout.size.width) as i32, top: (editor_layout.location.y + tabs_layout.size.height) as i32, right: self.size.width as i32, bottom: (editor_layout.location.y + tabs_layout.size.height + editor_layout.size.height) as i32 }, default_color: text_color, custom_glyphs: &[] },
                TextArea { buffer: &self.terminal_buffer, left: terminal_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width, top: terminal_layout.location.y + tabs_layout.size.height + editor_container_layout.size.height, scale: 1.0, bounds: TextBounds { left: (terminal_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width) as i32, top: (terminal_layout.location.y + tabs_layout.size.height + editor_container_layout.size.height) as i32, right: self.size.width as i32, bottom: self.size.height as i32 }, default_color: text_color, custom_glyphs: &[] },
                TextArea { buffer: &self.status_buffer, left: 0.0, top: self.size.height as f32 - 20.0, scale: 1.0, bounds: TextBounds { left: 0, top: self.size.height as i32 - 20, right: self.size.width as i32, bottom: self.size.height as i32 }, default_color: text_color, custom_glyphs: &[] },
            ];
            if self.palette_anim > 0.0 {
                areas.push(TextArea {
                    buffer: &self.palette_buffer,
                    left: screen_width / 2.0 - 195.0,
                    top: 53.0 - (1.0 - self.palette_anim) * 20.0,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: (screen_width / 2.0 - 200.0) as i32,
                        top: 0,
                        right: (screen_width / 2.0 + 200.0) as i32,
                        bottom: screen_height as i32,
                    },
                    default_color: text_color,
                    custom_glyphs: &[],
                });
            }
            if self.hover_anim > 0.0 {
                areas.push(TextArea {
                    buffer: &self.hover_buffer,
                    left: screen_width / 2.0 - 145.0,
                    top: screen_height / 2.0 - 95.0,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: (screen_width / 2.0 - 150.0) as i32,
                        top: (screen_height / 2.0 - 100.0) as i32,
                        right: (screen_width / 2.0 + 150.0) as i32,
                        bottom: (screen_height / 2.0 + 100.0) as i32,
                    },
                    default_color: text_color,
                    custom_glyphs: &[],
                });
            }
            if self.completion_open {
                if let LayoutNode::Leaf(pane) = &self.workspace.root {
                    if let PaneContent::Editor(editor) = &pane.content {
                        let (active_line, active_col) =
                            editor.document.get_line_col(editor.cursor_pos);
                        let start_line_visible = (self.current_scroll_offset / line_height) as usize;
                        let x = editor_layout.location.x + sidebar_layout.location.x + sidebar_layout.size.width + gutter_layout.size.width + (active_col as f32 * char_width);
                        let y = editor_container_layout.location.y
                            + tabs_layout.size.height
                            + ((active_line - start_line_visible) as f32 * line_height)
                            - y_offset
                            + line_height;
                        areas.push(TextArea {
                            buffer: &self.completion_buffer,
                            left: x + 5.0,
                            top: y + 5.0,
                            scale: 1.0,
                            bounds: TextBounds {
                                left: x as i32,
                                top: y as i32,
                                right: (x + 200.0) as i32,
                                bottom: (y + 100.0) as i32,
                            },
                            default_color: text_color,
                            custom_glyphs: &[],
                        });
                    }
                }
            }
            self.text_renderer
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut self.font_system,
                    &mut self.text_atlas,
                    &self.viewport,
                    areas,
                    &mut self.swash_cache,
                )
                .unwrap();
            self.text_renderer
                .render(&self.text_atlas, &self.viewport, &mut render_pass)
                .unwrap();
        }
        self.text_atlas.trim();
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

#[derive(Default)]
struct App {
    state: Option<RenderState>,
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            let window_attributes = Window::default_attributes().with_title("Xcode");
            let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
            let state = pollster::block_on(RenderState::new(window));
            self.state = Some(state);
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let state = match self.state.as_mut() {
            Some(state) => state,
            None => return,
        };
        if state.handle_input(&event) {
            state.window.request_redraw();
            return;
        }
        match event {
            WindowEvent::CloseRequested
            | WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        logical_key: Key::Named(NamedKey::Escape),
                        ..
                    },
                ..
            } => event_loop.exit(),
            WindowEvent::Resized(physical_size) => {
                state.resize(physical_size);
            }
            WindowEvent::RedrawRequested => {
                let _ = state.render();
                state.window.request_redraw();
            }
            _ => {}
        }
    }
}
pub fn run() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
