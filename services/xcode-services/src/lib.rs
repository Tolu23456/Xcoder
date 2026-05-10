#![deny(warnings)]
#![allow(dead_code)]
use tree_sitter::{Parser, Language, Tree};
use std::process::{Command, Stdio, Child};
use std::io::{BufReader, BufRead, Write, Read};
use serde::{Serialize, Deserialize};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::thread;

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

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcRequest<T> {
    jsonrpc: String,
    id: Option<u64>,
    method: String,
    params: T,
}

pub struct LspClient {
    _child: Child,
    stdin: std::process::ChildStdin,
    next_id: u64,
    responses: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl LspClient {
    pub fn new(command: &str) -> Result<Self> {
        let mut child = Command::new(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let responses = Arc::new(Mutex::new(Vec::new()));
        let responses_clone = responses.clone();

        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line.is_empty() { break; }
                if line.starts_with("Content-Length:") {
                    let len_str = line["Content-Length:".len()..].trim();
                    if let Ok(len) = len_str.parse::<usize>() {
                        let mut blank_line = String::new();
                        let _ = reader.read_line(&mut blank_line);
                        let mut body = vec![0u8; len];
                        if reader.read_exact(&mut body).is_ok() {
                            if let Ok(response) = serde_json::from_slice::<serde_json::Value>(&body) {
                                responses_clone.lock().unwrap().push(response);
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            _child: child,
            stdin,
            next_id: 1,
            responses,
        })
    }

    pub fn send_request<T: Serialize>(&mut self, method: &str, params: T) -> Result<u64> {
        let id = self.next_id;
        self.next_id += 1;
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            method: method.to_string(),
            params,
        };
        self.send_raw(request)?;
        Ok(id)
    }

    pub fn send_notification<T: Serialize>(&mut self, method: &str, params: T) -> Result<()> {
        let request: JsonRpcRequest<T> = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.to_string(),
            params,
        };
        self.send_raw(request)
    }

    fn send_raw<T: Serialize>(&mut self, request: T) -> Result<()> {
        let body = serde_json::to_string(&request)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin.write_all(header.as_bytes())?;
        self.stdin.write_all(body.as_bytes())?;
        self.stdin.flush()?;
        Ok(())
    }

    pub fn get_response(&self, id: u64) -> Option<serde_json::Value> {
        let mut responses = self.responses.lock().unwrap();
        if let Some(pos) = responses.iter().position(|r| r["id"] == id) {
            return Some(responses.remove(pos));
        }
        None
    }

    pub fn initialize(&mut self, root_uri: &str) -> Result<u64> {
        self.send_request("initialize", serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "definition": { "dynamicRegistration": true },
                    "synchronization": { "didSave": true, "dynamicRegistration": true }
                }
            }
        }))
    }

    pub fn did_open(&mut self, uri: &str, text: &str) -> Result<()> {
        self.send_notification("textDocument/didOpen", serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": "rust",
                "version": 1,
                "text": text
            }
        }))
    }

    pub fn did_change(&mut self, uri: &str, version: i32, text: &str) -> Result<()> {
        self.send_notification("textDocument/didChange", serde_json::json!({
            "textDocument": {
                "uri": uri,
                "version": version
            },
            "contentChanges": [{ "text": text }]
        }))
    }

    pub fn goto_definition(&mut self, uri: &str, line: usize, character: usize) -> Result<u64> {
        self.send_request("textDocument/definition", serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        }))
    }

    pub fn hover(&mut self, uri: &str, line: usize, character: usize) -> Result<u64> {
        self.send_request("textDocument/hover", serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        }))
    }
}
