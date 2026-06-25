//! clack — native macOS mechanical keyboard sounds. Phase 0: Dock + menu bar shell.

mod audio;
mod input;
mod permissions;
mod soundpack;

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;

use arc_swap::ArcSwap;

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

    if !permissions::ensure_trusted() {
        eprintln!("clack: waiting for Accessibility permission (System Settings → Privacy & Security → Accessibility).");
    }

    // Start the audio engine, then load a pack at the device sample rate.
    let (engine, tx) = audio::engine::AudioEngine::start().expect("audio engine");
    eprintln!("clack: audio @ {} Hz", engine.sample_rate);

    let packs = soundpack::loader::list_packs();
    eprintln!("clack: found {} soundpack(s)", packs.len());
    let bank = packs
        .iter()
        .find(|p| p.category == "keyboard")
        .and_then(|p| soundpack::loader::load_pack(&p.dir, engine.sample_rate).ok())
        .unwrap_or_else(audio::bank::SoundBank::empty);
    eprintln!("clack: loaded '{}' — {} samples", bank.name, bank.samples.len());

    // Shared state between the menu (later) and the event tap.
    let bank = Arc::new(ArcSwap::from_pointee(bank));
    let enabled = Arc::new(AtomicBool::new(true));
    let pitch = Arc::new(AtomicU32::new(0.0f32.to_bits()));
    let disable_modifiers = Arc::new(AtomicBool::new(false));
    let ignore_rapid = Arc::new(AtomicBool::new(false));

    // Install the global keyboard tap. The tx (ring producer) moves into it.
    let state = input::tap::TapState::new(
        tx,
        bank.clone(),
        enabled.clone(),
        pitch.clone(),
        disable_modifiers.clone(),
        ignore_rapid.clone(),
    );
    if !input::tap::install(state) {
        eprintln!("clack: key capture unavailable — grant Accessibility and relaunch.");
    }
    let _ = &engine; // keep the audio stream alive

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
