#![deny(warnings)]
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Original,
    Added,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub source: Source,
    pub start: usize,
    pub length: usize,
}

#[derive(Clone, Debug)]
pub struct PieceTable {
    original: String,
    added: String,
    pieces: Vec<Piece>,
    undo_stack: Vec<Vec<Piece>>,
    redo_stack: Vec<Vec<Piece>>,
}

impl PieceTable {
    pub fn new(content: String) -> Self {
        let length = content.len();
        let pieces = if length > 0 {
            vec![Piece {
                source: Source::Original,
                start: 0,
                length,
            }]
        } else {
            vec![]
        };

        Self {
            original: content,
            added: String::new(),
            pieces,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.pieces.iter().map(|p| p.length).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    fn save_state(&mut self) {
        self.undo_stack.push(self.pieces.clone());
        self.redo_stack.clear();
        if self.undo_stack.len() > 1000 {
            self.undo_stack.remove(0);
        }
    }

    pub fn insert(&mut self, offset: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        // Ensure offset is a char boundary
        let content = self.collect_content();
        if !content.is_char_boundary(offset) {
            return;
        }

        self.save_state();
        let added_start = self.added.len();
        self.added.push_str(text);
        let new_piece = Piece {
            source: Source::Added,
            start: added_start,
            length: text.len(),
        };

        if self.pieces.is_empty() {
            self.pieces.push(new_piece);
            return;
        }

        let (piece_idx, offset_in_piece) = self.find_piece_at_offset(offset);

        if piece_idx == self.pieces.len() {
            self.pieces.push(new_piece);
        } else if offset_in_piece == 0 {
            self.pieces.insert(piece_idx, new_piece);
        } else {
            let piece = self.pieces[piece_idx];
            let left = Piece {
                source: piece.source,
                start: piece.start,
                length: offset_in_piece,
            };
            let right = Piece {
                source: piece.source,
                start: piece.start + offset_in_piece,
                length: piece.length - offset_in_piece,
            };

            self.pieces[piece_idx] = left;
            self.pieces.insert(piece_idx + 1, new_piece);
            self.pieces.insert(piece_idx + 2, right);
        }
    }

    pub fn delete(&mut self, range: Range<usize>) {
        if range.start >= range.end || range.start >= self.len() {
            return;
        }

        let content = self.collect_content();
        if !content.is_char_boundary(range.start) || !content.is_char_boundary(range.end) {
            return;
        }

        self.save_state();

        let start = range.start;
        let end = range.end.min(self.len());

        let mut new_pieces = Vec::new();
        let mut current_offset = 0;

        for piece in &self.pieces {
            let piece_end = current_offset + piece.length;

            if piece_end <= start || current_offset >= end {
                new_pieces.push(*piece);
            } else {
                if current_offset < start {
                    new_pieces.push(Piece {
                        source: piece.source,
                        start: piece.start,
                        length: start - current_offset,
                    });
                }

                if piece_end > end {
                    new_pieces.push(Piece {
                        source: piece.source,
                        start: piece.start + (end - current_offset),
                        length: piece_end - end,
                    });
                }
            }
            current_offset = piece_end;
        }
        self.pieces = new_pieces;
    }

    fn find_piece_at_offset(&self, offset: usize) -> (usize, usize) {
        let mut current_offset = 0;
        for (i, piece) in self.pieces.iter().enumerate() {
            if offset >= current_offset && offset < current_offset + piece.length {
                return (i, offset - current_offset);
            }
            current_offset += piece.length;
        }
        (self.pieces.len(), 0)
    }

    pub fn undo(&mut self) -> bool {
        if let Some(state) = self.undo_stack.pop() {
            self.redo_stack.push(self.pieces.clone());
            self.pieces = state;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(state) = self.redo_stack.pop() {
            self.undo_stack.push(self.pieces.clone());
            self.pieces = state;
            true
        } else {
            false
        }
    }

    pub fn collect_content(&self) -> String {
        let mut content = String::with_capacity(self.len());
        for piece in &self.pieces {
            let source_str = match piece.source {
                Source::Original => &self.original,
                Source::Added => &self.added,
            };
            content.push_str(&source_str[piece.start..piece.start + piece.length]);
        }
        content
    }

    pub fn search(&self, query: &str) -> Vec<Range<usize>> {
        let content = self.collect_content();
        content.match_indices(query).map(|(i, s)| i..i+s.len()).collect()
    }

    pub fn replace_all(&mut self, query: &str, replacement: &str) {
        let matches = self.search(query);
        for range in matches.into_iter().rev() {
            self.delete(range.clone());
            self.insert(range.start, replacement);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub position: usize,
    pub selection_anchor: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub buffer: PieceTable,
    pub cursors: Vec<Cursor>,
}

impl Document {
    pub fn new() -> Self {
        Self::from_str("")
    }

    pub fn from_str(text: &str) -> Self {
        Self {
            buffer: PieceTable::new(text.to_string()),
            cursors: vec![Cursor { position: 0, selection_anchor: None }],
        }
    }

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.buffer.insert(char_idx, text);
    }

    pub fn remove(&mut self, range: Range<usize>) {
        self.buffer.delete(range);
    }

    pub fn insert_at_cursors(&mut self, text: &str) {
        self.cursors.sort_by(|a, b| b.position.cmp(&a.position));
        for i in 0..self.cursors.len() {
            let mut cursor = self.cursors[i];
            if let Some(anchor) = cursor.selection_anchor {
                let range = if anchor < cursor.position { anchor..cursor.position } else { cursor.position..anchor };
                self.buffer.delete(range.clone());
                cursor.position = range.start;
                cursor.selection_anchor = None;
            }
            self.buffer.insert(cursor.position, text);
            cursor.position += text.len();
            self.cursors[i] = cursor;
        }
    }

    pub fn delete_at_cursors(&mut self) {
        self.cursors.sort_by(|a, b| b.position.cmp(&a.position));
        let content = self.buffer.collect_content();
        for i in 0..self.cursors.len() {
            let mut cursor = self.cursors[i];
            if let Some(anchor) = cursor.selection_anchor {
                let range = if anchor < cursor.position { anchor..cursor.position } else { cursor.position..anchor };
                self.buffer.delete(range.clone());
                cursor.position = range.start;
                cursor.selection_anchor = None;
            } else if cursor.position > 0 {
                let prev = self.find_prev_char_boundary(&content, cursor.position);
                self.buffer.delete(prev..cursor.position);
                cursor.position = prev;
            }
            self.cursors[i] = cursor;
        }
    }

    fn find_prev_char_boundary(&self, content: &str, pos: usize) -> usize {
        let mut current = pos;
        while current > 0 {
            current -= 1;
            if content.is_char_boundary(current) {
                return current;
            }
        }
        0
    }

    pub fn add_cursor(&mut self, position: usize) {
        self.cursors.push(Cursor { position, selection_anchor: None });
    }

    pub fn to_string(&self) -> String {
        self.buffer.collect_content()
    }

    pub fn len_chars(&self) -> usize {
        self.buffer.len()
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_piece_table_basic() {
        let mut pt = PieceTable::new("Hello World".to_string());
        pt.insert(5, ",");
        assert_eq!(pt.collect_content(), "Hello, World");
        pt.delete(5..6);
        assert_eq!(pt.collect_content(), "Hello World");
    }

    #[test]
    fn test_undo_redo() {
        let mut pt = PieceTable::new("Hello".to_string());
        pt.insert(5, " World");
        assert_eq!(pt.collect_content(), "Hello World");
        pt.undo();
        assert_eq!(pt.collect_content(), "Hello");
        pt.redo();
        assert_eq!(pt.collect_content(), "Hello World");
    }

    #[test]
    fn test_multi_cursor() {
        let mut doc = Document::from_str("A B C");
        doc.cursors = vec![
            Cursor { position: 0, selection_anchor: None },
            Cursor { position: 2, selection_anchor: None },
            Cursor { position: 4, selection_anchor: None },
        ];
        doc.insert_at_cursors("!");
        assert_eq!(doc.to_string(), "!A !B !C");
    }
}
