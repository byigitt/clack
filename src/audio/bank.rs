//! Preloaded, immutable sound bank. Everything is decoded to interleaved stereo
//! f32 at the audio device's sample rate up front — nothing is decoded on a
//! keypress. Shared across threads via `arc-swap`.

use std::collections::HashMap;
use std::sync::Arc;

pub type PcmId = usize;

/// One decoded sound: interleaved stereo f32, `frames` stereo frames.
pub struct Sample {
    pub data: Arc<[f32]>,
    pub frames: usize,
}

pub struct SoundBank {
    pub name: String,
    pub samples: Vec<Sample>,
    pub down: HashMap<String, Vec<PcmId>>,
    pub up: HashMap<String, Vec<PcmId>>,
    pub supports_key_up: bool,
}

impl SoundBank {
    /// Pick a random key-down sound for `key`, falling back to "default".
    pub fn pick_down(&self, key: &str) -> Option<PcmId> {
        pick(&self.down, key)
    }

    /// Pick a random key-up sound for `key`, falling back to "default".
    pub fn pick_up(&self, key: &str) -> Option<PcmId> {
        if !self.supports_key_up {
            return None;
        }
        pick(&self.up, key)
    }

    pub fn sample(&self, id: PcmId) -> &Sample {
        &self.samples[id]
    }

    /// An empty bank — used as the initial state before a pack is loaded.
    pub fn empty() -> Self {
        SoundBank {
            name: "none".into(),
            samples: Vec::new(),
            down: HashMap::new(),
            up: HashMap::new(),
            supports_key_up: false,
        }
    }
}

fn pick(map: &HashMap<String, Vec<PcmId>>, key: &str) -> Option<PcmId> {
    let list = map
        .get(key)
        .filter(|l| !l.is_empty())
        .or_else(|| map.get("default").filter(|l| !l.is_empty()))?;
    Some(list[fastrand::usize(..list.len())])
}
