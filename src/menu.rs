//! Menu-bar menu (built with muda) + a background poller that turns menu clicks
//! into state changes. The poller only touches `Send` shared state (atomics,
//! ArcSwap, the volume handle), never the non-`Send` cpal stream or menu items.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwap;
use muda::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};

use crate::audio::bank::SoundBank;
use crate::soundpack::loader::{self, PackEntry};

#[derive(Clone)]
pub enum Action {
    Toggle,
    Volume(f32),
    Pitch(f32),
    Pack(usize),
    ToggleRapid,
    ToggleModifiers,
    ToggleLogin,
}

/// Shared state the poller mutates. All fields are `Send`/thread-safe.
pub struct MenuShared {
    pub enabled: Arc<AtomicBool>,
    pub pitch: Arc<AtomicU32>,  // semitone variation, f32 bits
    pub volume: Arc<AtomicU32>, // master volume, f32 bits
    pub ignore_rapid: Arc<AtomicBool>,
    pub disable_modifiers: Arc<AtomicBool>,
    pub launch_at_login: Arc<AtomicBool>,
    pub bank: Arc<ArcSwap<SoundBank>>,
    pub sample_rate: u32,
}

impl MenuShared {
    /// Snapshot current state into a persistable `Settings`.
    fn snapshot(&self, pack: Option<String>) -> crate::settings::Settings {
        crate::settings::Settings {
            enabled: self.enabled.load(Ordering::Relaxed),
            volume: f32::from_bits(self.volume.load(Ordering::Relaxed)),
            pitch: f32::from_bits(self.pitch.load(Ordering::Relaxed)),
            ignore_rapid: self.ignore_rapid.load(Ordering::Relaxed),
            disable_modifiers: self.disable_modifiers.load(Ordering::Relaxed),
            launch_at_login: self.launch_at_login.load(Ordering::Relaxed),
            pack,
        }
    }
}

/// Build the menu. Returns the `Menu` (give it to the tray) and the id->action
/// map (move it into the poller). Packs are listed for the soundpack submenu.
pub fn build(
    packs: &[PackEntry],
    s: &crate::settings::Settings,
) -> (Menu, HashMap<MenuId, Action>) {
    let menu = Menu::new();
    let mut map: HashMap<MenuId, Action> = HashMap::new();

    let enable = CheckMenuItem::new("Enable clack", true, s.enabled, None);
    map.insert(enable.id().clone(), Action::Toggle);
    let _ = menu.append(&enable);
    let _ = menu.append(&PredefinedMenuItem::separator());

    // Volume submenu (discrete steps).
    let vol = Submenu::new("Volume", true);
    for pct in [0u32, 25, 50, 75, 100] {
        let item = MenuItem::new(format!("{pct}%"), true, None);
        map.insert(item.id().clone(), Action::Volume(pct as f32 / 100.0));
        let _ = vol.append(&item);
    }
    let _ = menu.append(&vol);

    // Pitch variation submenu.
    let pitch = Submenu::new("Pitch variation", true);
    for (label, semis) in [("Off", 0.0), ("Subtle", 1.0), ("Medium", 2.0), ("Wild", 4.0)] {
        let item = MenuItem::new(label, true, None);
        map.insert(item.id().clone(), Action::Pitch(semis));
        let _ = pitch.append(&item);
    }
    let _ = menu.append(&pitch);

    // Soundpack picker.
    let kb_packs: Vec<(usize, &PackEntry)> = packs
        .iter()
        .enumerate()
        .filter(|(_, p)| p.category == "keyboard")
        .collect();
    let picker = Submenu::new("Soundpack", true);
    for (idx, p) in &kb_packs {
        let item = MenuItem::new(&p.name, true, None);
        map.insert(item.id().clone(), Action::Pack(*idx));
        let _ = picker.append(&item);
    }
    let _ = menu.append(&picker);
    let _ = menu.append(&PredefinedMenuItem::separator());

    // Quick settings.
    let quick = Submenu::new("Quick settings", true);
    let rapid = CheckMenuItem::new("Ignore rapid keys (<10ms)", true, s.ignore_rapid, None);
    map.insert(rapid.id().clone(), Action::ToggleRapid);
    let _ = quick.append(&rapid);
    let mods = CheckMenuItem::new("Disable modifier sounds", true, s.disable_modifiers, None);
    map.insert(mods.id().clone(), Action::ToggleModifiers);
    let _ = quick.append(&mods);
    let login = CheckMenuItem::new("Launch at login", true, s.launch_at_login, None);
    map.insert(login.id().clone(), Action::ToggleLogin);
    let _ = quick.append(&login);
    let _ = menu.append(&quick);
    let _ = menu.append(&PredefinedMenuItem::separator());

    let _ = menu.append(&PredefinedMenuItem::quit(Some("Quit clack")));

    (menu, map)
}

/// Spawn the background poller. Consumes the id->action map and shared state.
pub fn spawn_poller(
    map: HashMap<MenuId, Action>,
    shared: MenuShared,
    packs: Vec<PackEntry>,
    mut current_pack: Option<String>,
) {
    std::thread::spawn(move || {
        let rx = MenuEvent::receiver();
        while let Ok(ev) = rx.recv() {
            let Some(action) = map.get(&ev.id) else {
                continue;
            };
            match action {
                Action::Toggle => {
                    let now = !shared.enabled.load(Ordering::Relaxed);
                    shared.enabled.store(now, Ordering::Relaxed);
                }
                Action::Volume(v) => {
                    shared.volume.store(v.to_bits(), Ordering::Relaxed);
                }
                Action::Pitch(p) => {
                    shared.pitch.store(p.to_bits(), Ordering::Relaxed);
                }
                Action::ToggleRapid => {
                    let now = !shared.ignore_rapid.load(Ordering::Relaxed);
                    shared.ignore_rapid.store(now, Ordering::Relaxed);
                }
                Action::ToggleModifiers => {
                    let now = !shared.disable_modifiers.load(Ordering::Relaxed);
                    shared.disable_modifiers.store(now, Ordering::Relaxed);
                }
                Action::ToggleLogin => {
                    let now = !shared.launch_at_login.load(Ordering::Relaxed);
                    shared.launch_at_login.store(now, Ordering::Relaxed);
                    crate::launch::set(now);
                }
                Action::Pack(idx) => {
                    if let Some(p) = packs.get(*idx) {
                        match loader::load_pack(&p.dir, shared.sample_rate) {
                            Ok(bank) => {
                                eprintln!("clack: switched to '{}'", bank.name);
                                shared.bank.store(Arc::new(bank));
                                current_pack = p
                                    .dir
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned());
                            }
                            Err(e) => eprintln!("clack: pack load failed: {e}"),
                        }
                    }
                }
            }
            shared.snapshot(current_pack.clone()).save();
        }
    });
}
