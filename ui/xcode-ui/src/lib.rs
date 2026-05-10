#![deny(warnings)]
use xcode_core::Document;
use std::path::{PathBuf};
use taffy::prelude::*;

#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: 1.0,
        }
    }
}

pub struct Theme {
    pub background: Color,
    pub sidebar_background: Color,
    pub activity_bar_background: Color,
    pub status_bar_background: Color,
    pub editor_background: Color,
    pub cursor_color: Color,
    pub selection_color: Color,
    pub text_default: Color,
    pub keyword: Color,
    pub function: Color,
    pub string: Color,
    pub comment: Color,
    pub active_line_bg: Color,
}

impl Theme {
    pub fn one_dark() -> Self {
        Self {
            background: Color::rgb(33, 37, 43),
            sidebar_background: Color::rgb(33, 37, 43),
            activity_bar_background: Color::rgb(40, 44, 52),
            status_bar_background: Color::rgb(33, 37, 43),
            editor_background: Color::rgb(40, 44, 52),
            cursor_color: Color::rgb(82, 139, 255),
            selection_color: Color::rgb(62, 68, 81),
            text_default: Color::rgb(171, 178, 191),
            keyword: Color::rgb(198, 120, 221),
            function: Color::rgb(97, 175, 239),
            string: Color::rgb(152, 195, 121),
            comment: Color::rgb(92, 99, 112),
            active_line_bg: Color::rgb(44, 49, 58),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EditorState {
    pub document: Document,
    pub path: PathBuf,
    pub cursor_pos: usize,
    pub scroll_offset: f32,
}

impl EditorState {
    pub fn new(path: PathBuf) -> Self {
        let mut document = Document::new();
        if path.exists() && path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                document = Document::from_str(&content);
            }
        }
        Self {
            document,
            path,
            cursor_pos: 0,
            scroll_offset: 0.0,
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let mut temp_path = self.path.clone();
        temp_path.set_extension("tmp");

        let file = std::fs::File::create(&temp_path)?;
        self.document.buffer.stream_write(file)?;

        std::fs::rename(&temp_path, &self.path)?;
        Ok(())
    }
}

#[derive(Clone)]
pub enum PaneContent {
    Editor(EditorState),
    Terminal,
    Home,
}

pub struct Pane {
    pub content: PaneContent,
    pub focused: bool,
    pub id: usize,
}

#[derive(Clone, Copy)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

pub enum LayoutNode {
    Leaf(Pane),
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

#[derive(Debug, Clone)]
pub struct FileItem {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub struct Workspace {
    pub root: LayoutNode,
    pub files: Vec<FileItem>,
    pub open_editors: Vec<EditorState>,
    pub active_editor_idx: Option<usize>,
    pub taffy: TaffyTree<()>,
    pub layout_root: NodeId,
    next_pane_id: usize,
}

impl Workspace {
    pub fn new_home() -> Self {
        let mut taffy: TaffyTree<()> = TaffyTree::new();
        let layout_root = taffy.new_leaf(Style {
            size: Size { width: length(100.0), height: length(100.0) },
            ..Default::default()
        }).unwrap();

        let mut ws = Self {
            root: LayoutNode::Leaf(Pane {
                content: PaneContent::Home,
                focused: true,
                id: 0,
            }),
            files: Vec::new(),
            open_editors: Vec::new(),
            active_editor_idx: None,
            taffy,
            layout_root,
            next_pane_id: 1,
        };
        ws.refresh_files();
        ws
    }

    pub fn refresh_files(&mut self) {
        self.files.clear();
        if let Ok(entries) = std::fs::read_dir(".") {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string();
                
                if name.starts_with('.') {
                    continue;
                }

                self.files.push(FileItem {
                    name,
                    is_dir: path.is_dir(),
                    path,
                });
            }
        }
        self.files.sort_by(|a, b| {
            if a.is_dir != b.is_dir {
                b.is_dir.cmp(&a.is_dir)
            } else {
                a.name.cmp(&b.name)
            }
        });
    }

    pub fn open_editor(&mut self, path: PathBuf) {
        if let Some(pos) = self.open_editors.iter().position(|e| e.path == path) {
            self.active_editor_idx = Some(pos);
        } else {
            let editor = EditorState::new(path);
            self.open_editors.push(editor.clone());
            self.active_editor_idx = Some(self.open_editors.len() - 1);
        }

        if let Some(idx) = self.active_editor_idx {
            let editor = self.open_editors[idx].clone();
            // Update the focused leaf node in the layout tree
            self.update_active_pane_content(PaneContent::Editor(editor));
        }
        self.next_pane_id += 1;
    }

    fn update_active_pane_content(&mut self, content: PaneContent) {
        fn recurse(node: &mut LayoutNode, content: PaneContent) -> bool {
            match node {
                LayoutNode::Leaf(pane) if pane.focused => {
                    pane.content = content;
                    true
                }
                LayoutNode::Split { first, second, .. } => {
                    recurse(first, content.clone()) || recurse(second, content)
                }
                _ => false,
            }
        }
        if !recurse(&mut self.root, content.clone()) {
            // If no pane is focused, just replace root for now (fallback)
            if let PaneContent::Editor(editor) = content {
               self.root = LayoutNode::Leaf(Pane {
                   content: PaneContent::Editor(editor),
                   focused: true,
                   id: 0,
               });
            }
        }
    }

    pub fn split(&mut self, direction: SplitDirection) {
        fn recurse(node: &mut LayoutNode, direction: SplitDirection, next_id: &mut usize) -> Option<LayoutNode> {
            match node {
                LayoutNode::Leaf(pane) if pane.focused => {
                    let first = LayoutNode::Leaf(Pane {
                        content: pane.content.clone(),
                        focused: true,
                        id: pane.id,
                    });
                    let second = LayoutNode::Leaf(Pane {
                        content: pane.content.clone(), // Duplicate for now
                        focused: false,
                        id: *next_id,
                    });
                    *next_id += 1;
                    Some(LayoutNode::Split {
                        direction,
                        ratio: 0.5,
                        first: Box::new(first),
                        second: Box::new(second),
                    })
                }
                LayoutNode::Split { first, second, .. } => {
                    if let Some(new_first) = recurse(first, direction, next_id) {
                        *first = Box::new(new_first);
                        None
                    } else if let Some(new_second) = recurse(second, direction, next_id) {
                        *second = Box::new(new_second);
                        None
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        let mut next_id = self.next_pane_id;
        if let Some(new_root) = recurse(&mut self.root, direction, &mut next_id) {
            self.root = new_root;
        }
        self.next_pane_id = next_id;
    }

    pub fn close_active_editor(&mut self) {
        if let Some(idx) = self.active_editor_idx {
            self.open_editors.remove(idx);
            if self.open_editors.is_empty() {
                self.active_editor_idx = None;
                self.root = LayoutNode::Leaf(Pane {
                    content: PaneContent::Home,
                    focused: true,
                    id: 0,
                });
            } else {
                let new_idx = idx.min(self.open_editors.len() - 1);
                self.active_editor_idx = Some(new_idx);
                let editor = self.open_editors[new_idx].clone();
                self.root = LayoutNode::Leaf(Pane {
                    content: PaneContent::Editor(editor),
                    focused: true,
                    id: self.next_pane_id,
                });
            }
        }
    }
}
