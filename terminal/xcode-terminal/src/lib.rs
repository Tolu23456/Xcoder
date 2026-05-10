#![deny(warnings)]
use portable_pty::{native_pty_system, PtySize, CommandBuilder};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use vte::{Parser, Perform};

struct TerminalReceiver {
    output: Arc<Mutex<String>>,
}

impl Perform for TerminalReceiver {
    fn print(&mut self, c: char) {
        let mut out = self.output.lock().unwrap();
        out.push(c);
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\n' {
            let mut out = self.output.lock().unwrap();
            out.push('\n');
        }
    }
}

pub struct TerminalEmulator {
    _master: Box<dyn portable_pty::MasterPty>,
    pub output: Arc<Mutex<String>>,
    writer: Box<dyn Write + Send>,
}

impl TerminalEmulator {
    pub fn new() -> Self {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }).unwrap();

        let shell_owned;
        let shell = if cfg!(target_os = "windows") {
            "powershell.exe"
        } else {
            shell_owned = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
            &shell_owned
        };
        let cmd = CommandBuilder::new(shell);
        let _child = pair.slave.spawn_command(cmd).unwrap();

        let mut reader = pair.master.try_clone_reader().unwrap();
        let writer = pair.master.take_writer().unwrap();
        let output = Arc::new(Mutex::new(String::new()));

        let output_clone = output.clone();
        thread::spawn(move || {
            let mut statemachine = Parser::new();
            let mut receiver = TerminalReceiver { output: output_clone };
            let mut buf = [0u8; 1024];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 { break; }
                statemachine.advance(&mut receiver, &buf[..n]);
            }
        });

        Self {
            _master: pair.master,
            output,
            writer,
        }
    }

    pub fn write(&mut self, text: &str) {
        let _ = self.writer.write_all(text.as_bytes());
    }
}
