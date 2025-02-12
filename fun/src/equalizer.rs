// Low-pass filter: simple first-order filter.
fn low_pass_filter(buffer: &mut [f32], cutoff: f32) {
    let alpha = cutoff; // alpha in (0,1); adjust as needed
    let mut prev = buffer[0];
    for sample in buffer.iter_mut() {
        *sample = alpha * *sample + (1.0 - alpha) * prev;
        prev = *sample;
    }
}

// High-pass filter: simple first-order filter.
fn high_pass_filter(buffer: &mut [f32], cutoff: f32) {
    let alpha = cutoff; // alpha in (0,1); adjust as needed
    let mut prev_in = buffer[0];
    let mut prev_out = buffer[0];
    for sample in buffer.iter_mut() {
        let current = *sample;
        *sample = alpha * (prev_out + current - prev_in);
        prev_in = current;
        prev_out = *sample;
    }
}

// Q filter (mid-frequency adjustment): a placeholder filter.
fn q_filter(buffer: &mut [f32], gain: f32) {
    // Simple gain adjustment as an example.
    for sample in buffer.iter_mut() {
        *sample *= gain;
    }
}

/// Equalizer shoulder: applies low-pass, high-pass, and Q filters in sequence to a mono channel.
pub fn equalizer(buffer: &mut [f32]) {
    let low_pass_cutoff = 0.1; // Low-pass parameter.
    let high_pass_cutoff = 0.001; // Low high-pass cutoff to avoid silencing.
    let q_gain = 1.0;
    low_pass_filter(buffer, low_pass_cutoff);
    high_pass_filter(buffer, high_pass_cutoff);
    q_filter(buffer, q_gain);
}

/// Processes interleaved stereo data: splits channels, applies equalizer on each, then interleaves.
pub fn equalizer_stereo(buffer: &mut [f32]) {
    let n = buffer.len() / 2;
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    for i in 0..n {
        left.push(buffer[i * 2]);
        right.push(buffer[i * 2 + 1]);
    }
    equalizer(&mut left);
    equalizer(&mut right);
    for i in 0..n {
        buffer[i * 2] = left[i];
        buffer[i * 2 + 1] = right[i];
    }
}
