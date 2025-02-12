use std::f32;
use std::f32::consts::PI;

pub struct AnalogEQ {
    sample_rate: u32,
    bands: Vec<Band>,
    tone: f32,       // Global tone control (0.0-1.0)
    saturation: f32, // Global saturation (0.0-1.0)
}

impl AnalogEQ {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            bands: Vec::new(),
            tone: 0.5,
            saturation: 0.3,
        }
    }

    pub fn add_pultec_low(&mut self, frequency: f32, boost_db: f32, cut_db: f32) {
        // Pultec-style low-end: simultaneous boost and cut
        self.bands.push(Band::new(
            frequency,
            1.0 + boost_db.abs() / 6.0, // Q increases with boost
            boost_db,
            self.sample_rate,
            AnalogMode::PultecLowBoost,
        ));

        self.bands.push(Band::new(
            frequency,
            0.5 + cut_db.abs() / 12.0, // Different Q for cut
            cut_db,
            self.sample_rate,
            AnalogMode::PultecLowCut,
        ));
    }

    pub fn add_pultec_high(&mut self, frequency: f32, boost_db: f32) {
        self.bands.push(Band::new(
            frequency,
            0.7, // Fixed Q for high shelf
            boost_db,
            self.sample_rate,
            AnalogMode::PultecHigh,
        ));
    }

    pub fn process_buffer(&mut self, buffer: &mut [f32]) {
        for frame in buffer.chunks_exact_mut(2) {
            let mut left = frame[0];
            let mut right = frame[1];

            // Apply analog tone coloration
            let (l, r) = self.apply_tone(left, right);
            left = l;
            right = r;

            // Process bands
            for band in &mut self.bands {
                left = band.process_left(left);
                right = band.process_right(right);
            }

            // Apply global saturation
            left = self.analog_saturation(left);
            right = self.analog_saturation(right);

            frame[0] = left;
            frame[1] = right;
        }
    }

    fn apply_tone(&self, left: f32, right: f32) -> (f32, f32) {
        // Simple high-frequency roll-off simulation
        let tone_factor = 1.0 - self.tone;
        let l = left * (1.0 - tone_factor * 0.2);
        let r = right * (1.0 - tone_factor * 0.2);
        (l, r)
    }

    fn analog_saturation(&self, sample: f32) -> f32 {
        // Cubic soft-clipping with drive control
        let drive = 1.0 + self.saturation * 3.0;
        let s = sample * drive;
        s - s * s * s / 3.0
    }
}

#[derive(Clone, Copy)]
enum AnalogMode {
    PultecLowBoost,
    PultecLowCut,
    PultecHigh,
    VintageBell,
}

struct Band {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    l_x1: f32,
    l_x2: f32,
    l_y1: f32,
    l_y2: f32,
    r_x1: f32,
    r_x2: f32,
    r_y1: f32,
    r_y2: f32,
    mode: AnalogMode,
    saturation: f32,
}

impl Band {
    fn new(frequency: f32, q: f32, gain_db: f32, sample_rate: u32, mode: AnalogMode) -> Self {
        let mut freq = Self::analog_frequency_warp(frequency, mode);
        let (q, gain_db) = Self::analog_gain_compensation(q, gain_db, mode);

        let sample_rate = sample_rate as f32;
        let omega = 2.0 * PI * freq / sample_rate;
        let sn = omega.sin();
        let cs = omega.cos();
        let alpha = sn / (2.0 * q);

        let a = match mode {
            AnalogMode::PultecLowBoost => 10.0f32.powf(gain_db / 20.0),
            _ => 10.0f32.powf(gain_db / 40.0),
        };

        // Different coefficient calculations based on mode
        let (b0, b1, b2, a0, a1, a2) = match mode {
            AnalogMode::PultecHigh => {
                // High shelf filter
                let a = 10.0f32.powf(gain_db / 40.0);
                let sqrt_a = a.sqrt();
                let ap1 = sqrt_a + 1.0;
                let am1 = sqrt_a - 1.0;

                let b0 = a * (ap1 - am1 * cs + sn * alpha);
                let b1 = 2.0 * a * (am1 - ap1 * cs);
                let b2 = a * (ap1 - am1 * cs - sn * alpha);
                let a0 = ap1 + am1 * cs + sn * alpha;
                let a1 = -2.0 * (am1 + ap1 * cs);
                let a2 = ap1 + am1 * cs - sn * alpha;

                (b0, b1, b2, a0, a1, a2)
            }
            _ => {
                // Standard peaking filter with analog tweaks
                let b0 = 1.0 + alpha * a;
                let b1 = -2.0 * cs;
                let b2 = 1.0 - alpha * a;
                let a0 = 1.0 + alpha / a;
                let a1 = -2.0 * cs;
                let a2 = 1.0 - alpha / a;
                (b0, b1, b2, a0, a1, a2)
            }
        };

        let norm = 1.0 / a0;
        let coeff = |val| val * norm;

        Self {
            b0: coeff(b0),
            b1: coeff(b1),
            b2: coeff(b2),
            a1: coeff(a1),
            a2: coeff(a2),
            l_x1: 0.0,
            l_x2: 0.0,
            l_y1: 0.0,
            l_y2: 0.0,
            r_x1: 0.0,
            r_x2: 0.0,
            r_y1: 0.0,
            r_y2: 0.0,
            mode,
            saturation: match mode {
                AnalogMode::PultecLowBoost => 0.4,
                AnalogMode::PultecLowCut => 0.2,
                _ => 0.1,
            },
        }
    }

    fn analog_frequency_warp(freq: f32, mode: AnalogMode) -> f32 {
        // Simulate analog frequency response inaccuracies
        match mode {
            AnalogMode::PultecLowBoost | AnalogMode::PultecLowCut => freq * 0.98,
            AnalogMode::PultecHigh => freq * 1.03,
            _ => freq,
        }
    }

    fn analog_gain_compensation(q: f32, gain_db: f32, mode: AnalogMode) -> (f32, f32) {
        // Analog-style Q/gain relationships
        match mode {
            AnalogMode::PultecLowBoost => (q * (1.0 + gain_db.abs() / 10.0), gain_db * 0.9),
            AnalogMode::PultecLowCut => (q * 0.8, gain_db * 1.1),
            AnalogMode::PultecHigh => (q * 0.7, gain_db),
            _ => (q, gain_db),
        }
    }

    fn process_left(&mut self, sample: f32) -> f32 {
        let x = sample;
        let y = self.b0 * x + self.b1 * self.l_x1 + self.b2 * self.l_x2
            - self.a1 * self.l_y1
            - self.a2 * self.l_y2;

        self.l_x2 = self.l_x1;
        self.l_x1 = x;
        self.l_y2 = self.l_y1;
        self.l_y1 = y;

        // Band-specific saturation
        self.analog_tape_effect(y)
    }

    fn process_right(&mut self, sample: f32) -> f32 {
        let x = sample;
        let y = self.b0 * x + self.b1 * self.r_x1 + self.b2 * self.r_x2
            - self.a1 * self.r_y1
            - self.a2 * self.r_y2;

        self.r_x2 = self.r_x1;
        self.r_x1 = x;
        self.r_y2 = self.r_y1;
        self.r_y1 = y;

        self.analog_tape_effect(y)
    }

    fn analog_tape_effect(&self, sample: f32) -> f32 {
        // Gentle saturation curve
        let s = sample * (1.0 + self.saturation);
        s / (1.0 + s.abs()).sqrt()
    }
}

// Example usage simulating Pultec EQP-1A characteristics
pub fn new_pultec_eq(sample_rate: u32) -> AnalogEQ {
    let mut eq = AnalogEQ::new(sample_rate);

    // Pultec-style low-end with simultaneous boost and cut
    eq.add_pultec_low(60.0, 3.0, -3.0);
    // High shelf
    eq.add_pultec_high(12000.0, 4.0);
    // Add vintage bell curve
    eq.bands.push(Band::new(
        800.0,
        1.5,
        2.0,
        sample_rate,
        AnalogMode::VintageBell,
    ));

    eq.tone = 0.6; // Warmer tone
    eq.saturation = 0.01; // More saturation

    eq
}
