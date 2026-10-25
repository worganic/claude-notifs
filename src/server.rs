//! Serveur HTTP local (127.0.0.1 uniquement) qui reçoit les événements des hooks.

use crate::UserEvent;
use std::io::Read;
use tao::event_loop::EventLoopProxy;
use tiny_http::{Method, Response, Server};

pub fn start(server: Server, proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let reply = match (req.method(), req.url()) {
                (Method::Get, "/ping") => "claude-notifs",
                (Method::Post, "/event") => {
                    let mut body = String::new();
                    let _ = req.as_reader().take(1 << 20).read_to_string(&mut body);
                    let _ = proxy.send_event(UserEvent::Hook(body));
                    "ok"
                }
                (Method::Post, "/show-admin") => {
                    let _ = proxy.send_event(UserEvent::ShowAdmin);
                    "ok"
                }
                _ => {
                    let _ = req.respond(Response::from_string("not found").with_status_code(404));
                    continue;
                }
            };
            let _ = req.respond(Response::from_string(reply));
        }
    });
}
