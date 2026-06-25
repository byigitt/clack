//! A single playing voice: an immutable stereo PCM buffer read at a fractional
//! position (linear interpolation gives pitch shifting for free).

use std::sync::Arc;

pub struct Voice {
    data: Arc<[f32]>,
    frames: usize,
    pos: f64,
    ratio: f64,
    gain: f32,
}

impl Voice {
    pub fn new(data: Arc<[f32]>, frames: usize, gain: f32, ratio: f32) -> Self {
        Voice {
            data,
            frames,
            pos: 0.0,
            ratio: ratio as f64,
            gain,
        }
    }

    #[inline]
    pub fn done(&self) -> bool {
        self.pos as usize >= self.frames.saturating_sub(1)
    }

    /// Add this voice's contribution into one interleaved stereo frame.
    #[inline]
    pub fn mix_frame(&mut self, out_l: &mut f32, out_r: &mut f32, master: f32) {
        let idx = self.pos as usize;
        if idx + 1 >= self.frames {
            self.pos = self.frames as f64; // mark finished
            return;
        }
        let frac = (self.pos - idx as f64) as f32;
        let base = idx * 2;
        let next = base + 2;
        let l = self.data[base] + (self.data[next] - self.data[base]) * frac;
        let r = self.data[base + 1] + (self.data[next + 1] - self.data[base + 1]) * frac;
        let g = self.gain * master;
        *out_l += l * g;
        *out_r += r * g;
        self.pos += self.ratio;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_and_finishes() {
        // 3 stereo frames: L=1.0, R=0.5 each.
        let data: Arc<[f32]> = Arc::from(vec![1.0, 0.5, 1.0, 0.5, 1.0, 0.5].into_boxed_slice());
        let mut v = Voice::new(data, 3, 0.5, 1.0);
        let (mut l, mut r) = (0.0f32, 0.0f32);
        v.mix_frame(&mut l, &mut r, 1.0); // gain 0.5 * master 1.0
        assert!((l - 0.5).abs() < 1e-6, "l={l}");
        assert!((r - 0.25).abs() < 1e-6, "r={r}");
        v.mix_frame(&mut l, &mut r, 1.0);
        assert!(v.done(), "voice should finish at last frame");
    }
}
