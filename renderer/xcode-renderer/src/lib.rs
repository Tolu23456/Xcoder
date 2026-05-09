use std::sync::Arc;
use std::path::PathBuf;
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
    keyboard::{Key, NamedKey, ModifiersState},
};
use glyphon::{
    FontSystem, SwashCache, TextAtlas, TextRenderer, TextArea, TextBounds,
    Cache, Metrics, Family, Attrs, Shaping, Buffer, Viewport, Resolution, Color
};
use xcode_ui::{Workspace, LayoutNode, PaneContent};

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
    
    // UI Elements
    main_text_buffer: Buffer,
    sidebar_buffer: Buffer,
    tabs_buffer: Buffer,

    // Workspace State
    workspace: Workspace,
    modifiers: ModifiersState,
    
    // Animation/Cursor
    start_time: Instant,
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
        viewport.update(&queue, Resolution { width: size.width, height: size.height });
        
        let main_text_buffer = Buffer::new(&mut font_system, Metrics::new(14.0, 20.0));
        let sidebar_buffer = Buffer::new(&mut font_system, Metrics::new(12.0, 18.0));
        let tabs_buffer = Buffer::new(&mut font_system, Metrics::new(12.0, 24.0));
        
        let workspace = Workspace::new_home();

        let mut state = Self {
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
            main_text_buffer,
            sidebar_buffer,
            tabs_buffer,
            workspace,
            modifiers: ModifiersState::default(),
            start_time: Instant::now(),
        };
        state.update_ui_buffers();
        state
    }

    fn update_ui_buffers(&mut self) {
        // 1. Sidebar (Real File Tree)
        let mut sidebar_text = String::from(" PROJECT\n ───────\n");
        for file in &self.workspace.files {
            let icon = if file.is_dir { "📁" } else { "📄" };
            sidebar_text.push_str(&format!(" {} {}\n", icon, file.name));
        }

        self.sidebar_buffer.set_text(
            &mut self.font_system,
            &sidebar_text,
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
            None
        );
        self.sidebar_buffer.set_size(&mut self.font_system, Some(200.0), Some(self.size.height as f32));

        // 2. Tabs
        self.tabs_buffer.set_text(
            &mut self.font_system,
            "  main.rs  |  lib.rs  |  Cargo.toml  ",
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
            None
        );
        self.tabs_buffer.set_size(&mut self.font_system, Some(self.size.width as f32), Some(30.0));

        // 3. Main Content
        if let LayoutNode::Leaf(pane) = &self.workspace.root {
            match &pane.content {
                PaneContent::Home => {
                    let home_text = "\n\n      [ Xcode ]\n\n   Welcome to Xcode\n\n   - Press Ctrl+N for New File\n   - Press Esc to Exit\n\n   (Offline-First Edition)";
                    self.main_text_buffer.set_text(
                        &mut self.font_system,
                        home_text,
                        &Attrs::new().family(Family::Monospace).weight(glyphon::Weight::BOLD),
                        Shaping::Advanced,
                        None
                    );
                }
                PaneContent::Editor(editor) => {
                    let mut text_with_cursor = editor.document.to_string();
                    
                    // Simple blinking cursor logic
                    let show_cursor = (self.start_time.elapsed().as_millis() / 500) % 2 == 0;
                    if show_cursor {
                         if editor.cursor_pos <= text_with_cursor.len() {
                             text_with_cursor.insert(editor.cursor_pos, '|');
                         } else {
                             text_with_cursor.push('|');
                         }
                    } else {
                         if editor.cursor_pos <= text_with_cursor.len() {
                             text_with_cursor.insert(editor.cursor_pos, ' ');
                         } else {
                             text_with_cursor.push(' ');
                         }
                    }

                    // Add line numbers
                    let mut final_text = String::new();
                    for (i, line) in text_with_cursor.lines().enumerate() {
                        final_text.push_str(&format!("{:>3} │ {}\n", i + 1, line));
                    }
                    if text_with_cursor.ends_with('\n') {
                        final_text.push_str(&format!("{:>3} │ \n", text_with_cursor.lines().count() + 1));
                    }

                    self.main_text_buffer.set_text(
                        &mut self.font_system,
                        &final_text,
                        &Attrs::new().family(Family::Monospace),
                        Shaping::Advanced,
                        None
                    );
                }
                _ => {}
            }
        }
        self.main_text_buffer.shape_until_scroll(&mut self.font_system, false);
        self.sidebar_buffer.shape_until_scroll(&mut self.font_system, false);
        self.tabs_buffer.shape_until_scroll(&mut self.font_system, false);
    }

    pub fn handle_input(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
                return true;
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                if key_event.state == ElementState::Pressed {
                    // Global Shortcuts
                    if self.modifiers.control_key() && matches!(key_event.logical_key, Key::Character(ref c) if c == "n") {
                        self.workspace.open_editor(PathBuf::from("untitled"));
                        self.update_ui_buffers();
                        return true;
                    }
                    
                    if self.modifiers.control_key() && matches!(key_event.logical_key, Key::Character(ref c) if c == "s") {
                        if let LayoutNode::Leaf(pane) = &self.workspace.root {
                            if let PaneContent::Editor(editor) = &pane.content {
                                let _ = editor.save();
                            }
                        }
                        return true;
                    }

                    // Pane Content Shortcuts
                    if let LayoutNode::Leaf(pane) = &mut self.workspace.root {
                        if let PaneContent::Editor(editor) = &mut pane.content {
                            match &key_event.logical_key {
                                Key::Named(NamedKey::Backspace) => {
                                    if editor.cursor_pos > 0 {
                                        editor.document.remove(editor.cursor_pos - 1, editor.cursor_pos);
                                        editor.cursor_pos -= 1;
                                        self.update_ui_buffers();
                                    }
                                    return true;
                                }
                                Key::Named(NamedKey::Delete) => {
                                    if editor.cursor_pos < editor.document.len_chars() {
                                        editor.document.remove(editor.cursor_pos, editor.cursor_pos + 1);
                                        self.update_ui_buffers();
                                    }
                                    return true;
                                }
                                Key::Named(NamedKey::ArrowLeft) => {
                                    if editor.cursor_pos > 0 {
                                        editor.cursor_pos -= 1;
                                        self.update_ui_buffers();
                                    }
                                    return true;
                                }
                                Key::Named(NamedKey::ArrowRight) => {
                                    if editor.cursor_pos < editor.document.len_chars() {
                                        editor.cursor_pos += 1;
                                        self.update_ui_buffers();
                                    }
                                    return true;
                                }
                                Key::Named(NamedKey::Enter) => {
                                    editor.document.insert(editor.cursor_pos, "\n");
                                    editor.cursor_pos += 1;
                                    self.update_ui_buffers();
                                    return true;
                                }
                                Key::Named(NamedKey::Space) => {
                                    editor.document.insert(editor.cursor_pos, " ");
                                    editor.cursor_pos += 1;
                                    self.update_ui_buffers();
                                    return true;
                                }
                                Key::Character(text) => {
                                    if !text.chars().any(|c| c.is_control()) {
                                        editor.document.insert(editor.cursor_pos, text);
                                        editor.cursor_pos += text.len();
                                        self.update_ui_buffers();
                                        return true;
                                    }
                                }
                                _ => {}
                            }
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
            
            self.viewport.update(&self.queue, Resolution { width: new_size.width, height: new_size.height });
            self.update_ui_buffers();
        }
    }

    pub fn render(&mut self) -> Result<(), &'static str> {
        self.update_ui_buffers();

        self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.text_atlas,
            &self.viewport,
            [
                TextArea {
                    buffer: &self.sidebar_buffer,
                    left: 0.0,
                    top: 0.0,
                    scale: 1.0,
                    bounds: TextBounds { left: 0, top: 0, right: 200, bottom: self.size.height as i32 },
                    default_color: Color::rgb(150, 150, 150),
                    custom_glyphs: &[],
                },
                TextArea {
                    buffer: &self.tabs_buffer,
                    left: 200.0,
                    top: 0.0,
                    scale: 1.0,
                    bounds: TextBounds { left: 200, top: 0, right: self.size.width as i32, bottom: 30 },
                    default_color: Color::rgb(255, 255, 255),
                    custom_glyphs: &[],
                },
                TextArea {
                    buffer: &self.main_text_buffer,
                    left: 210.0,
                    top: 40.0,
                    scale: 1.0,
                    bounds: TextBounds { left: 210, top: 40, right: self.size.width as i32, bottom: self.size.height as i32 },
                    default_color: Color::rgb(200, 200, 200),
                    custom_glyphs: &[],
                }
            ],
            &mut self.swash_cache,
        ).unwrap();

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            _ => return Err("Surface Error"),
        };
        
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.02, b: 0.02, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.text_renderer.render(&self.text_atlas, &self.viewport, &mut render_pass).unwrap();
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
                event: KeyEvent { state: ElementState::Pressed, logical_key: Key::Named(NamedKey::Escape), .. }, ..
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
