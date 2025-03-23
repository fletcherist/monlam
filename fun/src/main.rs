use std::error::Error;

const SAMPLE_RATE: u32 = 44100;

mod audio_effect;
mod audio_similarity;
mod eq_trainer;
mod parametric_eq;
mod putlec_eq;

use eq_trainer::EQTrainer;

/// Trait defining the interface for any audio effect

const INPUT_FILE_PATH: &str = "/Users/fletcherist/monlam/files/input.wav";
const OUTPUT_FILE_PATH: &str = "/Users/fletcherist/monlam/files/output.wav";
const REFERENCE_FILE_PATH: &str = "/Users/fletcherist/monlam/files/reference.wav";

fn main() {
    println!("Hello, world!");

    let eq_params = train_eq_to_match(INPUT_FILE_PATH, REFERENCE_FILE_PATH, 3).unwrap();
    println!("EQ parameters: {:?}", eq_params);

    // // Load the input WAV file from the directory.
    // match load_wav(INPUT_FILE_PATH) {
    //     Ok((mut buffer, in_channels)) => {
    //         // Create effect chain
    //         let mut chain = audio_effect::AudioEffectChain::new(SAMPLE_RATE);

    //         // Create and configure EQ
    //         let mut eq = parametric_eq::ParametricEQ::new(SAMPLE_RATE);
    //         eq.add_band(100.0, 1.0, 10.0);
    //         eq.add_band(1000.0, 2.0, 12.0);
    //         eq.add_band(4000.0, 0.5, 3.0);

    //         // Add EQ to chain
    //         chain.add_effect(eq);

    //         // Process chunks through the effect chain
    //         let chunk_size = 1024;
    //         for chunk in buffer.chunks_mut(chunk_size) {
    //             chain.process_buffer(chunk);
    //         }

    //         // Save the processed audio buffer to output file.
    //         match save_wav(OUTPUT_FILE_PATH, &buffer, in_channels) {
    //             Ok(_) => println!("Audio saved successfully."),
    //             Err(e) => eprintln!("Failed to save audio: {}", e),
    //         }
    //     }
    //     Err(e) => {
    //         eprintln!("Failed to process WAV file: {}", e);
    //     }
    // }
}

/// Loads a WAV file and returns a tuple containing a buffer of f32 samples
/// and its channel count. If the file is stereo, the samples are kept interleaved.
pub fn load_wav(file_path: &str) -> Result<(Vec<f32>, u16), Box<dyn Error>> {
    let mut reader = hound::WavReader::open(file_path)?;
    let spec = reader.spec();
    let channels = spec.channels;
    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Int {
        reader
            .samples::<i16>()
            .map(|s| s.map(|sample| sample as f32 / i16::MAX as f32))
            .collect::<Result<_, _>>()?
    } else {
        reader.samples::<f32>().collect::<Result<_, _>>()?
    };
    Ok((samples, channels))
}

/// Processes audio samples. If channels==2, splits and processes left/right channels separately;
/// otherwise processes the mono signal.
pub fn process_chunk(buffer: &mut [f32], channels: u16) {
    let mut eq = parametric_eq::ParametricEQ::new(SAMPLE_RATE);
    // Add some bands
    eq.add_band(100.0, 1.0, -20.0); // Cut 100Hz by 3dB
    eq.add_band(1000.0, 2.0, 12.0); // Boost 1kHz by 6dB
    eq.add_band(4000.0, 0.5, 3.0); // Boost 4kHz with wider Q

    // let mut eq = putlec_eq::new_pultec_eq(SAMPLE_RATE);

    if channels == 2 {
        eq.process_buffer(buffer);
    } else {
        // Process mono data.
        eq.process_buffer(buffer);
    }
}

/// Saves a buffer of f32 samples to an output WAV file using the input file's channel count.
/// When channels == 2, assumes the buffer is interleaved stereo; when channels == 1, writes mono.
pub fn save_wav(file_path: &str, buffer: &[f32], channels: u16) -> Result<(), Box<dyn Error>> {
    let spec = hound::WavSpec {
        channels,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(file_path, spec)?;
    let amplitude = i16::MAX as f32;
    if channels == 2 {
        // Buffer is already interleaved stereo; write directly.
        for sample in buffer {
            let s = (sample * amplitude) as i16;
            writer.write_sample(s)?;
        }
    } else {
        // Mono: write each sample.
        for sample in buffer {
            let s = (sample * amplitude) as i16;
            writer.write_sample(s)?;
        }
    }
    writer.finalize()?;
    Ok(())
}

// Add this function to your main.rs
pub fn train_eq_to_match(
    source_path: &str,
    target_path: &str,
    num_bands: usize,
) -> Result<Vec<(f32, f32, f32)>, Box<dyn Error>> {
    // Load source and target audio
    let (mut source_audio, source_channels) = load_wav(source_path)?;
    let (mut target_audio, target_channels) = load_wav(target_path)?;

    // Ensure both files have the same number of channels
    if source_channels != target_channels {
        return Err("Source and target must have same number of channels".into());
    }

    // Calculate sample positions for 30-35 second segment
    let start_sample = (30 * SAMPLE_RATE) as usize;
    let end_sample = (35 * SAMPLE_RATE) as usize;

    // Ensure both files are long enough
    if source_audio.len() < end_sample || target_audio.len() < end_sample {
        return Err("Audio files must be at least 35 seconds long".into());
    }

    // Crop to 30-35 second segment
    source_audio = source_audio[start_sample..end_sample].to_vec();
    target_audio = target_audio[start_sample..end_sample].to_vec();

    println!(
        "Processing 5 second segment from 30-35s ({} samples)",
        source_audio.len()
    );

    // Create and run trainer
    let trainer = EQTrainer::new(source_audio, target_audio, SAMPLE_RATE, num_bands);
    trainer.train()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_audio() {
        // For mono test.
        let buffer = vec![0.5, 0.3, 0.1, -0.2];
        let file_path = "test_output.wav";
        save_wav(file_path, &buffer, 1).unwrap();
        let (samples, _) = load_wav(file_path).unwrap();
        assert_eq!(samples, buffer);
        std::fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_pipeline() {
        // Create a mono input file.
        let input_path = "test_input.wav";
        let output_path = "test_output_pipeline.wav";
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(input_path, spec).unwrap();
        let amplitude = i16::MAX as f32;
        let input_samples = vec![0.8, 0.4, 0.2, -0.4];
        for sample in &input_samples {
            let s = (*sample * amplitude) as i16;
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();

        let (mut buffer, channels) = load_wav(input_path).unwrap();
        process_chunk(&mut buffer, channels);
        save_wav(output_path, &buffer, channels).unwrap();
        let (output_samples, _) = load_wav(output_path).unwrap();
        // Expected output adjusted by equalizer processing (placeholder behavior).
        // Here we assume equalizer processes signal to roughly half amplitude.
        let expected: Vec<f32> = input_samples.into_iter().map(|v| v * 0.5).collect();
        // Allow some tolerance.
        for (o, e) in output_samples.iter().zip(expected.iter()) {
            assert!((o - e).abs() < 0.1);
        }
        std::fs::remove_file(input_path).unwrap();
        std::fs::remove_file(output_path).unwrap();
    }

    #[test]
    fn test_eq_training() {
        // Train EQ
        let eq_params = train_eq_to_match(INPUT_FILE_PATH, REFERENCE_FILE_PATH, 3).unwrap();
        println!("EQ parameters: {:?}", eq_params);

        // Verify we got reasonable parameters
        assert_eq!(eq_params.len(), 3);
        for (freq, q, gain) in eq_params {
            assert!(freq >= 20.0 && freq <= 20000.0);
            assert!(q >= 0.1 && q <= 10.0);
            assert!(gain >= -24.0 && gain <= 24.0);
        }
    }
}
