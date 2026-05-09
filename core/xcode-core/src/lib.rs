use ropey::Rope;

#[derive(Debug, Default, Clone)]
pub struct Document {
    content: Rope,
}

impl Document {
    pub fn new() -> Self {
        Self {
            content: Rope::new(),
        }
    }

    pub fn from_str(text: &str) -> Self {
        Self {
            content: Rope::from_str(text),
        }
    }

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.content.insert(char_idx, text);
    }

    pub fn remove(&mut self, start_char_idx: usize, end_char_idx: usize) {
        self.content.remove(start_char_idx..end_char_idx);
    }

    pub fn len_chars(&self) -> usize {
        self.content.len_chars()
    }

    pub fn to_string(&self) -> String {
        self.content.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_remove() {
        let mut doc = Document::from_str("Hello World");
        doc.insert(5, ",");
        assert_eq!(doc.to_string(), "Hello, World");
        
        doc.remove(5, 6);
        assert_eq!(doc.to_string(), "Hello World");
    }
}
