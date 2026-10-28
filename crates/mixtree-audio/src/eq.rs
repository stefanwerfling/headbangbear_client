//! A 3-band DJ-style EQ built from RBJ biquad filters (low-shelf, peak,
//! high-shelf). Each band's gain maps a `0.0..=1.0` control to decibels, with
//! `0.0` acting as a near-kill and `0.5` as unity — matching a DJ mixer.

/// A transposed-direct-form-II biquad filter.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        // Pass-through.
        Biquad {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }
}

impl Biquad {
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    fn set(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// RBJ low-shelf.
    fn low_shelf(fs: f32, f0: f32, gain_db: f32) -> Biquad {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * f0 / fs;
        let (sn, cs) = w0.sin_cos();
        let alpha = sn / 2.0 * (2.0f32).sqrt();
        let sq = 2.0 * a.sqrt() * alpha;
        let mut bq = Biquad::default();
        bq.set(
            a * ((a + 1.0) - (a - 1.0) * cs + sq),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cs),
            a * ((a + 1.0) - (a - 1.0) * cs - sq),
            (a + 1.0) + (a - 1.0) * cs + sq,
            -2.0 * ((a - 1.0) + (a + 1.0) * cs),
            (a + 1.0) + (a - 1.0) * cs - sq,
        );
        bq
    }

    /// RBJ high-shelf.
    fn high_shelf(fs: f32, f0: f32, gain_db: f32) -> Biquad {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * f0 / fs;
        let (sn, cs) = w0.sin_cos();
        let alpha = sn / 2.0 * (2.0f32).sqrt();
        let sq = 2.0 * a.sqrt() * alpha;
        let mut bq = Biquad::default();
        bq.set(
            a * ((a + 1.0) + (a - 1.0) * cs + sq),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cs),
            a * ((a + 1.0) + (a - 1.0) * cs - sq),
            (a + 1.0) - (a - 1.0) * cs + sq,
            2.0 * ((a - 1.0) - (a + 1.0) * cs),
            (a + 1.0) - (a - 1.0) * cs - sq,
        );
        bq
    }

    /// RBJ peaking EQ.
    fn peaking(fs: f32, f0: f32, q: f32, gain_db: f32) -> Biquad {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * f0 / fs;
        let (sn, cs) = w0.sin_cos();
        let alpha = sn / (2.0 * q);
        let mut bq = Biquad::default();
        bq.set(
            1.0 + alpha * a,
            -2.0 * cs,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cs,
            1.0 - alpha / a,
        );
        bq
    }
}

/// Maps a `0.0..=1.0` control to dB: `0.5` → 0 dB (unity), `1.0` → +6 dB,
/// `0.0` → −40 dB (near kill).
fn control_to_db(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v >= 0.5 {
        (v - 0.5) / 0.5 * 6.0
    } else {
        // 0.5→0 dB down to 0.0→-40 dB.
        (v - 0.5) / 0.5 * 40.0
    }
}

/// A 3-band EQ for one audio channel.
#[derive(Clone, Copy, Debug, Default)]
pub struct Eq3 {
    low: Biquad,
    mid: Biquad,
    high: Biquad,
}

impl Eq3 {
    /// Recomputes coefficients for the given control gains `[low, mid, high]`
    /// (each `0.0..=1.0`) at sample rate `fs`.
    pub fn configure(&mut self, fs: f32, gains: [f32; 3]) {
        self.low = Biquad::low_shelf(fs, 200.0, control_to_db(gains[0]));
        self.mid = Biquad::peaking(fs, 1000.0, 0.9, control_to_db(gains[1]));
        self.high = Biquad::high_shelf(fs, 4000.0, control_to_db(gains[2]));
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.high.process(self.mid.process(self.low.process(x)))
    }

    /// Clears filter memory (e.g. on track load / seek).
    pub fn reset(&mut self) {
        self.low.reset();
        self.mid.reset();
        self.high.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, fs: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / fs).sin())
            .collect()
    }

    fn rms(s: &[f32]) -> f32 {
        (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
    }

    #[test]
    fn neutral_eq_is_transparent() {
        let fs = 44_100.0;
        let mut eq = Eq3::default();
        eq.configure(fs, [0.5, 0.5, 0.5]);
        let input = sine(1000.0, fs, 4096);
        let out: Vec<f32> = input.iter().map(|&x| eq.process(x)).collect();
        // Skip the filter warm-up transient.
        assert!((rms(&out[1000..]) - rms(&input[1000..])).abs() < 0.05);
    }

    #[test]
    fn killing_low_band_attenuates_bass() {
        let fs = 44_100.0;
        let mut eq = Eq3::default();
        eq.configure(fs, [0.0, 0.5, 0.5]); // kill lows
        let bass = sine(60.0, fs, 8192);
        let out: Vec<f32> = bass.iter().map(|&x| eq.process(x)).collect();
        assert!(
            rms(&out[2000..]) < rms(&bass[2000..]) * 0.5,
            "bass should be strongly attenuated"
        );
    }

    #[test]
    fn boosting_high_band_raises_treble() {
        let fs = 44_100.0;
        let mut eq = Eq3::default();
        eq.configure(fs, [0.5, 0.5, 1.0]); // +6 dB highs
        let treble = sine(8000.0, fs, 8192);
        let out: Vec<f32> = treble.iter().map(|&x| eq.process(x)).collect();
        assert!(
            rms(&out[2000..]) > rms(&treble[2000..]) * 1.2,
            "treble should be boosted"
        );
    }

    #[test]
    fn control_mapping_endpoints() {
        assert!((control_to_db(0.5) - 0.0).abs() < 1e-6);
        assert!((control_to_db(1.0) - 6.0).abs() < 1e-6);
        assert!(control_to_db(0.0) <= -39.0);
    }
}
