//! clack — native macOS mechanical keyboard sounds.
//! Dock + menu-bar app: CGEventTap key capture -> cpal additive mixer, reusing
//! thock's soundpacks. Menu controls enable/volume/pitch/soundpack.

mod audio;
mod dashboard;
mod input;
mod launch;
mod permissions;
mod settings;
mod soundpack;
mod state;

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;
use objc2::runtime::ProtocolObject;
use objc2::AllocAnyThread;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSImage};
use objc2_foundation::{MainThreadMarker, NSString};

/// Locate the app icon: bundled `Resources/clack.icns`, else dev `assets/clack.icns`.
fn app_icon(mtm: MainThreadMarker) -> Option<objc2::rc::Retained<NSImage>> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(contents) = exe.parent().and_then(|p| p.parent()) {
            candidates.push(contents.join("Resources/clack.icns"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("assets/clack.icns"));
    }
    for path in candidates {
        if path.is_file() {
            let s = NSString::from_str(&path.to_string_lossy());
            let _ = mtm;
            if let Some(img) = NSImage::initWithContentsOfFile(NSImage::alloc(), &s) {
                return Some(img);
            }
        }
    }
    None
}

fn main() {
    let mtm = MainThreadMarker::new().expect("main thread");
    let app = NSApplication::sharedApplication(mtm);
    // Regular = visible in the Dock (not a pure menu-bar agent).
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    if let Some(icon) = app_icon(mtm) {
        unsafe { app.setApplicationIconImage(Some(&icon)) };
    }

    // CLACK_NO_PROMPT lets you launch the UI without the Accessibility prompt
    // (e.g. for screenshots); key capture stays off until granted.
    let trusted = if std::env::var_os("CLACK_NO_PROMPT").is_some() {
        permissions::is_trusted()
    } else {
        permissions::ensure_trusted()
    };
    if !trusted {
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

    // Build the status-bar menu + dashboard window, all on one controller, and
    // make it the app delegate (so a Dock click re-opens the window).
    let _ = &cfg;
    let controller = dashboard::Controller::new(mtm, shared);
    let delegate: &ProtocolObject<dyn NSApplicationDelegate> =
        ProtocolObject::from_ref(&*controller);
    app.setDelegate(Some(delegate));

    // Bring the dashboard to the front on launch.
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);

    let _ = &engine; // keep the audio stream alive
    app.run();
    drop(controller);
}
