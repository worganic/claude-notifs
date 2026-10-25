//! Icône de la zone de notification et son menu.

use crate::UserEvent;
use tao::event_loop::EventLoopProxy;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub const ICON_SIZE: u32 = 32;

/// Cloche blanche sur disque orange (gris en mode « ne pas déranger »), dessinée en RGBA.
pub fn icon_rgba(dnd: bool) -> Vec<u8> {
    let bg: [f64; 3] = if dnd { [120.0, 120.0, 128.0] } else { [217.0, 119.0, 87.0] };
    let n = ICON_SIZE as f64;
    let c = n / 2.0;
    let mut px = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
            let disc = (c - 0.5 - ((fx - c).powi(2) + (fy - c).powi(2)).sqrt()).clamp(0.0, 1.0);
            // Cloche : dôme (demi-disque) + corps + rebord + battant.
            let dome = (6.5 - ((fx - c).powi(2) + (fy - 13.5).powi(2)).sqrt()).clamp(0.0, 1.0)
                * if fy <= 14.0 { 1.0 } else { 0.0 };
            let body = if (fx - c).abs() <= 6.5 && (13.5..=20.0).contains(&fy) { 1.0 } else { 0.0 };
            let rim = if (fx - c).abs() <= 9.0 && (19.5..=22.0).contains(&fy) { 1.0 } else { 0.0 };
            let clapper = (2.2 - ((fx - c).powi(2) + (fy - 23.5).powi(2)).sqrt()).clamp(0.0, 1.0);
            let bell = f64::max(f64::max(dome, body), f64::max(rim, clapper)) * disc;
            for ch in bg {
                px.push((ch * (1.0 - bell) + 255.0 * bell).round() as u8);
            }
            px.push((disc * 255.0).round() as u8);
        }
    }
    px
}

pub fn icon(dnd: bool) -> Icon {
    Icon::from_rgba(icon_rgba(dnd), ICON_SIZE, ICON_SIZE).expect("icône valide")
}

pub struct Tray {
    pub tray: TrayIcon,
    pub admin: MenuItem,
    pub test: MenuItem,
    pub dnd: CheckMenuItem,
    pub quit: MenuItem,
}

impl Tray {
    pub fn new(dnd: bool, proxy: EventLoopProxy<UserEvent>) -> tray_icon::Result<Tray> {
        let admin = MenuItem::new("Administration", true, None);
        let test = MenuItem::new("Tester une notification", true, None);
        let dnd_item = CheckMenuItem::new("Ne pas déranger", true, dnd, None);
        let quit = MenuItem::new("Quitter", true, None);
        let menu = Menu::new();
        let _ = menu.append_items(&[&admin, &test, &dnd_item, &PredefinedMenuItem::separator(), &quit]);

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(tooltip(dnd))
            .with_icon(icon(dnd))
            .build()?;

        let p = proxy.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let _ = p.send_event(UserEvent::Menu(e.id));
        }));
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            let open = matches!(
                e,
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                    | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }
            );
            if open {
                let _ = proxy.send_event(UserEvent::ShowAdmin);
            }
        }));

        Ok(Tray { tray, admin, test, dnd: dnd_item, quit })
    }

    pub fn set_dnd(&self, dnd: bool) {
        self.dnd.set_checked(dnd);
        let _ = self.tray.set_icon(Some(icon(dnd)));
        let _ = self.tray.set_tooltip(Some(tooltip(dnd)));
    }
}

fn tooltip(dnd: bool) -> &'static str {
    if dnd { "Claude Notifs — ne pas déranger" } else { "Claude Notifs" }
}
