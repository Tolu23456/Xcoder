#![deny(warnings)]
use tree_sitter::{Parser, Language, Tree};

pub struct SyntaxHighlighter {
    parser: Parser,
    tree: Option<Tree>,
}

impl SyntaxHighlighter {
    pub fn new(language: Language) -> Self {
        let mut parser = Parser::new();
        parser.set_language(&language).expect("Error loading language");
        Self {
            parser,
            tree: None,
        }
    }

    pub fn highlight(&mut self, source: &str) -> Vec<(usize, usize, String)> {
        self.tree = self.parser.parse(source, self.tree.as_ref());
        let mut highlights = Vec::new();
        if let Some(tree) = &self.tree {
            let mut cursor = tree.walk();
            self.collect_highlights(&mut cursor, &mut highlights);
        }
        highlights
    }

    fn collect_highlights(&self, cursor: &mut tree_sitter::TreeCursor, highlights: &mut Vec<(usize, usize, String)>) {
        let node = cursor.node();
        if node.child_count() == 0 {
            highlights.push((node.start_byte(), node.end_byte(), node.kind().to_string()));
        }
        if cursor.goto_first_child() {
            self.collect_highlights(cursor, highlights);
            while cursor.goto_next_sibling() {
                self.collect_highlights(cursor, highlights);
            }
            cursor.goto_parent();
        }
    }
}

pub fn get_rust_lang() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}

pub struct LspClient {
    pub is_initialized: bool,
    pub capabilities: LspCapabilities,
}

pub struct LspCapabilities {
    pub hover: bool,
    pub definition: bool,
    pub completion: bool,
}

impl LspClient {
    pub fn new() -> Self {
        Self {
            is_initialized: false,
            capabilities: LspCapabilities {
                hover: true,
                definition: true,
                completion: true,
            },
        }
    }

    pub fn initialize(&mut self) {
        // In a real implementation, this would send an 'initialize' request to the server
        self.is_initialized = true;
    }

    pub fn get_completions(&self, _path: &str, _line: usize, _col: usize) -> Vec<String> {
        if !self.is_initialized { return vec![]; }
        // Return mock completions for demonstration
        vec!["fn".to_string(), "let".to_string(), "pub".to_string(), "use".to_string()]
    }
}

impl Default for LspClient {
    fn default() -> Self {
        Self::new()
    }
}
