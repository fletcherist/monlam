// parametric_eq.rs

// prompt: i want you to create an equalizer for audio in rust in a single file. equalizer uses bands with frequency, Q and Gain. there could be several bends. it's called parametric equalizer. no interface. no vst, clap or something. just a program that accepts audio buffer in 2 channels and processes it, doing equalisation. you can ask me questions

use std::f32::consts::PI;

pub struct ParametricEQ {
    sample_rate: u32,
    bands: Vec<Band>,
}

impl ParametricEQ {
    /// Create a new parametric EQ with the given sample rate
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            bands: Vec::new(),
        }
    }

    /// Add a new band to the equalizer
    pub fn add_band(&mut self, frequency: f32, q: f32, gain_db: f32) {
        self.bands
            .push(Band::new(frequency, q, gain_db, self.sample_rate));
    }

    /// Process an interleaved stereo buffer in-place
    pub fn process_buffer(&mut self, buffer: &mut [f32]) {
        for frame in buffer.chunks_exact_mut(2) {
            let mut left = frame[0];
            let mut right = frame[1];

            for band in &mut self.bands {
                left = band.process_left(left);
                right = band.process_right(right);
            }

            frame[0] = left;
            frame[1] = right;
        }
    }
}

struct Band {
    // Filter coefficients
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    // Left channel state
    l_x1: f32,
    l_x2: f32,
    l_y1: f32,
    l_y2: f32,
    // Right channel state
    r_x1: f32,
    r_x2: f32,
    r_y1: f32,
    r_y2: f32,
}

impl Band {
    fn new(frequency: f32, q: f32, gain_db: f32, sample_rate: u32) -> Self {
        let sample_rate = sample_rate as f32;
        let omega = 2.0 * PI * frequency / sample_rate;
        let sn = omega.sin();
        let cs = omega.cos();
        let alpha = sn / (2.0 * q);
        let a = 10.0f32.powf(gain_db / 40.0);

        // Calculate coefficients
        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cs;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cs;
        let a2 = 1.0 - alpha / a;

        // Normalize coefficients
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
        }
    }

    fn process_left(&mut self, sample: f32) -> f32 {
        let x = sample;
        let y = self.b0 * x + self.b1 * self.l_x1 + self.b2 * self.l_x2
            - self.a1 * self.l_y1
            - self.a2 * self.l_y2;

        // Update state
        self.l_x2 = self.l_x1;
        self.l_x1 = x;
        self.l_y2 = self.l_y1;
        self.l_y1 = y;

        y
    }

    fn process_right(&mut self, sample: f32) -> f32 {
        let x = sample;
        let y = self.b0 * x + self.b1 * self.r_x1 + self.b2 * self.r_x2
            - self.a1 * self.r_y1
            - self.a2 * self.r_y2;

        // Update state
        self.r_x2 = self.r_x1;
        self.r_x1 = x;
        self.r_y2 = self.r_y1;
        self.r_y1 = y;

        y
    }
}
