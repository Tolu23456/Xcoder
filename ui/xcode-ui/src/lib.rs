use xcode_core::Document;
use std::path::PathBuf;

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
        std::fs::write(&self.path, self.document.to_string())
    }
}

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
    next_pane_id: usize,
}

impl Workspace {
    pub fn new_home() -> Self {
        let mut ws = Self {
            root: LayoutNode::Leaf(Pane {
                content: PaneContent::Home,
                focused: true,
                id: 0,
            }),
            files: Vec::new(),
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
                
                // Skip hidden files
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
        // Sort: directories first, then alphabetical
        self.files.sort_by(|a, b| {
            if a.is_dir != b.is_dir {
                b.is_dir.cmp(&a.is_dir)
            } else {
                a.name.cmp(&b.name)
            }
        });
    }

    pub fn open_editor(&mut self, path: PathBuf) {
        let editor = EditorState::new(path);
        self.root = LayoutNode::Leaf(Pane {
            content: PaneContent::Editor(editor),
            focused: true,
            id: self.next_pane_id,
        });
        self.next_pane_id += 1;
    }

    pub fn split(&mut self, _direction: SplitDirection) {
        if let LayoutNode::Leaf(_pane) = &self.root {
            // Placeholder for split logic
            self.next_pane_id += 1;
        }
    }
}
