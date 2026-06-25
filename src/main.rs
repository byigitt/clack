//! clack — native macOS mechanical keyboard sounds.
//! Dock + menu-bar app: CGEventTap key capture -> cpal additive mixer, reusing
//! thock's soundpacks. Menu controls enable/volume/pitch/soundpack.

mod audio;
mod dashboard;
mod input;
mod launch;
mod menu;
mod permissions;
mod settings;
mod soundpack;
mod state;

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::NSApplicationDelegate;

use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::MainThreadMarker;
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

    let cfg = settings::Settings::load();
    engine
        .volume_handle()
        .store(cfg.volume.to_bits(), std::sync::atomic::Ordering::Relaxed);

    let packs = soundpack::loader::list_packs();
    eprintln!("clack: found {} soundpack(s)", packs.len());
    // Prefer the saved pack, else the first keyboard pack.
    let chosen = packs
        .iter()
        .find(|p| {
            cfg.pack.as_deref()
                == p.dir.file_name().and_then(|n| n.to_str())
        })
        .or_else(|| packs.iter().find(|p| p.category == "keyboard"));
    let current_pack = chosen
        .and_then(|p| p.dir.file_name().map(|n| n.to_string_lossy().into_owned()));
    let bank = chosen
        .and_then(|p| soundpack::loader::load_pack(&p.dir, engine.sample_rate).ok())
        .unwrap_or_else(audio::bank::SoundBank::empty);
    eprintln!("clack: loaded '{}' — {} samples", bank.name, bank.samples.len());

    // Shared state between the menu and the event tap, seeded from settings.
    let bank = Arc::new(ArcSwap::from_pointee(bank));
    let enabled = Arc::new(AtomicBool::new(cfg.enabled));
    let pitch = Arc::new(AtomicU32::new(cfg.pitch.to_bits()));
    let disable_modifiers = Arc::new(AtomicBool::new(cfg.disable_modifiers));
    let ignore_rapid = Arc::new(AtomicBool::new(cfg.ignore_rapid));
    let launch_at_login = Arc::new(AtomicBool::new(cfg.launch_at_login));

    // One shared-state bundle drives the tap, the menu, and the dashboard.
    let shared = state::Shared {
        enabled: enabled.clone(),
        pitch: pitch.clone(),
        volume: engine.volume_handle(),
        ignore_rapid: ignore_rapid.clone(),
        disable_modifiers: disable_modifiers.clone(),
        launch_at_login: launch_at_login.clone(),
        bank: bank.clone(),
        sample_rate: engine.sample_rate,
        packs: Arc::new(packs.clone()),
        current_pack: Arc::new(Mutex::new(current_pack)),
    };

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

    // Build the menu + tray, and spawn the click poller.
    let (m, action_map) = menu::build(&packs, &cfg);
    let _tray: TrayIcon = TrayIconBuilder::new()
        .with_menu(Box::new(m))
        .with_tooltip("clack")
        .with_icon(tray_image())
        .build()
        .expect("build tray");
    menu::spawn_poller(action_map, shared.clone());

    // Build the dashboard window + make it the app delegate (so a Dock click
    // re-opens it). Keep the controller alive for the whole process.
    let controller = dashboard::Controller::new(mtm, shared);
    let delegate: &ProtocolObject<dyn NSApplicationDelegate> =
        ProtocolObject::from_ref(&*controller);
    app.setDelegate(Some(delegate));

    let _ = &engine; // keep the audio stream alive
    app.run();
    drop(controller);
}
