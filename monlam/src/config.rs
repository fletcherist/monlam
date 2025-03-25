use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
pub struct Config {
    latest_project: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
pub struct WaveformData {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub duration: f32,
}

pub fn get_config_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".monlam").join("config.json")
}

pub fn get_project_dir(project_path: &Path) -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let project_name = project_path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed");
    Path::new(&home)
        .join(".monlam")
        .join("projects")
        .join(project_name)
}

pub fn save_waveform_data(
    project_path: &Path,
    track_name: &str,
    waveform_data: &WaveformData,
) -> Option<PathBuf> {
    let project_dir = get_project_dir(project_path);
    if let Err(e) = std::fs::create_dir_all(&project_dir) {
        eprintln!("Failed to create project directory: {}", e);
        return None;
    }

    let waveform_path = project_dir.join(format!("{}.json", track_name));
    if let Ok(serialized) = serde_json::to_string_pretty(waveform_data) {
        if let Err(e) = fs::write(&waveform_path, serialized) {
            eprintln!("Failed to save waveform data: {}", e);
            return None;
        }
        Some(waveform_path)
    } else {
        None
    }
}

pub fn load_waveform_data(waveform_path: &Path) -> Option<WaveformData> {
    if let Ok(contents) = fs::read_to_string(waveform_path) {
        if let Ok(data) = serde_json::from_str::<WaveformData>(&contents) {
            return Some(data);
        }
    }
    None
}

pub fn save_config(project_path: Option<PathBuf>) {
    let config = Config {
        latest_project: project_path,
    };
    if let Ok(serialized) = serde_json::to_string_pretty(&config) {
        let config_path = get_config_path();
        if let Some(parent) = config_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = fs::write(config_path, serialized);
    }
}

pub fn load_config() -> Option<PathBuf> {
    let config_path = get_config_path();
    if let Ok(contents) = fs::read_to_string(config_path) {
        if let Ok(config) = serde_json::from_str::<Config>(&contents) {
            return config.latest_project;
        }
    }
    None
}
