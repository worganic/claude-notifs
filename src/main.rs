#![windows_subsystem = "windows"]
//! claude-notifs : notifications Windows pour Claude Code.
//!
//! - `claude-notifs.exe`                  lance l'application et ouvre l'administration
//! - `claude-notifs.exe --background`     lance l'application sans ouvrir l'administration
//! - `claude-notifs.exe hook <Event>`     appelé par les hooks Claude Code (JSON sur stdin)
//! - `claude-notifs.exe install-hooks`    ajoute la liaison dans ~/.claude/settings.json
//! - `claude-notifs.exe uninstall-hooks`  retire la liaison

mod admin;
mod autostart;
mod claude_settings;
mod config;
mod events;
mod hook;
mod server;
mod sound;
mod toast;
mod tray;

use admin::Admin;
use config::{Config, PORT};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use toast::{ToastData, ToastManager};
use tray::Tray;
use tray_icon::menu::MenuId;
use wry::WebContext;

pub enum UserEvent {
    Hook(String),
    ShowAdmin,
    AdminIpc(String),
    CloseToast(u32),
    ResizeToast(u32, u32),
    Menu(MenuId),
}

/// Notifications qui attendent une action de l'utilisateur dans une session.
const PENDING_KINDS: &[&str] = &["question", "plan", "permission", "idle"];

struct App {
    config: Config,
    toasts: ToastManager,
    admin: Option<Admin>,
    tray: Tray,
    ctx: WebContext,
    proxy: EventLoopProxy<UserEvent>,
    last_event: Option<String>,
    /// Dernière question / plan affiché par session, pour ne pas doubler avec la demande de permission.
    recent_questions: HashMap<String, Instant>,
}

fn local_time() -> String {
    let mut t: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t) };
    format!("{:02}:{:02}:{:02}", t.wHour, t.wMinute, t.wSecond)
}

fn exe_path() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()
}

impl App {
    fn scale(&self, target: &EventLoopWindowTarget<UserEvent>) -> f64 {
        target.primary_monitor().map(|m| m.scale_factor()).unwrap_or(1.0)
    }

    fn show_toast(&mut self, target: &EventLoopWindowTarget<UserEvent>, kind: &str, project: &str, message: &str, session_id: &str) {
        let Some(tc) = self.config.type_config(kind) else { return };
        let data = ToastData {
            kind: kind.to_string(),
            label: tc.label.clone(),
            project: project.to_string(),
            message: message.to_string(),
            color: tc.color.clone(),
            duration: tc.duration,
            blink: tc.blink,
            session_id: session_id.to_string(),
        };
        if tc.sound {
            sound::play_alert();
        }
        self.toasts.show(target, &mut self.ctx, &self.proxy, &self.config, data);
    }

    fn handle_hook(&mut self, target: &EventLoopWindowTarget<UserEvent>, body: &str) {
        let Ok(v) = serde_json::from_str::<Value>(body) else { return };
        let event = v.get("event").and_then(Value::as_str).unwrap_or("");
        let payload = v.get("payload").cloned().unwrap_or(Value::Null);
        let Some(n) = events::map(event, &payload) else { return };

        let project = if n.project.is_empty() { String::new() } else { format!(" ({})", n.project) };
        self.last_event = Some(format!("{} — {}{}", local_time(), event, project));

        // Un nouveau prompt, une fin de réponse ou de session rend les attentes précédentes obsolètes.
        if matches!(n.kind.as_str(), "prompt_start" | "prompt_end" | "session_end") {
            let scale = self.scale(target);
            self.toasts.dismiss_session(&n.session_id, PENDING_KINDS, self.config.position, scale);
        }

        let now = Instant::now();
        self.recent_questions.retain(|_, t| now.duration_since(*t) < Duration::from_secs(10));
        let duplicate = n.kind == "permission"
            && self.recent_questions.get(&n.session_id).is_some_and(|t| now.duration_since(*t) < Duration::from_secs(5));
        if matches!(n.kind.as_str(), "question" | "plan") {
            self.recent_questions.insert(n.session_id.clone(), now);
        }

        let enabled = self.config.type_config(&n.kind).is_some_and(|t| t.enabled);
        if enabled && !self.config.dnd && !duplicate {
            self.show_toast(target, &n.kind, &n.project, &n.message, &n.session_id);
        }
        self.push_state();
    }

    fn show_admin(&mut self, target: &EventLoopWindowTarget<UserEvent>) {
        match &self.admin {
            Some(a) => a.focus(),
            None => self.admin = Admin::open(target, &mut self.ctx, &self.proxy),
        }
    }

    fn push_state(&self) {
        let Some(admin) = &self.admin else { return };
        let state = json!({
            "config": self.config,
            "autostart": autostart::is_enabled(),
            "hooks_installed": claude_settings::is_installed(),
            "settings_path": claude_settings::settings_path().display().to_string(),
            "exe_path": exe_path(),
            "port": PORT,
            "last_event": self.last_event,
        });
        admin.call(&format!("window.onState({state})"));
    }

    fn admin_message(&self, text: &str, ok: bool) {
        if let Some(admin) = &self.admin {
            admin.call(&format!("window.onMessage({}, {ok})", json!(text)));
        }
    }

    fn test_toast(&mut self, target: &EventLoopWindowTarget<UserEvent>, kind: &str) {
        let message = match kind {
            "question" => "Quelle technologie veux-tu utiliser pour l'application ?",
            "plan" => "Un plan attend ta validation",
            "permission" => "Claude a besoin de ta permission pour utiliser Bash",
            "idle" => "Claude attend ta réponse",
            "prompt_end" => "Claude a terminé de répondre",
            "prompt_start" => "crée-moi une mini application Windows…",
            "subagent_end" => "Un sous-agent a terminé",
            "session_start" => "Session démarrée (startup)",
            "session_end" => "Session terminée",
            _ => "Ceci est une notification de test",
        };
        self.show_toast(target, kind, "test", message, "");
    }

    fn set_dnd(&mut self, dnd: bool) {
        self.config.dnd = dnd;
        let _ = self.config.save();
        self.tray.set_dnd(dnd);
    }

    fn handle_admin(&mut self, target: &EventLoopWindowTarget<UserEvent>, body: &str) {
        let Ok(msg) = serde_json::from_str::<Value>(body) else { return };
        match msg.get("cmd").and_then(Value::as_str).unwrap_or("") {
            "ready" => self.push_state(),
            "save" => {
                let Some(cfg) = msg.get("config").and_then(|c| serde_json::from_value::<Config>(c.clone()).ok()) else {
                    return;
                };
                let moved = cfg.position != self.config.position;
                let dnd_changed = cfg.dnd != self.config.dnd;
                self.config = cfg;
                self.config.normalize();
                if self.config.save().is_ok() {
                    if let Some(a) = &self.admin {
                        a.call("window.onSaved()");
                    }
                }
                if dnd_changed {
                    self.tray.set_dnd(self.config.dnd);
                }
                if moved {
                    let scale = self.scale(target);
                    self.toasts.relayout(self.config.position, scale);
                }
            }
            "test" => {
                let kind = msg.get("kind").and_then(Value::as_str).unwrap_or("notification").to_string();
                self.test_toast(target, &kind);
            }
            "test_all" => {
                let kinds: Vec<String> =
                    self.config.types.iter().filter(|t| t.enabled).map(|t| t.key.clone()).collect();
                for kind in kinds.iter().take(self.config.max_visible) {
                    self.test_toast(target, kind);
                }
            }
            "autostart" => {
                let enabled = msg.get("enabled").and_then(Value::as_bool).unwrap_or(false);
                if let Err(e) = autostart::set(enabled) {
                    self.admin_message(&format!("Démarrage automatique : {e}"), false);
                }
                self.push_state();
            }
            "install_hooks" => {
                let res = std::env::current_exe()
                    .map_err(|e| e.to_string())
                    .and_then(|exe| claude_settings::install(&exe));
                match res {
                    Ok(m) => self.admin_message(&format!("{m}. Les nouvelles sessions Claude Code l'utiliseront."), true),
                    Err(e) => self.admin_message(&e, false),
                }
                self.push_state();
            }
            "uninstall_hooks" => {
                match claude_settings::uninstall() {
                    Ok(m) => self.admin_message(&m, true),
                    Err(e) => self.admin_message(&e, false),
                }
                self.push_state();
            }
            _ => {}
        }
    }

    fn handle_menu(&mut self, target: &EventLoopWindowTarget<UserEvent>, id: &MenuId, control_flow: &mut ControlFlow) {
        if id == self.tray.admin.id() {
            self.show_admin(target);
        } else if id == self.tray.test.id() {
            self.test_toast(target, "prompt_end");
        } else if id == self.tray.dnd.id() {
            // La case est déjà cochée/décochée par le menu lui-même.
            let dnd = self.tray.dnd.is_checked();
            self.set_dnd(dnd);
            self.push_state();
        } else if id == self.tray.quit.id() {
            *control_flow = ControlFlow::Exit;
        }
    }
}

/// Pour les commandes en ligne : affiche la sortie dans la console qui a lancé l'exe.
fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn message_box(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    unsafe {
        MessageBoxW(std::ptr::null_mut(), wide(text).as_ptr(), wide("Claude Notifs").as_ptr(), MB_OK | MB_ICONERROR);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("hook") => {
            hook::run(args.get(1).map(String::as_str).unwrap_or(""));
            return;
        }
        Some(cmd @ ("install-hooks" | "uninstall-hooks")) => {
            attach_console();
            let res = if cmd == "install-hooks" {
                std::env::current_exe().map_err(|e| e.to_string()).and_then(|exe| claude_settings::install(&exe))
            } else {
                claude_settings::uninstall()
            };
            match res {
                Ok(m) => println!("{m}"),
                Err(e) => {
                    eprintln!("Erreur : {e}");
                    std::process::exit(1);
                }
            }
            return;
        }
        _ => {}
    }
    let background = args.iter().any(|a| a == "--background");

    // Instance unique : si le port est déjà pris par claude-notifs, on lui demande d'ouvrir l'admin.
    let server = match tiny_http::Server::http(("127.0.0.1", PORT)) {
        Ok(s) => s,
        Err(_) => {
            if hook::send("GET", "/ping", "") {
                if !background {
                    hook::send("POST", "/show-admin", "");
                }
            } else {
                message_box(&format!("Le port {PORT} est déjà utilisé par une autre application."));
            }
            return;
        }
    };

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    server::start(server, proxy.clone());

    let config = Config::load();
    let tray = match Tray::new(config.dnd, proxy.clone()) {
        Ok(t) => t,
        Err(e) => {
            message_box(&format!("Impossible de créer l'icône de notification : {e}"));
            return;
        }
    };
    let data_dir = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("claude-notifs")
        .join("webview");

    let mut app = App {
        config,
        toasts: ToastManager::default(),
        admin: None,
        tray,
        ctx: WebContext::new(Some(data_dir)),
        proxy,
        last_event: None,
        recent_questions: HashMap::new(),
    };
    let mut opened = background;

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(tao::event::StartCause::Init) if !opened => {
                opened = true;
                app.show_admin(target);
            }
            Event::UserEvent(ev) => match ev {
                UserEvent::Hook(body) => app.handle_hook(target, &body),
                UserEvent::ShowAdmin => app.show_admin(target),
                UserEvent::AdminIpc(body) => app.handle_admin(target, &body),
                UserEvent::CloseToast(id) => {
                    let scale = app.scale(target);
                    app.toasts.close(id, app.config.position, scale);
                }
                UserEvent::ResizeToast(id, h) => {
                    let scale = app.scale(target);
                    app.toasts.resize(id, h, app.config.position, scale);
                }
                UserEvent::Menu(id) => app.handle_menu(target, &id, control_flow),
            },
            Event::WindowEvent { window_id, event: WindowEvent::CloseRequested, .. } => {
                if app.admin.as_ref().is_some_and(|a| a.window.id() == window_id) {
                    app.admin = None;
                }
            }
            _ => {}
        }
    });
}
