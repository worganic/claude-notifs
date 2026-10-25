//! Mode hook : `claude-notifs.exe hook <Event>`, appelé par Claude Code.
//! Lit le JSON du hook sur stdin et le transmet à l'application ; ne bloque jamais Claude Code.

use crate::config::PORT;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::windows::process::CommandExt;
use std::time::{Duration, Instant};

const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

fn addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], PORT))
}

/// Envoie une requête HTTP minimale ; retourne `true` si l'application a répondu.
pub fn send(method: &str, path: &str, body: &str) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&addr(), Duration::from_millis(300)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(800)));
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 64];
    matches!(stream.read(&mut buf), Ok(n) if n > 0)
}

pub fn run(event: &str) {
    // Garde-fou : quoi qu'il arrive, on rend la main à Claude Code.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(5));
        std::process::exit(0);
    });

    let mut raw = String::new();
    let _ = std::io::stdin().take(1 << 20).read_to_string(&mut raw);
    let payload: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
    let body = serde_json::json!({ "event": event, "payload": payload }).to_string();

    if send("POST", "/event", &body) {
        return;
    }

    // L'application ne tourne pas : on la lance en arrière-plan puis on réessaie.
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .arg("--background")
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(4) {
            std::thread::sleep(Duration::from_millis(150));
            if send("POST", "/event", &body) {
                return;
            }
        }
    }
}
