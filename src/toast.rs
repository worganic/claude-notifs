//! Popups de notification : fenêtres sans bordure, au premier plan, qui ne prennent pas le focus.

use crate::UserEvent;
use crate::config::{Config, Position};
use serde_json::json;
use tao::dpi::{PhysicalPosition, PhysicalSize};
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::platform::windows::WindowBuilderExtWindows;
use tao::window::{Window, WindowBuilder};
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETWORKAREA, SystemParametersInfoW};
use wry::{BackgroundThrottlingPolicy, WebContext, WebView, WebViewBuilder};

const HTML: &str = include_str!("../ui/toast.html");
const WIDTH: f64 = 400.0;
/// Hauteur initiale ; la page envoie ensuite sa hauteur réelle (« h:<px> »).
const HEIGHT: f64 = 100.0;
const MARGIN: f64 = 12.0;
const GAP: f64 = 6.0;

pub struct ToastData {
    pub kind: String,
    pub label: String,
    pub project: String,
    pub message: String,
    pub color: String,
    pub duration: u32,
    pub blink: bool,
    pub session_id: String,
}

struct Toast {
    id: u32,
    kind: String,
    session_id: String,
    /// Hauteur de la fenêtre en pixels physiques.
    height: u32,
    window: Window,
    _webview: WebView,
}

#[derive(Default)]
pub struct ToastManager {
    toasts: Vec<Toast>,
    next_id: u32,
}

/// Zone de travail de l'écran principal (barre des tâches exclue), en pixels physiques.
fn work_area() -> RECT {
    let mut rect = RECT { left: 0, top: 0, right: 1920, bottom: 1080 };
    unsafe {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut RECT as *mut _, 0);
    }
    rect
}

/// Position d'un toast de hauteur `h` décalé de `offset` pixels depuis le coin.
fn slot(position: Position, offset: f64, h: f64, scale: f64) -> PhysicalPosition<i32> {
    let wa = work_area();
    let (w, m) = (WIDTH * scale, MARGIN * scale);
    let left = matches!(position, Position::TopLeft | Position::BottomLeft);
    let top = matches!(position, Position::TopLeft | Position::TopRight);
    let x = if left { wa.left as f64 + m } else { wa.right as f64 - m - w };
    let y = if top { wa.top as f64 + m + offset } else { wa.bottom as f64 - m - h - offset };
    PhysicalPosition::new(x.round() as i32, y.round() as i32)
}

impl ToastManager {
    pub fn show(
        &mut self,
        target: &EventLoopWindowTarget<UserEvent>,
        ctx: &mut WebContext,
        proxy: &EventLoopProxy<UserEvent>,
        cfg: &Config,
        data: ToastData,
    ) {
        while self.toasts.len() >= cfg.max_visible {
            self.toasts.remove(0);
        }

        let scale = target.primary_monitor().map(|m| m.scale_factor()).unwrap_or(1.0);
        let id = self.next_id;
        self.next_id += 1;

        let left = matches!(cfg.position, Position::TopLeft | Position::BottomLeft);
        let payload = json!({
            "kind": data.kind,
            "label": data.label,
            "project": data.project,
            "message": data.message,
            "color": data.color,
            "duration": data.duration,
            "blink": data.blink,
            "left": left,
        });
        // `</` est échappé pour ne pas fermer la balise <script> prématurément.
        let script = format!("window.T = {};", payload.to_string().replace("</", "<\\/"));
        let html = HTML.replacen("/*DATA*/", &script, 1);

        let Ok(window) = WindowBuilder::new()
            .with_title("Claude Notifs")
            .with_decorations(false)
            .with_resizable(false)
            .with_always_on_top(true)
            .with_focused(false)
            .with_focusable(false)
            .with_skip_taskbar(true)
            .with_transparent(true)
            .with_undecorated_shadow(false)
            .with_inner_size(PhysicalSize::new((WIDTH * scale) as u32, (HEIGHT * scale) as u32))
            .with_position(slot(cfg.position, 0.0, HEIGHT * scale, scale))
            .build(target)
        else {
            return;
        };

        let proxy = proxy.clone();
        let Ok(webview) = WebViewBuilder::new_with_web_context(ctx)
            .with_html(html)
            .with_transparent(true)
            .with_focused(false)
            .with_background_throttling(BackgroundThrottlingPolicy::Disabled)
            .with_ipc_handler(move |req| {
                let body = req.body();
                if body == "close" {
                    let _ = proxy.send_event(UserEvent::CloseToast(id));
                } else if let Some(h) = body.strip_prefix("h:").and_then(|h| h.parse().ok()) {
                    let _ = proxy.send_event(UserEvent::ResizeToast(id, h));
                }
            })
            .build(&window)
        else {
            return;
        };

        let height = (HEIGHT * scale) as u32;
        self.toasts.push(Toast { id, kind: data.kind, session_id: data.session_id, height, window, _webview: webview });
        self.relayout(cfg.position, scale);
    }

    pub fn resize(&mut self, id: u32, height: u32, position: Position, scale: f64) {
        let Some(t) = self.toasts.iter_mut().find(|t| t.id == id) else { return };
        t.height = height.clamp(40, 400);
        t.window.set_inner_size(PhysicalSize::new((WIDTH * scale) as u32, t.height));
        self.relayout(position, scale);
    }

    pub fn close(&mut self, id: u32, position: Position, scale: f64) {
        self.toasts.retain(|t| t.id != id);
        self.relayout(position, scale);
    }

    /// Ferme les notifications « en attente » d'une session devenues obsolètes.
    pub fn dismiss_session(&mut self, session_id: &str, kinds: &[&str], position: Position, scale: f64) {
        if session_id.is_empty() {
            return;
        }
        let before = self.toasts.len();
        self.toasts.retain(|t| !(t.session_id == session_id && kinds.contains(&t.kind.as_str())));
        if self.toasts.len() != before {
            self.relayout(position, scale);
        }
    }

    /// Replace les toasts : le plus récent dans le coin, les plus anciens empilés au-dessus/dessous.
    pub fn relayout(&self, position: Position, scale: f64) {
        let mut offset = 0.0;
        for t in self.toasts.iter().rev() {
            t.window.set_outer_position(slot(position, offset, t.height as f64, scale));
            offset += t.height as f64 + GAP * scale;
        }
    }
}
