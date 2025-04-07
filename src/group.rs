use crate::audio::load_audio;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// A Group is a container for audio content that can be saved and reused in projects.
/// Each Group is stored as a folder in the project directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// User-friendly name of the Group
    pub name: String,
    
    /// Path to the Group folder
    #[serde(skip)]
    pub path: PathBuf,
    
    /// Path to the rendered audio file (typically {group_name}/render.wav)
    #[serde(skip)]
    pub render_path: PathBuf,
    
    /// Waveform data for visualization
    #[serde(skip)]
    pub waveform: Vec<f32>,
    
    /// Sample rate of the rendered audio
    #[serde(skip)]
    pub sample_rate: u32,
    
    /// Duration in seconds
    #[serde(skip)]
    pub duration: f32,
}

impl Group {
    /// Create a new Group with the given name in the project directory
    pub fn new(name: &str, project_dir: &Path) -> Result<Self, String> {
        // Validate the name
        if name.contains('/') {
            return Err("Group name cannot contain '/'".to_string());
        }
        
        if name.trim().is_empty() {
            return Err("Group name cannot be empty".to_string());
        }
        
        // Create the group directory
        let group_path = project_dir.join(name);
        if group_path.exists() {
            return Err(format!("A Group with name '{}' already exists", name));
        }
        
        match fs::create_dir_all(&group_path) {
            Ok(_) => {
                // Create empty render.wav placeholder
                // The actual rendering happens separately
                let render_path = group_path.join("render.wav");
                
                Ok(Self {
                    name: name.to_string(),
                    path: group_path,
                    render_path,
                    waveform: Vec::new(),
                    sample_rate: 44100, // Default sample rate
                    duration: 0.0,
                })
            }
            Err(e) => Err(format!("Failed to create Group directory: {}", e)),
        }
    }
    
    /// Load a Group from an existing directory
    pub fn load(group_path: &Path) -> Result<Self, String> {
        if !group_path.exists() || !group_path.is_dir() {
            return Err(format!("Group directory not found: {:?}", group_path));
        }
        
        let name = group_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();
        
        let render_path = group_path.join("render.wav");
        
        let mut result = Self {
            name,
            path: group_path.to_path_buf(),
            render_path: render_path.clone(),
            waveform: Vec::new(),
            sample_rate: 44100,
            duration: 0.0,
        };
        
        // Load waveform data if render.wav exists
        if render_path.exists() {
            match load_audio(&render_path) {
                Ok((samples, sample_rate)) => {
                    result.waveform = generate_waveform(&samples, 1000);
                    result.sample_rate = sample_rate;
                    result.duration = samples.len() as f32 / sample_rate as f32;
                }
                Err(e) => {
                    eprintln!("Failed to load Group audio: {:?}", e);
                }
            }
        }
        
        Ok(result)
    }
    
    /// Render the Group contents to render.wav
    pub fn render(&mut self, audio_data: &[f32], sample_rate: u32) -> Result<(), String> {
        use hound::{SampleFormat, WavSpec, WavWriter};
        
        // Create the necessary directories if they don't exist
        if let Some(parent) = self.render_path.parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return Err(format!("Failed to create Group directory: {}", e));
                }
            }
        }
        
        // Create WAV spec
        let spec = WavSpec {
            channels: 2, // Stereo output
            sample_rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        
        // Create WAV writer
        let writer = match WavWriter::create(&self.render_path, spec) {
            Ok(writer) => writer,
            Err(e) => return Err(format!("Failed to create WAV file: {}", e)),
        };
        
        // Convert mono to stereo if necessary and write samples
        let result = if audio_data.len() % 2 == 0 {
            // Data is already stereo format
            write_audio_to_wav(writer, audio_data)
        } else {
            // Data is mono, duplicate to stereo
            let mut stereo_data = Vec::with_capacity(audio_data.len() * 2);
            for sample in audio_data {
                stereo_data.push(*sample); // Left channel
                stereo_data.push(*sample); // Right channel
            }
            write_audio_to_wav(writer, &stereo_data)
        };
        
        if let Err(e) = result {
            return Err(format!("Failed to write audio data: {}", e));
        }
        
        // Update waveform data
        self.waveform = generate_waveform(audio_data, 1000);
        self.sample_rate = sample_rate;
        self.duration = audio_data.len() as f32 / sample_rate as f32;
        
        Ok(())
    }
    
    /// Rename the Group (updates both name and directory)
    pub fn rename(&mut self, new_name: &str, project_dir: &Path) -> Result<(), String> {
        // Validate the new name
        if new_name.contains('/') {
            return Err("Group name cannot contain '/'".to_string());
        }
        
        if new_name.trim().is_empty() {
            return Err("Group name cannot be empty".to_string());
        }
        
        // Create the new group directory path
        let new_group_path = project_dir.join(new_name);
        if new_group_path.exists() && new_group_path != self.path {
            return Err(format!("A Group with name '{}' already exists", new_name));
        }
        
        // Rename the directory
        if let Err(e) = fs::rename(&self.path, &new_group_path) {
            return Err(format!("Failed to rename Group: {}", e));
        }
        
        // Update the Group object
        self.name = new_name.to_string();
        self.path = new_group_path;
        self.render_path = self.path.join("render.wav");
        
        Ok(())
    }
}

/// Helper function to write audio data to a WAV file
fn write_audio_to_wav(mut writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>, audio_data: &[f32]) -> Result<(), hound::Error> {
    for sample in audio_data {
        writer.write_sample(*sample)?;
    }
    writer.finalize()
}

/// Generate a downsampled waveform for visualization
fn generate_waveform(samples: &[f32], target_size: usize) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }
    
    let samples_per_point = samples.len() / target_size;
    if samples_per_point <= 1 {
        return samples.to_vec();
    }
    
    let mut waveform = Vec::with_capacity(target_size);
    
    for i in 0..target_size {
        let start = i * samples_per_point;
        let end = std::cmp::min((i + 1) * samples_per_point, samples.len());
        
        if start < end {
            // Find the maximum amplitude in this segment
            let mut max_amplitude = 0.0f32;
            for j in start..end {
                let amplitude = samples[j].abs();
                if amplitude > max_amplitude {
                    max_amplitude = amplitude;
                }
            }
            waveform.push(max_amplitude);
        } else {
            break;
        }
    }
    
    waveform
} 