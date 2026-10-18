//! Discord's local RPC: a Unix socket the desktop client listens on, framed as
//! a little-endian opcode and length followed by JSON.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;

/// Where each Discord build puts its socket: the native client, then the
/// Flatpak and Snap ones.
fn sockets() -> Vec<PathBuf> {
    let runtime = crate::paths::runtime_dir();
    let mut dirs = vec![
        runtime.clone(),
        runtime.join("app/com.discordapp.Discord"),
        runtime.join("app/com.discordapp.DiscordCanary"),
        runtime.join("snap.discord"),
    ];
    dirs.push(std::env::temp_dir());
    dirs.iter()
        .flat_map(|dir| (0..10).map(move |n| dir.join(format!("discord-ipc-{n}"))))
        .filter(|path| path.exists())
        .collect()
}

pub struct Client {
    app_id: String,
    stream: Option<UnixStream>,
    nonce: u64,
}

impl Client {
    pub fn new(app_id: &str) -> Client {
        Client {
            app_id: app_id.to_string(),
            stream: None,
            nonce: 0,
        }
    }

    fn send(stream: &mut UnixStream, op: u32, payload: &Value) -> std::io::Result<()> {
        let body = serde_json::to_vec(payload)?;
        let mut frame = Vec::with_capacity(8 + body.len());
        frame.extend_from_slice(&op.to_le_bytes());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(&body);
        stream.write_all(&frame)
    }

    fn receive(stream: &mut UnixStream) -> std::io::Result<(u32, Value)> {
        let mut header = [0u8; 8];
        stream.read_exact(&mut header)?;
        let op = u32::from_le_bytes(header[..4].try_into().unwrap());
        let length = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
        if length > 1 << 20 {
            return Err(std::io::Error::other("oversized frame"));
        }
        let mut body = vec![0u8; length];
        stream.read_exact(&mut body)?;
        Ok((op, serde_json::from_slice(&body).unwrap_or(Value::Null)))
    }

    fn connect(&mut self) -> bool {
        if self.stream.is_some() {
            return true;
        }
        for path in sockets() {
            let Ok(mut stream) = UnixStream::connect(&path) else {
                continue;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let hello = json!({ "v": 1, "client_id": self.app_id });
            if Self::send(&mut stream, HANDSHAKE, &hello).is_err() {
                continue;
            }
            match Self::receive(&mut stream) {
                Ok((FRAME, answer)) if answer["evt"] == "READY" => {
                    self.stream = Some(stream);
                    return true;
                }
                _ => continue,
            }
        }
        false
    }

    /// Show `activity`, or clear it with `None`. A Discord that is not running,
    /// or went away, is not an error: the next call tries again.
    pub fn set_activity(&mut self, activity: Option<Value>) {
        if self.app_id.is_empty() || !self.connect() {
            return;
        }
        self.nonce += 1;
        let payload = json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": std::process::id(), "activity": activity },
            "nonce": self.nonce.to_string(),
        });
        let stream = self.stream.as_mut().unwrap();
        let ok = Self::send(stream, FRAME, &payload).is_ok()
            && match Self::receive(stream) {
                Ok((_, answer)) => {
                    if std::env::var_os("GINROSE_DEBUG").is_some() {
                        eprintln!(
                            "ginrose: discord answered {}",
                            answer["evt"].as_str().unwrap_or("ok")
                        );
                        if answer["evt"] == "ERROR" {
                            eprintln!("ginrose: {}", answer["data"]);
                        }
                    }
                    true
                }
                Err(_) => false,
            };
        if !ok {
            self.stream = None;
        }
    }
}
