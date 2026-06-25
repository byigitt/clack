//! Low-latency audio: one persistent cpal output stream with an app-owned
//! additive mixer. The render callback never locks, allocates, or does I/O — it
//! drains a lock-free ring of `Trigger`s and mixes a fixed pool of voices.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::RingBuffer;

use super::voice::Voice;

/// A request to start playing a (already resolved) sound. Built on the key
/// thread and pushed across the ring; the audio thread only mixes.
pub struct Trigger {
    pub data: Arc<[f32]>,
    pub frames: usize,
    pub gain: f32,
    pub ratio: f32,
}

const MAX_VOICES: usize = 64;
const RING_CAP: usize = 1024;

/// Handle to push triggers from the key thread (single producer).
pub type TriggerTx = rtrb::Producer<Trigger>;

pub struct AudioEngine {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    volume: Arc<AtomicU32>, // f32 bits
}

impl AudioEngine {
    /// Build and start the output stream. Returns the engine (keep it alive) and
    /// the producer end of the trigger ring.
    pub fn start() -> Result<(Self, TriggerTx), String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("no default output device")?;
        let supported = device
            .default_output_config()
            .map_err(|e| format!("default config: {e}"))?;

        let sample_rate = supported.sample_rate();
        let channels = supported.channels() as usize;

        // Small fixed buffer for low latency, clamped to what the device supports.
        let mut config: cpal::StreamConfig = supported.config();
        config.buffer_size = match supported.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } => {
                cpal::BufferSize::Fixed(256u32.clamp(*min, *max))
            }
            cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Default,
        };

        let (producer, mut consumer) = RingBuffer::<Trigger>::new(RING_CAP);

        let volume = Arc::new(AtomicU32::new(0.5f32.to_bits()));
        let cb_volume = volume.clone();

        let mut voices: Vec<Voice> = Vec::with_capacity(MAX_VOICES);

        let render = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            // Activate any pending triggers.
            while let Ok(t) = consumer.pop() {
                if voices.len() >= MAX_VOICES {
                    voices.remove(0); // drop oldest
                }
                voices.push(Voice::new(t.data, t.frames, t.gain, t.ratio));
            }

            let master = f32::from_bits(cb_volume.load(Ordering::Relaxed));

            for frame in data.chunks_mut(channels) {
                let mut l = 0.0f32;
                let mut r = 0.0f32;
                for v in voices.iter_mut() {
                    v.mix_frame(&mut l, &mut r, master);
                }
                // Write to however many channels the device has.
                if channels == 1 {
                    frame[0] = (l + r) * 0.5;
                } else {
                    frame[0] = l;
                    frame[1] = r;
                    for s in frame.iter_mut().skip(2) {
                        *s = 0.0;
                    }
                }
            }

            voices.retain(|v| !v.done());
        };

        let err_fn = |e| eprintln!("clack: audio stream error: {e}");

        let stream = device
            .build_output_stream(config, render, err_fn, None)
            .map_err(|e| format!("build stream: {e}"))?;

        stream.play().map_err(|e| format!("play: {e}"))?;

        Ok((
            AudioEngine {
                _stream: stream,
                sample_rate,
                volume,
            },
            producer,
        ))
    }

    /// A clonable handle to the master volume, so the menu thread can adjust it
    /// without holding the (non-Send) cpal stream.
    pub fn volume_handle(&self) -> Arc<AtomicU32> {
        self.volume.clone()
    }
}
