//! Lightweight seven-band peaking equalizer for the decoded audio path.

pub const FREQUENCIES: [f32; 7] = [60.0, 150.0, 400.0, 1_000.0, 2_400.0, 6_000.0, 15_000.0];
pub const FLAT: [f32; 7] = [0.0; 7];
pub const PRESETS: [(&str, [f32; 7]); 6] = [
    ("Flat", FLAT),
    ("Pop", [-1.0, 2.0, 4.0, 3.0, 0.0, -1.0, 1.0]),
    ("Rock", [4.0, 3.0, -1.0, -2.0, 2.0, 4.0, 5.0]),
    ("Classical", [4.0, 3.0, 2.0, 0.0, -2.0, 0.0, 3.0]),
    ("Jazz", [3.0, 2.0, 0.0, 2.0, -2.0, 2.0, 3.0]),
    ("Bass", [5.0, 4.0, 3.0, 1.0, 0.0, -1.0, 0.0]),
];

pub fn validate_bands(bands: [f32; 7]) -> [f32; 7] {
    bands.map(|gain| {
        if gain.is_finite() {
            gain.clamp(-12.0, 12.0)
        } else {
            0.0
        }
    })
}

pub fn parse_bands(value: &str) -> Option<[f32; 7]> {
    let parsed: Vec<f32> = value
        .split(',')
        .map(str::trim)
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let bands: [f32; 7] = parsed.try_into().ok()?;
    Some(validate_bands(bands))
}

pub fn encode_bands(bands: [f32; 7]) -> String {
    validate_bands(bands)
        .iter()
        .map(|gain| format!("{gain:.1}"))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn validate_bass_boost(db: f32) -> f32 {
    if db.is_finite() {
        db.clamp(0.0, 6.0)
    } else {
        0.0
    }
}

pub fn parse_bass_boost_db(value: &str) -> Option<f32> {
    value
        .parse::<f32>()
        .ok()
        .filter(|db| db.is_finite())
        .map(validate_bass_boost)
}

pub fn preset_name(bands: &[f32; 7]) -> &'static str {
    PRESETS
        .iter()
        .find_map(|(name, preset)| {
            preset
                .iter()
                .zip(bands)
                .all(|(a, b)| (a - b).abs() < 0.05)
                .then_some(*name)
        })
        .unwrap_or("Custom")
}

#[derive(Clone, Copy, Default)]
struct State {
    z1: f32,
    z2: f32,
}

struct PeakingFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    target: [f32; 5],
    ramp_left: usize,
    ramp_samples: usize,
    states: Vec<State>,
}

impl PeakingFilter {
    fn new(frequency: f32, sample_rate: f32, channels: usize) -> Self {
        let mut filter = Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            target: [1.0, 0.0, 0.0, 0.0, 0.0],
            ramp_left: 0,
            ramp_samples: (sample_rate * channels.max(1) as f32 * 0.02) as usize,
            states: vec![State::default(); channels.max(1)],
        };
        filter.set_gain(frequency, sample_rate, 0.0);
        filter
    }

    fn set_gain(&mut self, frequency: f32, sample_rate: f32, gain_db: f32) {
        if gain_db.abs() < 0.001 {
            self.target = [1.0, 0.0, 0.0, 0.0, 0.0];
        } else {
            let nyquist_safe_frequency = frequency.min(sample_rate * 0.45).max(1.0);
            let a = 10.0_f32.powf(gain_db / 40.0);
            let omega = std::f32::consts::TAU * nyquist_safe_frequency / sample_rate.max(1.0);
            let alpha = omega.sin() / 2.0; // Q = 1
            let cos = omega.cos();
            let a0 = 1.0 + alpha / a;
            self.target = [
                (1.0 + alpha * a) / a0,
                (-2.0 * cos) / a0,
                (1.0 - alpha * a) / a0,
                (-2.0 * cos) / a0,
                (1.0 - alpha / a) / a0,
            ];
        }
        self.ramp_left = self.ramp_samples;
    }

    fn process(&mut self, sample: f32, channel: usize) -> f32 {
        if self.ramp_left > 0 {
            let remaining = self.ramp_left as f32;
            self.b0 += (self.target[0] - self.b0) / remaining;
            self.b1 += (self.target[1] - self.b1) / remaining;
            self.b2 += (self.target[2] - self.b2) / remaining;
            self.a1 += (self.target[3] - self.a1) / remaining;
            self.a2 += (self.target[4] - self.a2) / remaining;
            self.ramp_left -= 1;
        }
        let index = channel % self.states.len();
        let state = &mut self.states[index];
        let out = self.b0 * sample + state.z1;
        state.z1 = self.b1 * sample - self.a1 * out + state.z2;
        state.z2 = self.b2 * sample - self.a2 * out;
        out
    }

    fn reset(&mut self) {
        self.states.fill(State::default());
    }
}

/// A set of seven real-time filters. Configuration changes rebuild coefficients only
/// when requested; sample processing itself allocates nothing.
pub struct Equalizer {
    sample_rate: f32,
    filters: Vec<PeakingFilter>,
    gains: [f32; 7],
    bass_boost_db: f32,
    headroom: f32,
    active: bool,
}

impl Equalizer {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        let sample_rate = sample_rate.max(1) as f32;
        let channels = channels.max(1) as usize;
        Self {
            sample_rate,
            filters: FREQUENCIES
                .iter()
                .map(|&hz| PeakingFilter::new(hz, sample_rate, channels))
                .collect(),
            gains: FLAT,
            bass_boost_db: 0.0,
            headroom: 1.0,
            active: false,
        }
    }

    pub fn set(&mut self, gains: [f32; 7], bass_boost_db: f32) {
        self.gains = validate_bands(gains);
        self.bass_boost_db = validate_bass_boost(bass_boost_db);
        self.active = self.bass_boost_db > 0.0 || self.gains.iter().any(|gain| gain.abs() >= 0.001);
        for (i, filter) in self.filters.iter_mut().enumerate() {
            let bass = if i == 0 { self.bass_boost_db } else { 0.0 };
            filter.set_gain(FREQUENCIES[i], self.sample_rate, self.gains[i] + bass);
        }
        // Leave conservative headroom for the strongest peak and its bass boost.
        let peak = self
            .gains
            .iter()
            .copied()
            .fold(0.0_f32, f32::max)
            .max(self.gains[0] + self.bass_boost_db)
            .max(0.0);
        self.headroom = 10.0_f32.powf(-peak / 20.0);
    }

    pub fn process(&mut self, mut sample: f32, channel: usize) -> f32 {
        if !self.active {
            return sample;
        }
        for filter in &mut self.filters {
            sample = filter.process(sample, channel);
        }
        // Floating point filters can overshoot their configured peak slightly. A soft
        // ceiling catches only those near-full-scale transients, avoiding hard clipping.
        sample *= self.headroom;
        if sample.abs() > 0.98 {
            sample = sample.signum() * (0.98 + 0.02 * ((sample.abs() - 0.98) / 0.02).tanh());
        }
        sample
    }

    pub fn reset(&mut self) {
        for filter in &mut self.filters {
            filter.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_valid_and_flat_is_unity() {
        assert_eq!(PRESETS[0].1, FLAT);
        for (_, bands) in PRESETS {
            assert_eq!(validate_bands(bands), bands);
        }
        assert_eq!(preset_name(&PRESETS[2].1), "Rock");
    }

    #[test]
    fn manual_bands_are_clamped_and_non_finite_values_are_reset() {
        assert_eq!(
            validate_bands([20.0, -20.0, f32::NAN, f32::INFINITY, 0.0, 0.0, 0.0]),
            [12.0, -12.0, 0.0, 0.0, 0.0, 0.0, 0.0]
        );
        assert!(parse_bands("1,2,3").is_none());
        assert_eq!(
            parse_bands("20,-20,0,0,0,0,0"),
            Some([12.0, -12.0, 0.0, 0.0, 0.0, 0.0, 0.0])
        );
    }

    #[test]
    fn bass_boost_is_bounded_and_filter_output_stays_finite() {
        let mut eq = Equalizer::new(48_000, 2);
        eq.set([12.0; 7], 99.0);
        for _ in 0..20_000 {
            let sample = eq.process(0.9, 0);
            assert!(sample.is_finite() && sample.abs() <= 1.0);
        }
    }

    #[test]
    fn enabled_equalizer_changes_the_decoded_samples() {
        let mut eq = Equalizer::new(48_000, 2);
        eq.set([0.0, 0.0, 0.0, -9.0, 0.0, 0.0, 0.0], 0.0);
        let mut input_energy = 0.0;
        let mut output_energy = 0.0;
        for frame in 0..4_800 {
            let sample = (std::f32::consts::TAU * 1_000.0 * frame as f32 / 48_000.0).sin() * 0.1;
            let output = eq.process(sample, 0);
            if frame >= 2_400 {
                input_energy += sample * sample;
                output_energy += output * output;
            }
        }
        assert!(output_energy < input_energy * 0.4);
    }

    #[test]
    fn saved_bands_round_trip() {
        let bands = [1.5, -2.0, 0.0, 3.0, 0.0, 0.0, -1.0];
        assert_eq!(parse_bands(&encode_bands(bands)), Some(bands));
    }

    #[test]
    fn bass_boost_setting_is_validated_and_round_trips() {
        assert_eq!(parse_bass_boost_db("3.5"), Some(3.5));
        assert_eq!(parse_bass_boost_db("9"), Some(6.0));
        assert_eq!(parse_bass_boost_db("NaN"), None);
        assert_eq!(parse_bass_boost_db("broken"), None);
    }
}
