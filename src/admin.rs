//! Fenêtre d'administration (WebView) ; les messages IPC sont traités par `App::handle_admin`.

use crate::UserEvent;
use crate::tray;
use tao::dpi::LogicalSize;
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::window::{Icon, Window, WindowBuilder};
use wry::{WebContext, WebView, WebViewBuilder};

const HTML: &str = include_str!("../ui/admin.html");

pub struct Admin {
    pub window: Window,
    pub webview: WebView,
}

impl Admin {
    pub fn open(
        target: &EventLoopWindowTarget<UserEvent>,
        ctx: &mut WebContext,
        proxy: &EventLoopProxy<UserEvent>,
    ) -> Option<Admin> {
        let icon = Icon::from_rgba(tray::icon_rgba(false), tray::ICON_SIZE, tray::ICON_SIZE).ok();
        let window = WindowBuilder::new()
            .with_title("Claude Notifs — Administration")
            .with_inner_size(LogicalSize::new(880.0, 820.0))
            .with_min_inner_size(LogicalSize::new(420.0, 400.0))
            .with_window_icon(icon)
            .build(target)
            .ok()?;
        let proxy = proxy.clone();
        let webview = WebViewBuilder::new_with_web_context(ctx)
            .with_html(HTML)
            .with_ipc_handler(move |req| {
                let _ = proxy.send_event(UserEvent::AdminIpc(req.body().clone()));
            })
            .build(&window)
            .ok()?;
        Some(Admin { window, webview })
    }

    pub fn focus(&self) {
        self.window.set_visible(true);
        self.window.set_minimized(false);
        self.window.set_focus();
    }

    pub fn call(&self, js: &str) {
        let _ = self.webview.evaluate_script(js);
    }
}
