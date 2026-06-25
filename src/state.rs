//! Shared app state + the single place that applies a control change and
//! persists it. Both the menu-bar dropdown and the dashboard window call these
//! methods, so behaviour stays identical (DRY).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;

use crate::audio::bank::SoundBank;
use crate::settings::Settings;
use crate::soundpack::loader::{self, PackEntry};

#[derive(Clone)]
pub struct Shared {
    pub enabled: Arc<AtomicBool>,
    pub pitch: Arc<AtomicU32>,  // semitone variation, f32 bits
    pub volume: Arc<AtomicU32>, // master volume, f32 bits
    pub ignore_rapid: Arc<AtomicBool>,
    pub disable_modifiers: Arc<AtomicBool>,
    pub launch_at_login: Arc<AtomicBool>,
    pub bank: Arc<ArcSwap<SoundBank>>,
    pub sample_rate: u32,
    pub packs: Arc<Vec<PackEntry>>,
    pub current_pack: Arc<Mutex<Option<String>>>,
}

impl Shared {
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
    pub fn pitch(&self) -> f32 {
        f32::from_bits(self.pitch.load(Ordering::Relaxed))
    }
    pub fn ignore_rapid(&self) -> bool {
        self.ignore_rapid.load(Ordering::Relaxed)
    }
    pub fn disable_modifiers(&self) -> bool {
        self.disable_modifiers.load(Ordering::Relaxed)
    }
    pub fn launch_at_login(&self) -> bool {
        self.launch_at_login.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, v: bool) {
        self.enabled.store(v, Ordering::Relaxed);
        self.persist();
    }
    pub fn set_volume(&self, v: f32) {
        self.volume.store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        self.persist();
    }
    pub fn set_pitch(&self, semis: f32) {
        self.pitch.store(semis.to_bits(), Ordering::Relaxed);
        self.persist();
    }
    pub fn set_ignore_rapid(&self, v: bool) {
        self.ignore_rapid.store(v, Ordering::Relaxed);
        self.persist();
    }
    pub fn set_disable_modifiers(&self, v: bool) {
        self.disable_modifiers.store(v, Ordering::Relaxed);
        self.persist();
    }
    pub fn set_launch_at_login(&self, v: bool) {
        self.launch_at_login.store(v, Ordering::Relaxed);
        crate::launch::set(v);
        self.persist();
    }

    /// Load + activate a pack by its index into `self.packs`.
    pub fn set_pack(&self, idx: usize) {
        let Some(p) = self.packs.get(idx) else { return };
        match loader::load_pack(&p.dir, self.sample_rate) {
            Ok(bank) => {
                eprintln!("clack: switched to '{}'", bank.name);
                self.bank.store(Arc::new(bank));
                let name = p.dir.file_name().map(|n| n.to_string_lossy().into_owned());
                if let Ok(mut cur) = self.current_pack.lock() {
                    *cur = name;
                }
                self.persist();
            }
            Err(e) => eprintln!("clack: pack load failed: {e}"),
        }
    }

    /// Index of the currently active pack within `self.packs`, if known.
    pub fn current_pack_index(&self) -> Option<usize> {
        let cur = self.current_pack.lock().ok()?.clone()?;
        self.packs.iter().position(|p| {
            p.dir.file_name().and_then(|n| n.to_str()) == Some(cur.as_str())
        })
    }

    fn persist(&self) {
        let pack = self.current_pack.lock().ok().and_then(|c| c.clone());
        Settings {
            enabled: self.enabled(),
            volume: self.volume(),
            pitch: self.pitch(),
            ignore_rapid: self.ignore_rapid(),
            disable_modifiers: self.disable_modifiers(),
            launch_at_login: self.launch_at_login(),
            pack,
        }
        .save();
    }
}
