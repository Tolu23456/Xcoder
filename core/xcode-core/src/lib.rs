#![deny(warnings)]
use std::ops::Range;
use std::time::Instant;
use std::io::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Source {
    Original,
    Added,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Piece {
    pub source: Source,
    pub start: usize,
    pub length: usize,
    pub line_starts: Vec<usize>,
}

impl Piece {
    pub fn line_breaks(&self) -> usize {
        self.line_starts.len()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EditorStateSnapshot {
    pub pieces: Vec<Piece>,
    pub cursors: Vec<Cursor>,
    pub group_id: u64,
}

#[derive(Clone, Debug)]
pub struct PieceTable {
    pub original: String,
    pub added: String,
    pub pieces: Vec<Piece>,
    pub undo_stack: Vec<EditorStateSnapshot>,
    pub redo_stack: Vec<EditorStateSnapshot>,
    pub last_op_time: Instant,
    pub current_group_id: u64,
}

impl PieceTable {
    pub fn new(content: String) -> Self {
        let line_starts = Self::find_line_starts(&content);
        let length = content.len();
        let pieces = if length > 0 {
            vec![Piece {
                source: Source::Original,
                start: 0,
                length,
                line_starts,
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
            last_op_time: Instant::now() - std::time::Duration::from_secs(5),
            current_group_id: 0,
        }
    }

    fn find_line_starts(text: &str) -> Vec<usize> {
        let mut starts = Vec::new();
        for (i, c) in text.char_indices() {
            if c == '\n' {
                starts.push(i + 1);
            }
        }
        starts
    }

    pub fn len(&self) -> usize {
        self.pieces.iter().map(|p| p.length).sum()
    }

    pub fn line_count(&self) -> usize {
        self.pieces.iter().map(|p| p.line_breaks()).sum::<usize>() + 1
    }

    pub fn is_char_boundary(&self, offset: usize) -> bool {
        if offset == 0 || offset == self.len() { return true; }
        if offset > self.len() { return false; }
        let (p_idx, offset_in_p) = self.find_piece_at_offset(offset);
        if p_idx >= self.pieces.len() { return false; }
        let piece = &self.pieces[p_idx];
        let source_str = match piece.source {
            Source::Original => &self.original,
            Source::Added => &self.added,
        };
        source_str.is_char_boundary(piece.start + offset_in_p)
    }

    pub fn save_state(&mut self, cursors: Vec<Cursor>, force_new_group: bool) {
        let now = Instant::now();
        let should_start_new_group = force_new_group || now.duration_since(self.last_op_time).as_secs() >= 2;

        if should_start_new_group {
            self.current_group_id += 1;
        }

        if should_start_new_group || self.undo_stack.is_empty() {
            self.undo_stack.push(EditorStateSnapshot {
                pieces: self.pieces.clone(),
                cursors,
                group_id: self.current_group_id,
            });
            self.redo_stack.clear();
            if self.undo_stack.len() > 1000 {
                self.undo_stack.remove(0);
            }
        }
        self.last_op_time = now;
    }

    pub fn insert(&mut self, offset: usize, text: &str) -> Result<(), &'static str> {
        if text.is_empty() {
            return Ok(());
        }
        if !self.is_char_boundary(offset) {
            return Err("Insert offset is not a char boundary");
        }
        let (piece_idx, offset_in_piece) = self.find_piece_at_offset(offset);
        let added_start = self.added.len();
        self.added.push_str(text);
        let line_starts = Self::find_line_starts(text);
        let new_piece = Piece {
            source: Source::Added,
            start: added_start,
            length: text.len(),
            line_starts,
        };
        if self.pieces.is_empty() {
            self.pieces.push(new_piece);
        } else if piece_idx == self.pieces.len() {
            let last = self.pieces.last_mut().unwrap();
            if last.source == Source::Added && last.start + last.length == added_start {
                last.line_starts.extend(Self::find_line_starts(text).into_iter().map(|ls| ls + last.length));
                last.length += text.len();
            } else {
                self.pieces.push(new_piece);
            }
        } else if offset_in_piece == 0 {
            self.pieces.insert(piece_idx, new_piece);
        } else {
            let piece = self.pieces[piece_idx].clone();
            let mut left_lines = Vec::new();
            let mut right_lines = Vec::new();
            for &ls in &piece.line_starts {
                if ls <= offset_in_piece {
                    left_lines.push(ls);
                } else {
                    right_lines.push(ls - offset_in_piece);
                }
            }
            let left = Piece {
                source: piece.source,
                start: piece.start,
                length: offset_in_piece,
                line_starts: left_lines,
            };
            let right = Piece {
                source: piece.source,
                start: piece.start + offset_in_piece,
                length: piece.length - offset_in_piece,
                line_starts: right_lines,
            };
            self.pieces[piece_idx] = left;
            self.pieces.insert(piece_idx + 1, new_piece);
            self.pieces.insert(piece_idx + 2, right);
        }
        Ok(())
    }

    pub fn delete(&mut self, range: Range<usize>) -> Result<(), &'static str> {
        if range.start >= range.end {
            return Ok(());
        }
        if !self.is_char_boundary(range.start) || !self.is_char_boundary(range.end) {
            return Err("Delete range boundaries are not char boundaries");
        }
        let start = range.start;
        let end = range.end.min(self.len());
        let mut new_pieces = Vec::new();
        let mut current_offset = 0;
        for piece in &self.pieces {
            let piece_end = current_offset + piece.length;
            if piece_end <= start || current_offset >= end {
                new_pieces.push(piece.clone());
            } else {
                if current_offset < start {
                    let len = start - current_offset;
                    let mut lines = Vec::new();
                    for &ls in &piece.line_starts {
                        if ls <= len {
                            lines.push(ls);
                        }
                    }
                    new_pieces.push(Piece {
                        source: piece.source,
                        start: piece.start,
                        length: len,
                        line_starts: lines,
                    });
                }
                if piece_end > end {
                    let split_offset = end - current_offset;
                    let len = piece_end - end;
                    let mut lines = Vec::new();
                    for &ls in &piece.line_starts {
                        if ls > split_offset {
                            lines.push(ls - split_offset);
                        }
                    }
                    new_pieces.push(Piece {
                        source: piece.source,
                        start: piece.start + split_offset,
                        length: len,
                        line_starts: lines,
                    });
                }
            }
            current_offset = piece_end;
        }
        self.pieces = new_pieces;
        Ok(())
    }

    fn find_piece_at_offset(&self, offset: usize) -> (usize, usize) {
        let mut current_offset = 0;
        for (i, piece) in self.pieces.iter().enumerate() {
            if offset >= current_offset && offset <= current_offset + piece.length {
                if offset == current_offset + piece.length && i < self.pieces.len() - 1 {
                } else {
                    return (i, offset - current_offset);
                }
            }
            current_offset += piece.length;
        }
        (self.pieces.len(), 0)
    }

    pub fn undo(&mut self, current_cursors: Vec<Cursor>) -> Option<Vec<Cursor>> {
        if let Some(snapshot) = self.undo_stack.pop() {
            self.redo_stack.push(EditorStateSnapshot {
                pieces: self.pieces.clone(),
                cursors: current_cursors,
                group_id: snapshot.group_id,
            });
            self.pieces = snapshot.pieces;
            self.last_op_time = Instant::now() - std::time::Duration::from_secs(5);
            Some(snapshot.cursors)
        } else {
            None
        }
    }

    pub fn redo(&mut self, current_cursors: Vec<Cursor>) -> Option<Vec<Cursor>> {
        if let Some(snapshot) = self.redo_stack.pop() {
            self.undo_stack.push(EditorStateSnapshot {
                pieces: self.pieces.clone(),
                cursors: current_cursors,
                group_id: snapshot.group_id,
            });
            self.pieces = snapshot.pieces;
            self.last_op_time = Instant::now() - std::time::Duration::from_secs(5);
            Some(snapshot.cursors)
        } else {
            None
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

    pub fn stream_write<W: Write>(&self, mut writer: W) -> std::io::Result<()> {
        for piece in &self.pieces {
            let source_str = match piece.source {
                Source::Original => &self.original,
                Source::Added => &self.added,
            };
            writer.write_all(source_str[piece.start..piece.start + piece.length].as_bytes())?;
        }
        Ok(())
    }

    pub fn search(&self, query: &str, case_sensitive: bool) -> Vec<Range<usize>> {
        let mut results = Vec::new();
        let query_text = if case_sensitive { query.to_string() } else { query.to_lowercase() };
        if query_text.is_empty() { return results; }

        let content = self.collect_content();
        let search_text = if case_sensitive { content } else { content.to_lowercase() };

        let mut start = 0;
        while let Some(pos) = search_text[start..].find(&query_text) {
            let actual_pos = start + pos;
            results.push(actual_pos..actual_pos + query.len());
            start = actual_pos + query.len().max(1);
        }
        results
    }

    pub fn get_line(&self, line_idx: usize) -> Option<String> {
        let mut current_line = 0;
        for (p_idx, piece) in self.pieces.iter().enumerate() {
            let piece_line_breaks = piece.line_breaks();
            if current_line + piece_line_breaks >= line_idx {
                let source_str = match piece.source {
                    Source::Original => &self.original,
                    Source::Added => &self.added,
                };
                let start_in_piece = if line_idx > current_line {
                    piece.line_starts[line_idx - current_line - 1]
                } else {
                    0
                };
                let mut line = String::new();
                line.push_str(&source_str[piece.start + start_in_piece..piece.start + piece.length]);
                let mut next_p = p_idx + 1;
                while !line.contains('\n') && next_p < self.pieces.len() {
                    let np = &self.pieces[next_p];
                    let ns = match np.source {
                        Source::Original => &self.original,
                        Source::Added => &self.added,
                    };
                    line.push_str(&ns[np.start..np.start + np.length]);
                    next_p += 1;
                }
                if let Some(pos) = line.find('\n') {
                    line.truncate(pos + 1);
                }
                return Some(line);
            }
            current_line += piece_line_breaks;
        }
        None
    }

    pub fn get_line_col(&self, pos: usize) -> (usize, usize) {
        let mut current_line = 0;
        let mut current_offset = 0;
        for piece in &self.pieces {
            if current_offset + piece.length >= pos {
                let source_str = match piece.source {
                    Source::Original => &self.original,
                    Source::Added => &self.added,
                };
                let offset_in_piece = pos - current_offset;
                let piece_text = &source_str[piece.start..piece.start + offset_in_piece];
                let line_in_piece = piece_text.chars().filter(|&c| c == '\n').count();
                let col = if line_in_piece > 0 {
                    piece_text.chars().rev().take_while(|&c| c != '\n').count()
                } else {
                    let mut c = piece_text.chars().count();
                    let mut p_idx = self.pieces.iter().position(|p| p as *const _ == piece as *const _).unwrap();
                    while p_idx > 0 {
                        p_idx -= 1;
                        let pp = &self.pieces[p_idx];
                        let ps = match pp.source {
                            Source::Original => &self.original,
                            Source::Added => &self.added,
                        };
                        let pt = &ps[pp.start..pp.start + pp.length];
                        if let Some(idx) = pt.rfind('\n') {
                            c += pt[idx+1..].chars().count();
                            break;
                        } else {
                            c += pt.chars().count();
                        }
                    }
                    c
                };
                return (current_line + line_in_piece, col);
            }
            current_line += piece.line_breaks();
            current_offset += piece.length;
        }
        (self.line_count().saturating_sub(1), 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

    pub fn insert_at_cursors(&mut self, text: &str) -> Vec<(Range<usize>, String)> {
        let mut changes = Vec::new();
        self.buffer.save_state(self.cursors.clone(), false);
        self.cursors.sort_by(|a, b| b.position.cmp(&a.position));
        for i in 0..self.cursors.len() {
            let mut cursor = self.cursors[i];
            if let Some(anchor) = cursor.selection_anchor {
                let range = if anchor < cursor.position { anchor..cursor.position } else { cursor.position..anchor };
                let _ = self.buffer.delete(range.clone());
                changes.push((range.clone(), String::new()));
                cursor.position = range.start;
                cursor.selection_anchor = None;
            }
            let _ = self.buffer.insert(cursor.position, text);
            changes.push((cursor.position..cursor.position, text.to_string()));
            cursor.position += text.len();
            self.cursors[i] = cursor;
        }
        changes
    }

    pub fn delete_at_cursors(&mut self) -> Vec<(Range<usize>, String)> {
        let mut changes = Vec::new();
        self.buffer.save_state(self.cursors.clone(), false);
        self.cursors.sort_by(|a, b| b.position.cmp(&a.position));
        for i in 0..self.cursors.len() {
            let mut cursor = self.cursors[i];
            if let Some(anchor) = cursor.selection_anchor {
                let range = if anchor < cursor.position { anchor..cursor.position } else { cursor.position..anchor };
                let _ = self.buffer.delete(range.clone());
                changes.push((range.clone(), String::new()));
                cursor.position = range.start;
                cursor.selection_anchor = None;
            } else if cursor.position > 0 {
                let prev = self.find_prev_char_boundary(cursor.position);
                let range = prev..cursor.position;
                let _ = self.buffer.delete(range.clone());
                changes.push((range, String::new()));
                cursor.position = prev;
            }
            self.cursors[i] = cursor;
        }
        changes
    }

    pub fn find_prev_char_boundary(&self, pos: usize) -> usize {
        let mut current = pos;
        while current > 0 {
            current -= 1;
            if self.buffer.is_char_boundary(current) {
                return current;
            }
        }
        0
    }

    pub fn find_next_char_boundary(&self, pos: usize) -> usize {
        let mut current = pos;
        let len = self.buffer.len();
        while current < len {
            current += 1;
            if self.buffer.is_char_boundary(current) {
                return current;
            }
        }
        len
    }

    pub fn get_line_col(&self, pos: usize) -> (usize, usize) {
        self.buffer.get_line_col(pos)
    }

    pub fn undo(&mut self) {
        if let Some(cursors) = self.buffer.undo(self.cursors.clone()) {
            self.cursors = cursors;
        }
    }

    pub fn redo(&mut self) {
        if let Some(cursors) = self.buffer.redo(self.cursors.clone()) {
            self.cursors = cursors;
        }
    }

    pub fn to_string(&self) -> String {
        self.buffer.collect_content()
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_piece_table_basic() {
        let mut pt = PieceTable::new("Hello World".to_string());
        pt.insert(5, ",").unwrap();
        assert_eq!(pt.collect_content(), "Hello, World");
        pt.delete(5..6).unwrap();
        assert_eq!(pt.collect_content(), "Hello World");
    }

    #[test]
    fn test_search() {
        let pt = PieceTable::new("The quick brown fox".to_string());
        let results = pt.search("quick", true);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 4..9);
    }

    #[test]
    fn test_streaming_write() {
        let mut pt = PieceTable::new("Hello".to_string());
        pt.insert(5, " World").unwrap();
        let mut output = Vec::new();
        pt.stream_write(&mut output).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "Hello World");
    }
}
