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
}

/// Shared state the poller mutates. All fields are `Send`/thread-safe.
pub struct MenuShared {
    pub enabled: Arc<AtomicBool>,
    pub pitch: Arc<AtomicU32>,  // semitone variation, f32 bits
    pub volume: Arc<AtomicU32>, // master volume, f32 bits
    pub bank: Arc<ArcSwap<SoundBank>>,
    pub sample_rate: u32,
}

/// Build the menu. Returns the `Menu` (give it to the tray) and the id->action
/// map (move it into the poller). Packs are listed for the soundpack submenu.
pub fn build(packs: &[PackEntry], enabled: bool) -> (Menu, HashMap<MenuId, Action>) {
    let menu = Menu::new();
    let mut map: HashMap<MenuId, Action> = HashMap::new();

    let enable = CheckMenuItem::new("Enable clack", true, enabled, None);
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

    let _ = menu.append(&PredefinedMenuItem::quit(Some("Quit clack")));

    (menu, map)
}

/// Spawn the background poller. Consumes the id->action map and shared state.
pub fn spawn_poller(map: HashMap<MenuId, Action>, shared: MenuShared, packs: Vec<PackEntry>) {
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
                Action::Pack(idx) => {
                    if let Some(p) = packs.get(*idx) {
                        match loader::load_pack(&p.dir, shared.sample_rate) {
                            Ok(bank) => {
                                eprintln!("clack: switched to '{}'", bank.name);
                                shared.bank.store(Arc::new(bank));
                            }
                            Err(e) => eprintln!("clack: pack load failed: {e}"),
                        }
                    }
                }
            }
        }
    });
}
