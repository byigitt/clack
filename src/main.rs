//! clack — native macOS mechanical keyboard sounds. Phase 0: Dock + menu bar shell.

use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::MainThreadMarker;
use tray_icon::menu::{Menu, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// Tiny generated icon so we don't need a bundled asset yet.
// ponytail: solid-glyph 22x22 icon; swap for a real .icns at packaging.
fn tray_image() -> tray_icon::Icon {
    const N: u32 = 22;
    let mut rgba = vec![0u8; (N * N * 4) as usize];
    for y in 0..N {
        for x in 0..N {
            let i = ((y * N + x) * 4) as usize;
            // a filled rounded-ish keycap silhouette
            let edge = x < 3 || x >= N - 3 || y < 5 || y >= N - 5;
            let on = !edge;
            let a = if on { 255 } else { 0 };
            rgba[i] = 235;
            rgba[i + 1] = 235;
            rgba[i + 2] = 235;
            rgba[i + 3] = a;
        }
    }
    tray_icon::Icon::from_rgba(rgba, N, N).expect("valid icon")
}

fn main() {
    let mtm = MainThreadMarker::new().expect("main thread");
    let app = NSApplication::sharedApplication(mtm);
    // Regular = visible in the Dock (not a pure menu-bar agent).
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let menu = Menu::new();
    menu.append(&PredefinedMenuItem::quit(Some("Quit clack")))
        .expect("append quit");

    // Keep the tray alive for the whole process.
    let _tray: TrayIcon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("clack")
        .with_icon(tray_image())
        .build()
        .expect("build tray");

    app.run();
}
