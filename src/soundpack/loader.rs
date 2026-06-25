//! Scan the thock soundpack directory and decode a pack into a `SoundBank`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::config::PackConfig;
use crate::audio::bank::{PcmId, Sample, SoundBank};

/// `~/Library/Application Support/Thock/Soundpacks`
pub fn soundpacks_dir() -> Option<PathBuf> {
    dirs::home_dir()
        .map(|h| h.join("Library/Application Support/Thock/Soundpacks"))
        .filter(|p| p.is_dir())
}

/// A discovered pack: its directory plus parsed metadata name.
pub struct PackEntry {
    pub dir: PathBuf,
    pub name: String,
    pub category: String,
}

/// List all valid packs (dir has a parseable config.json).
pub fn list_packs() -> Vec<PackEntry> {
    let Some(root) = soundpacks_dir() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return out;
    };
    for e in entries.flatten() {
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        if let Some(cfg) = read_config(&dir) {
            out.push(PackEntry {
                name: cfg.metadata.name,
                category: cfg.metadata.category,
                dir,
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

fn read_config(dir: &Path) -> Option<PackConfig> {
    let text = std::fs::read_to_string(dir.join("config.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// Load a pack directory into a `SoundBank`, decoding+resampling every referenced
/// wav to interleaved stereo f32 at `target_rate`.
pub fn load_pack(dir: &Path, target_rate: u32) -> Result<SoundBank, String> {
    let cfg = read_config(dir).ok_or_else(|| format!("no/invalid config.json in {dir:?}"))?;

    let mut samples: Vec<Sample> = Vec::new();
    let mut by_file: HashMap<String, PcmId> = HashMap::new();
    let mut down: HashMap<String, Vec<PcmId>> = HashMap::new();
    let mut up: HashMap<String, Vec<PcmId>> = HashMap::new();

    let mut intern = |file: &str,
                      samples: &mut Vec<Sample>,
                      by_file: &mut HashMap<String, PcmId>|
     -> Option<PcmId> {
        if let Some(&id) = by_file.get(file) {
            return Some(id);
        }
        let sample = decode_wav(&dir.join(file), target_rate).ok()?;
        let id = samples.len();
        samples.push(sample);
        by_file.insert(file.to_string(), id);
        Some(id)
    };

    for (key, ks) in &cfg.sounds {
        let mut d = Vec::new();
        for f in &ks.down {
            if let Some(id) = intern(f, &mut samples, &mut by_file) {
                d.push(id);
            }
        }
        if !d.is_empty() {
            down.insert(key.clone(), d);
        }
        let mut u = Vec::new();
        for f in &ks.up {
            if let Some(id) = intern(f, &mut samples, &mut by_file) {
                u.push(id);
            }
        }
        if !u.is_empty() {
            up.insert(key.clone(), u);
        }
    }

    if down.is_empty() {
        return Err(format!("pack '{}' decoded no down sounds", cfg.metadata.name));
    }

    Ok(SoundBank {
        name: cfg.metadata.name,
        samples,
        down,
        up,
        supports_key_up: cfg.metadata.supports_key_up,
    })
}

/// Decode a wav to interleaved stereo f32 at `target_rate` (linear resample).
fn decode_wav(path: &Path, target_rate: u32) -> Result<Sample, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("{path:?}: {e}"))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;

    // Read to a flat f32 buffer normalised to [-1, 1].
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .filter_map(|s| s.ok())
            .collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|v| v as f32 / max)
                .collect()
        }
    };

    // Deinterleave into stereo (mono -> duplicated), source-rate frames.
    let src_frames = raw.len() / channels;
    let mut stereo: Vec<f32> = Vec::with_capacity(src_frames * 2);
    for f in 0..src_frames {
        let base = f * channels;
        let l = raw[base];
        let r = if channels >= 2 { raw[base + 1] } else { l };
        stereo.push(l);
        stereo.push(r);
    }

    let stereo = if spec.sample_rate == target_rate {
        stereo
    } else {
        resample_stereo(&stereo, spec.sample_rate, target_rate)
    };

    let frames = stereo.len() / 2;
    Ok(Sample {
        data: Arc::from(stereo.into_boxed_slice()),
        frames,
    })
}

/// Linear-interpolation resample of interleaved stereo f32.
fn resample_stereo(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    let in_frames = input.len() / 2;
    if in_frames == 0 {
        return Vec::new();
    }
    let ratio = from as f64 / to as f64;
    let out_frames = ((in_frames as f64) / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_frames * 2);
    for i in 0..out_frames {
        let pos = i as f64 * ratio;
        let idx = pos.floor() as usize;
        let frac = (pos - idx as f64) as f32;
        let next = (idx + 1).min(in_frames - 1);
        for ch in 0..2 {
            let a = input[idx * 2 + ch];
            let b = input[next * 2 + ch];
            out.push(a + (b - a) * frac);
        }
    }
    out
}
