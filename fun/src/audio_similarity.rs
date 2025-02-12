use num_complex::Complex;
use rustfft::{FftDirection, FftPlanner};

pub fn audio_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert!(
        a.iter().all(|x| !x.is_nan()),
        "Buffer 'a' contains NaN values"
    );
    assert!(
        b.iter().all(|x| !x.is_nan()),
        "Buffer 'b' contains NaN values"
    );
    assert_eq!(a.len(), b.len(), "Buffers must be of the same length");

    let features_a = compute_spectral_features(a);
    let features_b = compute_spectral_features(b);

    cosine_similarity(&features_a, &features_b)
}

fn compute_spectral_features(audio_buffer: &[f32]) -> Vec<f32> {
    let n = audio_buffer.len();
    let mut buffer: Vec<Complex<f32>> =
        audio_buffer.iter().map(|&x| Complex::new(x, 0.0)).collect();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);

    fft.process(&mut buffer);

    let magnitudes: Vec<f32> = buffer.iter().map(|c| c.norm()).collect();
    let num_bins = (n / 2) + 1;

    magnitudes[0..num_bins].to_vec()
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Feature vectors must be same length");

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();

    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-10); // Avoid division by zero

    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-10); // Avoid division by zero

    (dot_product / (norm_a * norm_b)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_buffers() {
        let buffer = vec![0.5; 1024];
        assert!((audio_similarity(&buffer, &buffer) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_zero_buffers() {
        let buffer = vec![0.0; 1024];
        assert!((audio_similarity(&buffer, &buffer) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_different_buffers() {
        let buffer1 = vec![0.5; 1024];
        let buffer2 = vec![-0.5; 1024]; // Phase difference shouldn't affect magnitude comparison
        let similarity = audio_similarity(&buffer1, &buffer2);
        assert!(similarity > 0.9 && similarity <= 1.0);
    }
}
