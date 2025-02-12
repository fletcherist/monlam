use crate::audio_similarity::audio_similarity;
use crate::parametric_eq::ParametricEQ;
use cmaes::{CMAESOptions, DVector};
use std::error::Error;

pub struct EQTrainer {
    source_audio: Vec<f32>,
    target_audio: Vec<f32>,
    sample_rate: u32,
    num_bands: usize,
}

impl EQTrainer {
    pub fn new(
        source_audio: Vec<f32>,
        target_audio: Vec<f32>,
        sample_rate: u32,
        num_bands: usize,
    ) -> Self {
        Self {
            source_audio,
            target_audio,
            sample_rate,
            num_bands,
        }
    }

    // Add these helper functions for parameter mapping
    fn map_frequency(normalized: f32) -> f32 {
        // Map 0-1 to frequency range 20Hz-20kHz (logarithmically)
        20.0 * (1000.0f32).powf(normalized)
    }

    fn map_q(normalized: f32) -> f32 {
        // Map 0-1 to Q range 0.1-10 (logarithmically)
        0.1 * (100.0f32).powf(normalized)
    }

    fn map_gain(normalized: f32) -> f32 {
        // Map 0-1 to gain range -24dB to +24dB (linearly)
        normalized * 48.0 - 24.0
    }

    pub fn train(&self) -> Result<Vec<(f32, f32, f32)>, Box<dyn Error>> {
        // Each band has 3 parameters: frequency (20-20000 Hz), Q (0.1-10), gain (-24 to +24 dB)
        let num_parameters = self.num_bands * 3;

        // Define the objective function
        let objective = |x: &DVector<f64>| {
            // Create EQ with current parameters
            let mut eq = ParametricEQ::new(self.sample_rate);

            println!("Processing parameters: {:?}", x);
            // Map normalized parameters to actual EQ values
            for i in 0..self.num_bands {
                let base = i * 3;
                let normalized = [x[base] as f32, x[base + 1] as f32, x[base + 2] as f32];

                let frequency = Self::map_frequency(normalized[0]);
                let q = Self::map_q(normalized[1]);
                let gain_db = Self::map_gain(normalized[2]);

                println!(
                    "Band {}: freq={:.1}Hz, Q={:.2}, gain={:.1}dB",
                    i, frequency, q, gain_db
                );

                eq.add_band(frequency, q, gain_db);
            }

            // Process audio through EQ
            let mut processed_audio = self.source_audio.clone();
            eq.process_buffer(&mut processed_audio);

            // Calculate similarity between processed and target audio
            let similarity = audio_similarity(&processed_audio, &self.target_audio);

            // Convert similarity to cost (CMA-ES minimizes)
            let cost = 1.0 - similarity as f64;
            println!("Cost: {}", cost);
            cost
        };

        // Initialize CMA-ES with values in 0-1 range
        let mut optimizer = CMAESOptions::new(vec![0.5; num_parameters], 0.1)
            .fun_target(1e-8)
            .max_generations(100)
            .enable_printing(10)
            .build(objective)
            .unwrap();

        // Run optimization and log results
        let result = optimizer.run();
        println!("Optimization terminated due to: {:?}", result.reasons);
        println!("Current best fitness: {:?}", result.current_best); // Changed to {:?}

        // Get the best solution, return error if none found
        let best_solution = result
            .overall_best
            .ok_or("No solution found during optimization")?;

        // Convert normalized results back to EQ parameters
        let mut eq_params = Vec::with_capacity(self.num_bands);
        for i in 0..self.num_bands {
            let base = i * 3;
            let normalized = [
                best_solution.point[base] as f32,
                best_solution.point[base + 1] as f32,
                best_solution.point[base + 2] as f32,
            ];

            eq_params.push((
                Self::map_frequency(normalized[0]),
                Self::map_q(normalized[1]),
                Self::map_gain(normalized[2]),
            ));
        }

        Ok(eq_params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eq_trainer() {
        // Create dummy audio data
        let source = vec![0.0f32; 44100];
        let target = vec![0.5f32; 44100];

        let trainer = EQTrainer::new(source, target, 44100, 3);
        let result = trainer.train().unwrap();

        assert_eq!(result.len(), 3); // Should have 3 bands

        // Check if parameters are within reasonable bounds
        for (freq, q, gain) in result {
            assert!(freq >= 20.0 && freq <= 20000.0);
            assert!(q >= 0.1 && q <= 10.0);
            assert!(gain >= -24.0 && gain <= 24.0);
        }
    }
}
