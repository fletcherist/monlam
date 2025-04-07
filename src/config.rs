use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    latest_project: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WaveformData {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub duration: f32,
}


pub fn get_project_dir(project_path: &Path) -> PathBuf {
    let project_folder = project_path.parent().unwrap_or(Path::new("")).to_path_buf();
    project_folder.join("waveforms")
}

pub fn save_waveform_data(
    project_path: &Path,
    track_name: &str,
    waveform_data: &WaveformData,
) -> Option<PathBuf> {
    let project_dir = get_project_dir(project_path);
    if let Err(e) = std::fs::create_dir_all(&project_dir) {
        eprintln!("Failed to create waveforms directory: {}", e);
        return None;
    }

    let waveform_path = project_dir.join(format!("{}.json", track_name));
    if let Ok(serialized) = serde_json::to_string_pretty(waveform_data) {
        if let Err(e) = fs::write(&waveform_path, serialized) {
            eprintln!("Failed to save waveform data: {}", e);
            return None;
        }
        eprintln!("Saved waveform data to project folder: {}", waveform_path.display());
        Some(waveform_path)
    } else {
        None
    }
}

pub fn load_waveform_data(waveform_path: &Path) -> Option<WaveformData> {
    // First try loading from the provided path
    if let Ok(contents) = fs::read_to_string(waveform_path) {
        if let Ok(data) = serde_json::from_str::<WaveformData>(&contents) {
            eprintln!("Loaded waveform data from: {}", waveform_path.display());
            return Some(data);
        }
    }
    
    // If that fails, try alternate locations
    
    // If current path is in project folder, try .monlam folder
    if waveform_path.to_string_lossy().contains("waveforms") {
        // Try to load from .monlam folder
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let filename = waveform_path.file_name().unwrap_or_default();
        let project_name = waveform_path
            .parent()  // waveforms dir
            .and_then(|p| p.parent())  // project dir
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed");
        
        let old_path = Path::new(&home)
            .join(".monlam")
            .join("projects")
            .join(project_name)
            .join(filename);
            
        if let Ok(contents) = fs::read_to_string(&old_path) {
            if let Ok(data) = serde_json::from_str::<WaveformData>(&contents) {
                eprintln!("Loaded waveform data from legacy path: {}", old_path.display());
                return Some(data);
            }
        }
    } 
    // If current path is in .monlam folder, try project folder
    else if waveform_path.to_string_lossy().contains(".monlam") {
        // Extract the project name and filename
        
        // Since we don't know the project folder path, we can't load from it
        // This would require more context about the current project path
        eprintln!("Cannot convert legacy path to project path: {}", waveform_path.display());
    }
    
    eprintln!("Failed to load waveform data from: {}", waveform_path.display());
    None
}


