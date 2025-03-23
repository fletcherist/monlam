use crate::audio::{load_audio, Audio};
use cpal::traits::StreamTrait;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const BUFFER_SIZE: usize = 1024;
const SAMPLE_RATE: u32 = 44100;

#[derive(Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    pub audio_file: Option<PathBuf>,
    pub muted: bool,
    pub soloed: bool,
    pub recording: bool,
    #[serde(skip)]
    stream: Option<cpal::Stream>,
    #[serde(skip)]
    sample_index: Arc<AtomicUsize>,
    #[serde(skip)]
    audio_buffer: Arc<Mutex<Vec<f32>>>,
    pub duration: f32,
    pub current_position: f32,
    pub waveform_samples: Vec<f32>,
    pub sample_rate: u32,
    pub is_playing: bool,
    pub grid_position: f32,   // Position in the grid (in beats)
    pub grid_length: f32,     // Length in the grid (in beats)
    pub grid_start_time: f32, // When this track should start playing (in seconds)
    pub grid_end_time: f32,   // When this track should stop playing (in seconds)
}

impl Default for Track {
    fn default() -> Self {
        Self {
            name: String::new(),
            audio_file: None,
            muted: false,
            soloed: false,
            recording: false,
            stream: None,
            sample_index: Arc::new(AtomicUsize::new(0)),
            audio_buffer: Arc::new(Mutex::new(vec![0.0; 1024 * 1024])),
            duration: 0.0,
            current_position: 0.0,
            waveform_samples: Vec::new(),
            sample_rate: SAMPLE_RATE,
            is_playing: false,
            grid_position: 0.0,
            grid_length: 0.0,
            grid_start_time: 0.0,
            grid_end_time: 0.0,
        }
    }
}

impl Track {
    pub fn create_stream(&mut self, audio: &Audio) {
        if self.audio_file.is_none() {
            eprintln!("Cannot create stream: No audio file loaded");
            return;
        }

        if self.stream.is_some() {
            if let Err(e) = self.stream.as_ref().unwrap().pause() {
                eprintln!("Failed to pause existing stream: {}", e);
            }
            self.stream = None;
            self.is_playing = false;
        }

        if self.audio_buffer.lock().is_err() {
            self.audio_buffer = Arc::new(Mutex::new(vec![0.0; 1024 * 1024]));
        }

        let audio_buffer = Arc::clone(&self.audio_buffer);
        let sample_index = Arc::new(AtomicUsize::new(0));

        if let Some(stream) = audio.create_stream(audio_buffer, Arc::clone(&sample_index)) {
            self.stream = Some(stream);
            self.sample_index = sample_index;
            self.is_playing = false;
            eprintln!("Created new audio stream (paused)");
        }
    }

    pub fn seek_to(&mut self, position: f32) {
        let index = (position * self.sample_rate as f32) as usize;
        self.sample_index.store(index, Ordering::Relaxed);
        self.current_position = position;
    }

    pub fn play(&mut self) {
        if let Some(stream) = &self.stream {
            if let Err(e) = stream.play() {
                eprintln!("Failed to play stream: {}", e);
                return;
            }
            self.is_playing = true;
            eprintln!(
                "Started playing audio from position {}",
                self.current_position
            );
        } else {
            eprintln!("No audio stream available");
        }
    }

    pub fn pause(&mut self) {
        if let Some(stream) = &self.stream {
            if let Err(e) = stream.pause() {
                eprintln!("Failed to pause stream: {}", e);
                return;
            }
            self.is_playing = false;
            eprintln!("Paused audio at position {}", self.current_position);
        }
    }

    pub fn load_waveform(&mut self) {
        if let Some(path) = &self.audio_file {
            let path = path.clone();
            let (samples, sample_rate) = load_audio(&path);
            self.sample_rate = sample_rate;
            self.duration = samples.len() as f32 / sample_rate as f32;
            self.grid_length = self.duration * (120.0 / 60.0);

            let downsample_factor = samples.len() / 1000;
            self.waveform_samples = samples
                .chunks(downsample_factor)
                .map(|chunk| chunk.iter().map(|&s| s.abs()).fold(0.0, f32::max))
                .collect();

            if self.audio_buffer.lock().is_err() {
                self.audio_buffer = Arc::new(Mutex::new(vec![0.0; 1024 * 1024]));
            }

            if let Ok(mut buffer) = self.audio_buffer.lock() {
                buffer.clear();
                buffer.extend_from_slice(&samples);
                eprintln!("Loaded {} samples into audio buffer", samples.len());
            } else {
                eprintln!("Failed to lock audio buffer for writing");
            }
        }
    }

    pub fn current_position(&self) -> f32 {
        self.current_position
    }

    pub fn update_grid_times(&mut self, bpm: f32) {
        self.grid_start_time = self.grid_position * (60.0 / bpm);
        self.grid_end_time = (self.grid_position + self.grid_length) * (60.0 / bpm);
    }
}

#[derive(Serialize, Deserialize)]
pub struct DawState {
    pub timeline_position: f32,
    pub is_playing: bool,
    pub bpm: f32,
    pub tracks: Vec<Track>,
    pub grid_division: f32,
    #[serde(skip)]
    pub drag_offset: Option<f32>,
    #[serde(skip)]
    pub last_clicked_bar: f32,
}

#[derive(Serialize, Deserialize)]
struct Config {
    latest_project: Option<PathBuf>,
}

pub struct DawApp {
    pub state: DawState,
    pub last_update: std::time::Instant,
    pub seek_position: Option<f32>,
    pub audio: Audio,
}

impl DawApp {
    fn get_config_path() -> PathBuf {
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Path::new(&home).join(".monlam").join("config.json")
    }

    fn save_config(project_path: Option<PathBuf>) {
        let config = Config {
            latest_project: project_path,
        };
        if let Ok(serialized) = serde_json::to_string_pretty(&config) {
            let config_path = Self::get_config_path();
            if let Some(parent) = config_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = fs::write(config_path, serialized);
        }
    }

    fn load_config() -> Option<PathBuf> {
        let config_path = Self::get_config_path();
        if let Ok(contents) = fs::read_to_string(config_path) {
            if let Ok(config) = serde_json::from_str::<Config>(&contents) {
                return config.latest_project;
            }
        }
        None
    }

    pub fn save_project(&self) {
        if let Some(path) = FileDialog::new()
            .add_filter("DAW Project", &["json"])
            .save_file()
        {
            if let Ok(serialized) = serde_json::to_string_pretty(&self.state) {
                if fs::write(&path, serialized).is_ok() {
                    Self::save_config(Some(path));
                }
            }
        }
    }

    pub fn load_project(&mut self) {
        if let Some(path) = FileDialog::new()
            .add_filter("DAW Project", &["json"])
            .pick_file()
        {
            self.load_project_from_path(path.clone());
            Self::save_config(Some(path));
        }
    }

    fn load_project_from_path(&mut self, path: PathBuf) {
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(mut loaded_state) = serde_json::from_str::<DawState>(&contents) {
                let grid_lengths: Vec<f32> =
                    loaded_state.tracks.iter().map(|t| t.grid_length).collect();

                for (i, track) in loaded_state.tracks.iter_mut().enumerate() {
                    if let Some(path) = &track.audio_file {
                        eprintln!("Loading audio file: {}", path.display());
                        track.load_waveform();
                        track.create_stream(&self.audio);
                        track.is_playing = false;
                        track.current_position = 0.0;
                        track.grid_length = grid_lengths[i];
                    }
                }
                loaded_state.is_playing = false;
                self.state = loaded_state;
                eprintln!("Project loaded successfully");
            } else {
                eprintln!("Failed to parse project file");
            }
        } else {
            eprintln!("Failed to read project file");
        }
    }

    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let audio = Audio::new();

        let mut app = Self {
            state: DawState {
                timeline_position: 0.0,
                is_playing: false,
                bpm: 120.0,
                tracks: (1..=4)
                    .map(|i| Track {
                        name: format!("Track {}", i),
                        is_playing: false,
                        current_position: 0.0,
                        ..Default::default()
                    })
                    .collect(),
                grid_division: 0.25,
                drag_offset: None,
                last_clicked_bar: 0.0,
            },
            last_update: std::time::Instant::now(),
            seek_position: None,
            audio,
        };

        if let Some(path) = Self::load_config() {
            app.load_project_from_path(path);
        }

        for track in &mut app.state.tracks {
            track.is_playing = false;
            track.current_position = 0.0;
        }

        app
    }

    pub fn update_playback(&mut self) {
        if self.state.is_playing {
            let now = std::time::Instant::now();
            let delta = now.duration_since(self.last_update).as_secs_f32();

            // If we have a last clicked bar, start from there
            if self.state.last_clicked_bar > 0.0 {
                self.state.timeline_position = self.state.last_clicked_bar;
                eprintln!("Starting playback from bar {}", self.state.last_clicked_bar);
                // Reset all tracks to the new position
                for track in &mut self.state.tracks {
                    track.update_grid_times(self.state.bpm);
                    if self.state.timeline_position >= track.grid_start_time
                        && self.state.timeline_position < track.grid_end_time
                    {
                        let relative_position =
                            self.state.timeline_position - track.grid_start_time;
                        track.seek_to(relative_position);
                        track.play();
                    }
                }
            }

            // Update timeline position
            self.state.timeline_position += delta;

            // Update track positions and playback
            for track in &mut self.state.tracks {
                track.update_grid_times(self.state.bpm);

                if self.state.timeline_position >= track.grid_start_time
                    && self.state.timeline_position < track.grid_end_time
                {
                    if !track.is_playing {
                        let relative_position =
                            self.state.timeline_position - track.grid_start_time;
                        track.seek_to(relative_position);
                        track.play();
                    }
                    track.current_position = self.state.timeline_position - track.grid_start_time;
                } else {
                    if track.is_playing {
                        track.pause();
                    }
                    track.current_position = 0.0;
                }
            }

            self.last_update = now;
        } else {
            for track in &mut self.state.tracks {
                track.pause();
                track.current_position = 0.0;
            }
        }
    }

    pub fn on_exit(&mut self) {
        if let Some(path) = Self::load_config() {
            if let Ok(serialized) = serde_json::to_string_pretty(&self.state) {
                if let Err(e) = fs::write(&path, serialized) {
                    eprintln!("Failed to auto-save project: {}", e);
                } else {
                    eprintln!("Project auto-saved successfully");
                }
            } else {
                eprintln!("Failed to serialize project state for auto-save");
            }
        }
    }
}
