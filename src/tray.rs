use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use eframe::egui::{Context, ViewportCommand};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};

pub struct SystemTray {
    // Dropping this handle removes the icon and its menu.
    _icon: TrayIcon,
    exit_requested: Arc<AtomicBool>,
    hwnd: isize,
}

impl SystemTray {
    pub fn new(ctx: &Context) -> Result<Self, Box<dyn std::error::Error>> {
        let menu = Menu::new();
        let open = MenuItem::new("Open CraftLauncher", true, None);
        let exit = MenuItem::new("Exit", true, None);
        menu.append_items(&[&open, &PredefinedMenuItem::separator(), &exit])?;
        let image = super::window_icon().ok_or("Launcher icon is unavailable")?;
        let icon = TrayIconBuilder::new()
            .with_tooltip("CraftLauncher · Click to open")
            .with_icon(Icon::from_rgba(image.rgba, image.width, image.height)?)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?;
        let exit_requested = Arc::new(AtomicBool::new(false));
        let title: Vec<u16> = "CraftLauncher\0".encode_utf16().collect();
        let hwnd = unsafe { super::FindWindowW(std::ptr::null(), title.as_ptr()) };
        let open_id = open.id().clone();
        let exit_id = exit.id().clone();
        let menu_ctx = ctx.clone();
        let menu_exit = exit_requested.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == open_id {
                restore(&menu_ctx, hwnd);
            } else if event.id == exit_id {
                menu_exit.store(true, Ordering::Relaxed);
                // Wake the window before requesting shutdown, even when hidden.
                restore(&menu_ctx, hwnd);
                menu_ctx.send_viewport_cmd(ViewportCommand::Close);
                menu_ctx.request_repaint();
            }
        }));
        let tray_ctx = ctx.clone();
        let tray_id = icon.id().clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if event.id() == &tray_id && matches!(event,
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }) {
                restore(&tray_ctx, hwnd);
            }
        }));
        Ok(Self { _icon: icon, exit_requested, hwnd })
    }

    pub fn hide(&self) {
        // Keep hide/restore native: egui cannot process visibility commands
        // reliably while hidden, and stale commands affect later window actions.
        unsafe { ShowWindow(self.hwnd, 0); }
    }

    pub fn exiting(&self) -> bool {
        self.exit_requested.load(Ordering::Relaxed)
    }

    pub fn allow_exit(&self) { self.exit_requested.store(true, Ordering::Relaxed); }
}

fn restore(ctx: &Context, hwnd: isize) {
    // Hidden Windows windows do not render, so wake the native window first.
    unsafe {
        ShowWindow(hwnd, if IsIconic(hwnd) != 0 { 9 } else { 5 });
        SetForegroundWindow(hwnd);
    }
    ctx.request_repaint();
}

#[link(name = "user32")]
unsafe extern "system" {
    fn ShowWindow(hwnd: isize, command: i32) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
}
