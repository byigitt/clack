//! Menu-bar dropdown (muda) + a background poller that maps clicks to
//! `Shared` actions. The poller only touches `Send` shared state.

use std::collections::HashMap;

use muda::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};

use crate::settings::Settings;
use crate::soundpack::loader::PackEntry;
use crate::state::Shared;

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

/// Build the dropdown menu. Returns the `Menu` (for the tray) and the
/// id->action map (for the poller).
pub fn build(packs: &[PackEntry], s: &Settings) -> (Menu, HashMap<MenuId, Action>) {
    let menu = Menu::new();
    let mut map: HashMap<MenuId, Action> = HashMap::new();

    let enable = CheckMenuItem::new("Enable clack", true, s.enabled, None);
    map.insert(enable.id().clone(), Action::Toggle);
    let _ = menu.append(&enable);
    let _ = menu.append(&PredefinedMenuItem::separator());

    let vol = Submenu::new("Volume", true);
    for pct in [0u32, 25, 50, 75, 100] {
        let item = MenuItem::new(format!("{pct}%"), true, None);
        map.insert(item.id().clone(), Action::Volume(pct as f32 / 100.0));
        let _ = vol.append(&item);
    }
    let _ = menu.append(&vol);

    let pitch = Submenu::new("Pitch variation", true);
    for (label, semis) in [("Off", 0.0), ("Subtle", 1.0), ("Medium", 2.0), ("Wild", 4.0)] {
        let item = MenuItem::new(label, true, None);
        map.insert(item.id().clone(), Action::Pitch(semis));
        let _ = pitch.append(&item);
    }
    let _ = menu.append(&pitch);

    let picker = Submenu::new("Soundpack", true);
    for (idx, p) in packs.iter().enumerate().filter(|(_, p)| p.category == "keyboard") {
        let item = MenuItem::new(&p.name, true, None);
        map.insert(item.id().clone(), Action::Pack(idx));
        let _ = picker.append(&item);
    }
    let _ = menu.append(&picker);
    let _ = menu.append(&PredefinedMenuItem::separator());

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

/// Spawn the background poller that applies menu clicks to shared state.
pub fn spawn_poller(map: HashMap<MenuId, Action>, shared: Shared) {
    std::thread::spawn(move || {
        let rx = MenuEvent::receiver();
        while let Ok(ev) = rx.recv() {
            match map.get(&ev.id) {
                Some(Action::Toggle) => shared.set_enabled(!shared.enabled()),
                Some(Action::Volume(v)) => shared.set_volume(*v),
                Some(Action::Pitch(p)) => shared.set_pitch(*p),
                Some(Action::Pack(idx)) => shared.set_pack(*idx),
                Some(Action::ToggleRapid) => shared.set_ignore_rapid(!shared.ignore_rapid()),
                Some(Action::ToggleModifiers) => {
                    shared.set_disable_modifiers(!shared.disable_modifiers())
                }
                Some(Action::ToggleLogin) => shared.set_launch_at_login(!shared.launch_at_login()),
                None => {}
            }
        }
    });
}
