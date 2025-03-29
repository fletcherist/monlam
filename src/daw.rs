use crate::audio::{load_audio, Audio};
use crate::config::{load_waveform_data, save_waveform_data, WaveformData};
use cpal::traits::StreamTrait;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

// --- Define SelectionRect Struct HERE ---
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectionRect {
    pub start_track_idx: usize,
    pub start_beat: f32,
    pub end_track_idx: usize, // Inclusive index
    pub end_beat: f32,
}
// --- End SelectionRect Definition ---

// DAW Action enum for state management
#[derive(Debug, Clone)]
pub enum DawAction {
    SetTimelinePosition(f32),
    SetLastClickedBar(f32),
    TogglePlayback,
    SetBpm(f32),
    SetGridDivision(f32),
    RewindTimeline,
    ForwardTimeline(f32),
    SetTrackPosition(usize, f32),
    SetTrackLength(usize, f32),
    ToggleTrackMute(usize),
    ToggleTrackSolo(usize),
    ToggleTrackRecord(usize),
    AddSampleToTrack(usize, PathBuf),
    MoveSample(usize, usize, f32), // track_id, sample_id, new_position
    SetSampleLength(usize, usize, f32), // track_id, sample_id, new_length
    DeleteSample(usize, usize),    // track_id, sample_id
    SetSampleTrimPoints(usize, usize, f32, f32), // track_id, sample_id, start, end
    UpdateScrollPosition(f32, f32), // h_scroll, v_scroll
    SetSelection(Option<SelectionRect>), // Use Option<SelectionRect>
    ToggleLoopSelection,           // Toggle looping within the current selection
    RenderSelection(PathBuf),      // Path to save the rendered WAV file
}

const BUFFER_SIZE: usize = 1024;
const SAMPLE_RATE: u32 = 44100;

#[derive(Serialize, Deserialize)]
pub struct SampleWaveform {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub duration: f32,
}

// Implement Clone for SampleWaveform
impl Clone for SampleWaveform {
    fn clone(&self) -> Self {
        Self {
            samples: self.samples.clone(),
            sample_rate: self.sample_rate,
            duration: self.duration,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Sample {
    pub id: usize,
    pub name: String,
    pub audio_file: Option<PathBuf>,
    pub waveform_file: Option<PathBuf>,
    #[serde(skip)]
    stream: Option<cpal::Stream>,
    #[serde(skip)]
    sample_index: Arc<AtomicUsize>,
    #[serde(skip)]
    audio_buffer: Arc<Mutex<Vec<f32>>>,
    pub current_position: f32,
    #[serde(skip)]
    pub waveform: Option<SampleWaveform>,
    pub is_playing: bool,
    pub grid_position: f32,   // Position in the grid (in beats)
    pub grid_length: f32,     // Length in the grid (in beats)
    pub grid_start_time: f32, // When this sample should start playing (in seconds)
    pub grid_end_time: f32,   // When this sample should stop playing (in seconds)
    pub trim_start: f32,      // Start trim position in seconds (0.0 = beginning of sample)
    pub trim_end: f32,        // End trim position in seconds (0.0 = use full sample length)
    #[serde(skip)]
    total_frames: usize,
}

// Manual clone implementation for Sample to handle non-cloneable fields
impl Clone for Sample {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            audio_file: self.audio_file.clone(),
            waveform_file: self.waveform_file.clone(),
            stream: None, // Stream can't be cloned
            sample_index: Arc::new(AtomicUsize::new(0)),
            audio_buffer: Arc::new(Mutex::new(Vec::new())),
            current_position: self.current_position,
            waveform: self.waveform.clone(),
            is_playing: self.is_playing,
            grid_position: self.grid_position,
            grid_length: self.grid_length,
            grid_start_time: self.grid_start_time,
            grid_end_time: self.grid_end_time,
            trim_start: self.trim_start,
            trim_end: self.trim_end,
            total_frames: self.total_frames,
        }
    }
}

impl Default for Sample {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            audio_file: None,
            waveform_file: None,
            stream: None,
            sample_index: Arc::new(AtomicUsize::new(0)),
            audio_buffer: Arc::new(Mutex::new(Vec::new())),
            current_position: 0.0,
            waveform: None,
            is_playing: false,
            grid_position: 0.0,
            grid_length: 0.0,
            grid_start_time: 0.0,
            grid_end_time: 0.0,
            trim_start: 0.0,
            trim_end: 0.0,
            total_frames: 0,
        }
    }
}

// Implementation of Sample methods
impl Sample {
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

        // Make sure we have a valid audio buffer
        if let Ok(buffer) = self.audio_buffer.lock() {
            if buffer.is_empty() && self.audio_file.is_some() {
                drop(buffer); // Release the lock before loading audio
                if let Some(path) = &self.audio_file {
                    let (samples, sample_rate) = load_audio(path);
                    if let Ok(mut buffer) = self.audio_buffer.lock() {
                        buffer.clear();
                        buffer.extend_from_slice(&samples);
                        self.total_frames = samples.len();
                        eprintln!(
                            "Loaded {} samples into memory at {}Hz",
                            samples.len(),
                            sample_rate
                        );
                    }
                }
            }
        }

        // Get the original sample rate of the audio file
        let source_sample_rate = if let Some(waveform) = &self.waveform {
            waveform.sample_rate
        } else {
            SAMPLE_RATE
        };

        // Get the output device sample rate
        let device_sample_rate = audio.output_config.sample_rate.0;
        let rate_ratio = device_sample_rate as f32 / source_sample_rate as f32;

        eprintln!(
            "Audio source rate: {}Hz, device rate: {}Hz, ratio: {}",
            source_sample_rate, device_sample_rate, rate_ratio
        );

        let audio_buffer = Arc::clone(&self.audio_buffer);
        let sample_index = Arc::new(AtomicUsize::new(0));
        let sample_index_clone = Arc::clone(&sample_index);
        let trim_start = self.trim_start;
        let trim_end = self.trim_end;

        // Get the number of channels from the audio device
        let num_channels = audio.output_config.channels as usize;

        // Create a real-time buffer-based processing stream
        if let Some(stream) = audio.create_stream_with_callback(move |out_buffer: &mut [f32]| {
            // Get the current read position
            let mut index = sample_index_clone.load(Ordering::Relaxed);
            let buffer_lock = audio_buffer.lock();

            if let Ok(buffer) = buffer_lock {
                if !buffer.is_empty() {
                    // Calculate trim points in samples
                    let start_sample = (trim_start * source_sample_rate as f32) as usize;
                    let end_sample = if trim_end <= 0.0 {
                        buffer.len()
                    } else {
                        (trim_end * source_sample_rate as f32) as usize
                    };

                    // Fill the output buffer with samples, handling sample rate conversion if needed
                    for frame_idx in 0..(out_buffer.len() / num_channels) {
                        // Apply sample rate conversion - calculate exact position in source
                        let exact_source_pos = (index as f32) / rate_ratio;
                        let sample_position = exact_source_pos as usize;

                        // Skip if we're outside the trim boundaries
                        if sample_position < start_sample {
                            index = (start_sample as f32 * rate_ratio) as usize;
                            continue;
                        }

                        // Calculate the position in the buffer considering number of channels
                        let buffer_position = sample_position;

                        // Fill all channels
                        for channel in 0..num_channels {
                            let out_idx = frame_idx * num_channels + channel;
                            if out_idx < out_buffer.len() {
                                if buffer_position >= end_sample
                                    || buffer_position >= buffer.len() / num_channels
                                {
                                    out_buffer[out_idx] = 0.0; // Silence when past the end
                                } else {
                                    // If we have mono audio but stereo output, duplicate the sample
                                    // If we have stereo audio, use the appropriate channel
                                    let buffer_idx =
                                        if buffer.len() >= num_channels * (buffer_position + 1) {
                                            buffer_position * num_channels + channel
                                        } else {
                                            // If mono source, use the same sample for all channels
                                            buffer_position
                                        };

                                    if buffer_idx < buffer.len() {
                                        out_buffer[out_idx] = buffer[buffer_idx];
                                    } else {
                                        out_buffer[out_idx] = 0.0;
                                    }
                                }
                            }
                        }
                        index += 1;
                    }
                } else {
                    // Clear the output buffer if we have no data
                    for out_sample in out_buffer.iter_mut() {
                        *out_sample = 0.0;
                    }
                }
            } else {
                // Clear the output buffer if we can't get a lock
                for out_sample in out_buffer.iter_mut() {
                    *out_sample = 0.0;
                }
            }

            // Store the updated position
            sample_index_clone.store(index, Ordering::Relaxed);
        }) {
            self.stream = Some(stream);
            self.sample_index = sample_index;
            self.is_playing = false;
            eprintln!(
                "Created new multi-channel audio stream (paused) with device rate {}Hz",
                device_sample_rate
            );
        }
    }

    pub fn seek_to(&mut self, position: f32) {
        // Apply trim_start offset to the position
        let effective_position = self.trim_start + position;
        let sample_rate = if let Some(waveform) = &self.waveform {
            waveform.sample_rate
        } else {
            SAMPLE_RATE
        };

        let frame_position = (effective_position * sample_rate as f32) as usize;
        self.sample_index.store(frame_position, Ordering::Relaxed);
        self.current_position = position;

        eprintln!(
            "Seeked to position {}s (frame {})",
            effective_position, frame_position
        );
    }

    pub fn play(&mut self) {
        if let Some(stream) = &self.stream {
            if let Err(e) = stream.play() {
                eprintln!("Failed to play stream: {}", e);
                return;
            }
            self.is_playing = true;
            eprintln!(
                "Started playing audio from position {} (effective: {})",
                self.current_position,
                self.trim_start + self.current_position
            );
        } else {
            // Try to recreate the stream if we have an audio file but no stream
            if self.audio_file.is_some() {
                eprintln!("Recreating audio stream for {}", self.name);
                let audio = Audio::new();
                self.create_stream(&audio);
                if let Some(new_stream) = &self.stream {
                    if let Err(e) = new_stream.play() {
                        eprintln!("Failed to play recreated stream: {}", e);
                        return;
                    }
                    self.is_playing = true;
                    eprintln!(
                        "Started playing recreated audio from position {}",
                        self.current_position
                    );
                } else {
                    eprintln!("Failed to recreate audio stream for {}", self.name);
                }
            } else {
                eprintln!("No audio stream available for sample {}", self.name);
            }
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

    pub fn load_waveform(&mut self, project_path: Option<&Path>, bpm: Option<f32>) {
        if let Some(path) = &self.audio_file {
            // Load the audio data
            let (samples, sample_rate) = load_audio(path);
            let duration: f32 = samples.len() as f32 / sample_rate as f32;
            self.total_frames = samples.len();

            // Load samples into the audio buffer
            if let Ok(mut buffer) = self.audio_buffer.lock() {
                buffer.clear();
                buffer.extend_from_slice(&samples);
                eprintln!("Loaded {} samples into memory", samples.len());
            }

            // Initialize trim_end to the full duration if it's not set
            if self.trim_end == 0.0 {
                self.trim_end = duration;
            }

            // Calculate grid length based on the trimmed duration
            let beats_per_second = bpm.unwrap_or(120.0) / 60.0;
            let effective_duration = if self.trim_end <= 0.0 {
                duration - self.trim_start
            } else {
                self.trim_end - self.trim_start
            };

            // Calculate grid length based on the trimmed duration
            self.grid_length = effective_duration * beats_per_second * 0.5;

            eprintln!(
                "Sample loaded with duration: {:.2}s, effective duration: {:.2}s, grid_length (beats): {:.2}",
                duration, effective_duration, self.grid_length
            );

            // Generate downsampled waveform for display
            let downsample_factor = samples.len() / 1000;
            let waveform_samples: Vec<f32> = samples
                .chunks(downsample_factor.max(1)) // Ensure at least 1
                .map(|chunk| chunk.iter().map(|&s| s.abs()).fold(0.0, f32::max))
                .collect();

            // Save waveform data if we have a project path
            if let Some(project_path) = project_path {
                let waveform_data = WaveformData {
                    samples: waveform_samples.clone(),
                    sample_rate,
                    duration,
                };
                if let Some(waveform_path) =
                    save_waveform_data(project_path, &self.name, &waveform_data)
                {
                    self.waveform_file = Some(waveform_path.clone());
                    eprintln!("Saved waveform data to {}", waveform_path.display());
                }
            }

            self.waveform = Some(SampleWaveform {
                samples: waveform_samples,
                sample_rate,
                duration,
            });
        } else if let Some(waveform_path) = &self.waveform_file {
            // Try to load waveform data from file
            if let Some(waveform_data) = load_waveform_data(waveform_path) {
                self.waveform = Some(SampleWaveform {
                    samples: waveform_data.samples,
                    sample_rate: waveform_data.sample_rate,
                    duration: waveform_data.duration,
                });

                // Calculate the effective duration and grid length
                let beats_per_second = bpm.unwrap_or(120.0) / 60.0;
                let effective_duration = if self.trim_end <= 0.0 {
                    waveform_data.duration - self.trim_start
                } else {
                    self.trim_end - self.trim_start
                };
                self.grid_length = effective_duration * beats_per_second * 0.5;

                eprintln!("Loaded waveform data from {} with duration: {:.2}s, effective duration: {:.2}s, grid_length (beats): {:.2}", 
                         waveform_path.display(), waveform_data.duration, effective_duration, self.grid_length);
            }
        }
    }

    pub fn update_grid_times(&mut self, bpm: f32) {
        self.grid_start_time = self.grid_position * (60.0 / bpm);
        self.grid_end_time = (self.grid_position + self.grid_length) * (60.0 / bpm);
    }

    pub fn reset_playback(&mut self) {
        if let Some(stream) = &self.stream {
            if let Err(e) = stream.pause() {
                eprintln!("Failed to pause stream during reset: {}", e);
            }
        }

        self.is_playing = false;
        self.seek_to(0.0);
        self.current_position = 0.0;
    }

    // Add method to set trim points
    pub fn set_trim_points(&mut self, start: f32, end: f32) {
        if let Some(waveform) = &self.waveform {
            // Ensure trim points are within valid range
            let valid_start = start.max(0.0).min(waveform.duration);
            let valid_end = if end <= 0.0 {
                waveform.duration
            } else {
                end.max(valid_start).min(waveform.duration)
            };

            self.trim_start = valid_start;
            self.trim_end = valid_end;

            // Recalculate grid length based on trimmed duration
            let effective_duration = self.trim_end - self.trim_start;
            // Use a default BPM of 120 for calculations
            let beats_per_second = 120.0 / 60.0;
            self.grid_length = effective_duration * beats_per_second * 0.5;

            // Reset playback position
            self.current_position = 0.0;
            let index = (self.trim_start * waveform.sample_rate as f32) as usize;
            self.sample_index.store(index, Ordering::Relaxed);

            eprintln!(
                "Set trim points: start={:.2}s, end={:.2}s, effective duration={:.2}s, new grid_length={:.2}",
                self.trim_start,
                self.trim_end,
                effective_duration,
                self.grid_length
            );
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Track {
    pub id: usize,
    pub name: String,
    pub muted: bool,
    pub soloed: bool,
    pub recording: bool,
    pub samples: Vec<Sample>,
}

impl Default for Track {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            muted: false,
            soloed: false,
            recording: false,
            samples: Vec::new(),
        }
    }
}

impl Track {
    // Find overlapping samples - This is already implemented correctly
    pub fn find_overlapping_samples(&self) -> Vec<(usize, usize)> {
        let mut overlaps = Vec::new();

        for i in 0..self.samples.len() {
            for j in i + 1..self.samples.len() {
                let sample1 = &self.samples[i];
                let sample2 = &self.samples[j];

                // Check if the samples overlap
                if sample1.grid_position < sample2.grid_position + sample2.grid_length
                    && sample2.grid_position < sample1.grid_position + sample1.grid_length
                {
                    overlaps.push((sample1.id, sample2.id));
                }
            }
        }

        overlaps
    }

    // Add a sample to the track
    pub fn add_sample(&mut self, mut sample: Sample) {
        // Generate a unique ID for the sample
        let new_id = if self.samples.is_empty() {
            0
        } else {
            self.samples.iter().map(|s| s.id).max().unwrap() + 1
        };

        sample.id = new_id;
        self.samples.push(sample);
    }

    // Remove a sample from the track
    pub fn remove_sample(&mut self, sample_id: usize) -> Option<Sample> {
        if let Some(pos) = self.samples.iter().position(|s| s.id == sample_id) {
            Some(self.samples.remove(pos))
        } else {
            None
        }
    }

    // Get a sample by its ID
    pub fn get_sample(&self, sample_id: usize) -> Option<&Sample> {
        self.samples.iter().find(|s| s.id == sample_id)
    }

    // Get a mutable sample by its ID
    pub fn get_sample_mut(&mut self, sample_id: usize) -> Option<&mut Sample> {
        self.samples.iter_mut().find(|s| s.id == sample_id)
    }

    // Update grid times for all samples in the track
    pub fn update_grid_times(&mut self, bpm: f32) {
        for sample in &mut self.samples {
            sample.update_grid_times(bpm);
        }
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
    pub last_clicked_bar: f32,
    pub project_name: String,
    pub file_path: Option<PathBuf>,
    #[serde(default)]
    pub h_scroll_offset: f32,
    #[serde(default)]
    pub v_scroll_offset: f32,
    #[serde(default)]
    pub selection: Option<SelectionRect>, // Use Option<SelectionRect>
    #[serde(default)]
    pub loop_enabled: bool, // Whether looping is enabled for the current selection
}

impl Default for DawState {
    fn default() -> Self {
        Self {
            timeline_position: 0.0,
            is_playing: false,
            bpm: 120.0,
            tracks: (1..=4)
                .map(|i| Track {
                    id: i - 1,
                    name: format!("Track {}", i),
                    muted: false,
                    soloed: false,
                    recording: false,
                    samples: Vec::new(),
                })
                .collect(),
            grid_division: 0.25,
            drag_offset: None,
            last_clicked_bar: 0.0,
            project_name: String::new(),
            file_path: None,
            h_scroll_offset: 0.0,
            v_scroll_offset: 0.0,
            selection: None,
            loop_enabled: false,
        }
    }
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
            // First save waveform data for each sample in each track
            for track in &self.state.tracks {
                for sample in &track.samples {
                    if let Some(waveform) = &sample.waveform {
                        let waveform_data = WaveformData {
                            samples: waveform.samples.clone(),
                            sample_rate: waveform.sample_rate,
                            duration: waveform.duration,
                        };
                        if let Some(waveform_path) =
                            save_waveform_data(&path, &sample.name, &waveform_data)
                        {
                            eprintln!("Saved waveform data to {}", waveform_path.display());
                        }
                    }
                }
            }

            // Then save the project state (which only includes the waveform_file path, not the actual data)
            if let Ok(serialized) = serde_json::to_string_pretty(&self.state) {
                if fs::write(&path, serialized).is_ok() {
                    Self::save_config(Some(path.clone()));
                    eprintln!("Project saved successfully to {}", path.display());
                } else {
                    eprintln!("Failed to write project file to {}", path.display());
                }
            } else {
                eprintln!("Failed to serialize project state");
            }
        }
    }

    pub fn autosave_project(&self) -> bool {
        if let Some(path) = Self::load_config() {
            if let Ok(serialized) = serde_json::to_string_pretty(&self.state) {
                if let Err(e) = fs::write(&path, serialized) {
                    eprintln!("Failed to auto-save project: {}", e);
                    return false;
                } else {
                    eprintln!("Project auto-saved successfully to {}", path.display());
                    return true;
                }
            } else {
                eprintln!("Failed to serialize project state for auto-save");
                return false;
            }
        }
        false
    }

    pub fn load_project(&mut self) {
        if let Some(path) = FileDialog::new()
            .add_filter("DAW Project", &["json"])
            .pick_file()
        {
            if self.load_project_from_path(path.clone()) {
                Self::save_config(Some(path));
            }
        }
    }

    fn load_project_from_path(&mut self, path: PathBuf) -> bool {
        eprintln!("Attempting to load project from {}", path.display());
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(mut loaded_state) = serde_json::from_str::<DawState>(&contents) {
                // Process each track and its samples
                for track in &mut loaded_state.tracks {
                    for sample in &mut track.samples {
                        if sample.audio_file.is_some() {
                            eprintln!(
                                "Loading audio file for sample {} in track {}",
                                sample.name, track.name
                            );
                            sample.load_waveform(Some(&path), Some(loaded_state.bpm));

                            // Create audio stream for the sample
                            sample.create_stream(&self.audio);

                            // Verify stream was created successfully
                            if sample.stream.is_none() {
                                eprintln!("Warning: Failed to create audio stream on project load");
                                // Try one more time with a new Audio instance
                                let audio = Audio::new();
                                sample.create_stream(&audio);
                            }

                            sample.is_playing = false;
                            sample.current_position = 0.0;
                        }
                    }
                }

                loaded_state.is_playing = false;
                self.state = loaded_state;
                eprintln!("Project loaded successfully");
                return true;
            } else {
                eprintln!("Failed to parse project file: {}", path.display());
            }
        } else {
            eprintln!("Failed to read project file: {}", path.display());
        }
        false
    }

    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Create a new audio engine
        let audio = Audio::new();

        // Initialize with default state
        let mut app = Self {
            state: DawState::default(),
            last_update: std::time::Instant::now(),
            seek_position: None,
            audio,
        };

        // Try to load last project if exists
        if let Some(path) = Self::load_config() {
            if path.exists() {
                eprintln!("Loading last project from {}", path.display());
                app.load_project_from_path(path);
            } else {
                eprintln!("Last project path not found: {}", path.display());
            }
        }

        // Ensure tracks are in the right state
        for track in &mut app.state.tracks {
            // Remove these lines that reference fields that no longer exist on Track
            // track.is_playing = false;
            // track.current_position = 0.0;
            // Instead, reset all samples in the track
            for sample in &mut track.samples {
                sample.is_playing = false;
                sample.current_position = 0.0;
            }
        }

        app
    }

    // Process a DAW action and update the state accordingly
    pub fn dispatch(&mut self, action: DawAction) {
        match action {
            DawAction::SetTimelinePosition(position) => {
                self.state.timeline_position = position;
                for track in &mut self.state.tracks {
                    for sample in &mut track.samples {
                        sample.update_grid_times(self.state.bpm);
                        if position >= sample.grid_start_time && position < sample.grid_end_time {
                            let relative_position = position - sample.grid_start_time;
                            sample.seek_to(relative_position);
                        }
                    }
                }
            }
            DawAction::SetLastClickedBar(position) => {
                self.state.last_clicked_bar = position;
                // Also update timeline position immediately
                self.state.timeline_position = position;
                for track in &mut self.state.tracks {
                    for sample in &mut track.samples {
                        sample.update_grid_times(self.state.bpm);
                        if position >= sample.grid_start_time && position < sample.grid_end_time {
                            let relative_position = position - sample.grid_start_time;
                            sample.seek_to(relative_position);
                        }
                    }
                }
            }
            DawAction::TogglePlayback => {
                let was_playing = self.state.is_playing;
                self.state.is_playing = !was_playing;
                self.last_update = Instant::now();

                // If starting playback, make sure all samples are in a clean state
                if !was_playing {
                    // Reset sample playback states
                    for track in &mut self.state.tracks {
                        for sample in &mut track.samples {
                            sample.reset_playback();
                        }
                    }

                    // If we have a clicked position, prepare to seek there
                    if self.state.last_clicked_bar > 0.0 {
                        self.state.timeline_position = self.state.last_clicked_bar;
                    }

                    // Make sure all tracks have updated timings
                    self.update_track_timings();
                }

                self.update_playback();
            }
            DawAction::SetBpm(bpm) => {
                // Convert current timeline position from time to beats at old BPM
                let current_position_in_beats =
                    self.state.timeline_position * (self.state.bpm / 60.0);

                // Update the BPM
                self.state.bpm = bpm;

                // Update track and sample timings
                for track in &mut self.state.tracks {
                    track.update_grid_times(bpm);
                }

                // Convert back to time at new BPM, maintaining the same beat position
                self.state.timeline_position = current_position_in_beats * (60.0 / bpm);
            }
            DawAction::SetGridDivision(division) => {
                self.state.grid_division = division;
            }
            DawAction::RewindTimeline => {
                self.state.timeline_position = 0.0;
                for track in &mut self.state.tracks {
                    for sample in &mut track.samples {
                        sample.is_playing = false;
                        sample.current_position = 0.0;
                        sample.seek_to(0.0);
                    }
                }
            }
            DawAction::ForwardTimeline(bars) => {
                self.state.timeline_position += bars;
                for track in &mut self.state.tracks {
                    for sample in &mut track.samples {
                        sample.is_playing = false;
                        sample.current_position = self.state.timeline_position;
                        sample.seek_to(self.state.timeline_position);
                    }
                }
            }
            DawAction::SetTrackPosition(track_id, new_position) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    // In the new structure, we need to move all samples in the track
                    for sample in &mut track.samples {
                        sample.grid_position =
                            new_position + (sample.grid_position - sample.grid_position); // Maintain relative positions
                        sample.update_grid_times(self.state.bpm);
                    }
                }
            }
            DawAction::SetTrackLength(track_id, new_length) => {
                // This doesn't make sense for tracks anymore as they don't have a fixed length
                // Instead we'll interpret this as scaling all samples in the track
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    for sample in &mut track.samples {
                        sample.grid_length = new_length; // This is simplified; in reality you'd want a more complex scaling approach
                        sample.update_grid_times(self.state.bpm);
                    }
                }
            }
            DawAction::ToggleTrackMute(track_id) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    track.muted = !track.muted;
                }
            }
            DawAction::ToggleTrackSolo(track_id) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    track.soloed = !track.soloed;
                }
            }
            DawAction::ToggleTrackRecord(track_id) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    track.recording = !track.recording;
                }
            }
            DawAction::AddSampleToTrack(track_id, path) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    let mut sample = Sample::default();
                    sample.audio_file = Some(path.clone());
                    sample.name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    sample.load_waveform(None, Some(self.state.bpm));
                    sample.update_grid_times(self.state.bpm);
                    sample.create_stream(&self.audio);

                    // Add the sample to the track
                    track.add_sample(sample);
                    let new_sample_id = track.samples.last().map(|s| s.id).unwrap_or(0);

                    // Now check for and adjust any overlapping samples
                    if let Some(new_sample) = track.get_sample(new_sample_id) {
                        let current_sample_start = new_sample.grid_position;
                        let current_sample_end = new_sample.grid_position + new_sample.grid_length;

                        // Find overlapping samples
                        let overlapping_samples: Vec<usize> = track
                            .samples
                            .iter()
                            .filter(|s| s.id != new_sample_id) // Skip the newly added sample
                            .filter(|s| {
                                let other_start = s.grid_position;
                                let other_end = s.grid_position + s.grid_length;

                                // Check if the samples overlap
                                current_sample_start < other_end && current_sample_end > other_start
                            })
                            .map(|s| s.id)
                            .collect();

                        // Adjust the length of overlapping samples
                        for overlap_id in overlapping_samples {
                            if let Some(other_sample) = track.get_sample_mut(overlap_id) {
                                // If this is a sample that starts before our new sample
                                if other_sample.grid_position < current_sample_start {
                                    // Adjust its length to end exactly at the start of our new sample
                                    let new_length =
                                        current_sample_start - other_sample.grid_position;
                                    eprintln!("Adjusting sample {} length from {} to {} due to overlap with new sample {}", 
                                              other_sample.id, other_sample.grid_length, new_length, new_sample_id);
                                    other_sample.grid_length = new_length;
                                    other_sample.update_grid_times(self.state.bpm);
                                }
                            }
                        }
                    }

                    eprintln!("Added sample to track {}: {}", track.name, path.display());
                }
            }
            DawAction::MoveSample(track_id, sample_id, new_position) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(sample) = track.get_sample_mut(sample_id) {
                        sample.grid_position = new_position;
                        sample.update_grid_times(self.state.bpm);

                        // Adjust overlapping samples
                        // First, find all samples that this sample would overlap with
                        let current_sample_start = sample.grid_position;
                        let current_sample_end = sample.grid_position + sample.grid_length;

                        // Collect samples that overlap with the current sample
                        let overlapping_samples: Vec<usize> = track
                            .samples
                            .iter()
                            .filter(|s| s.id != sample_id) // Skip the current sample
                            .filter(|s| {
                                let other_start = s.grid_position;
                                let other_end = s.grid_position + s.grid_length;

                                // Check if the samples overlap
                                current_sample_start < other_end && current_sample_end > other_start
                            })
                            .map(|s| s.id)
                            .collect();

                        // Adjust the length of overlapping samples
                        for overlap_id in overlapping_samples {
                            if let Some(other_sample) = track.get_sample_mut(overlap_id) {
                                // If this is a sample that starts before our current sample
                                if other_sample.grid_position < current_sample_start {
                                    // Adjust its length to end exactly at the start of our current sample
                                    let new_length =
                                        current_sample_start - other_sample.grid_position;
                                    eprintln!("Adjusting sample {} length from {} to {} due to overlap with sample {}", 
                                              other_sample.id, other_sample.grid_length, new_length, sample_id);
                                    other_sample.grid_length = new_length;
                                    other_sample.update_grid_times(self.state.bpm);
                                }
                            }
                        }
                    }
                }
            }
            DawAction::SetSampleLength(track_id, sample_id, new_length) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(sample) = track.get_sample_mut(sample_id) {
                        sample.grid_length = new_length;
                        sample.update_grid_times(self.state.bpm);

                        // Handle any potential overlaps after changing length
                        let current_sample_start = sample.grid_position;
                        let current_sample_end = sample.grid_position + sample.grid_length;

                        // Find samples that might now overlap with this one
                        let overlapping_samples: Vec<usize> = track
                            .samples
                            .iter()
                            .filter(|s| s.id != sample_id) // Skip the current sample
                            .filter(|s| {
                                let other_start = s.grid_position;
                                let other_end = s.grid_position + s.grid_length;

                                // Check if the samples overlap
                                current_sample_start < other_end && current_sample_end > other_start
                            })
                            .map(|s| s.id)
                            .collect();

                        // Adjust the length of overlapping samples
                        for overlap_id in overlapping_samples {
                            if let Some(other_sample) = track.get_sample_mut(overlap_id) {
                                // If this is a sample that starts before our current sample
                                if other_sample.grid_position < current_sample_start {
                                    // Adjust its length to end exactly at the start of our current sample
                                    let new_length =
                                        current_sample_start - other_sample.grid_position;
                                    eprintln!("Adjusting sample {} length from {} to {} due to overlap with sample {}", 
                                            other_sample.id, other_sample.grid_length, new_length, sample_id);
                                    other_sample.grid_length = new_length;
                                    other_sample.update_grid_times(self.state.bpm);
                                }
                            }
                        }
                    }
                }
            }
            DawAction::DeleteSample(track_id, sample_id) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(sample) = track.remove_sample(sample_id) {
                        eprintln!("Removed sample {} from track {}", sample.name, track.name);
                    }
                }
            }
            DawAction::SetSampleTrimPoints(track_id, sample_id, trim_start, trim_end) => {
                if let Some(track) = self
                    .state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == track_id)
                {
                    if let Some(sample) = track
                        .samples
                        .iter_mut()
                        .find(|sample| sample.id == sample_id)
                    {
                        sample.trim_start = trim_start;
                        sample.trim_end = trim_end;
                    }
                }
            }
            DawAction::UpdateScrollPosition(h_scroll, v_scroll) => {
                self.state.h_scroll_offset = h_scroll;
                self.state.v_scroll_offset = v_scroll;
            }
            DawAction::SetSelection(selection) => {
                if self.state.selection != selection {
                    self.state.selection = selection;
                } else {
                    // State already matched
                }
            }
            DawAction::ToggleLoopSelection => {
                self.state.loop_enabled = !self.state.loop_enabled;
            }
            DawAction::RenderSelection(path) => {
                if let Some(selection) = &self.state.selection {
                    self.render_selection(&path, selection);
                } else {
                    eprintln!("Cannot render: No selection active");
                }
            }
        }
    }

    // Snap a position to the grid
    pub fn snap_to_grid(&self, position: f32) -> f32 {
        // Calculate the nearest grid line
        let division = self.state.grid_division;
        let lower_grid_line = (position / division).floor() * division;
        let upper_grid_line = (position / division).ceil() * division;

        // Find which grid line is closer
        if (position - lower_grid_line) < (upper_grid_line - position) {
            lower_grid_line
        } else {
            upper_grid_line
        }
    }

    pub fn update_playback(&mut self) {
        if self.state.is_playing {
            let now = std::time::Instant::now();
            let delta = now.duration_since(self.last_update).as_secs_f32();
            self.last_update = now; // Update timestamp immediately to prevent accumulation errors

            // Only use last_clicked_bar on the first frame of playback
            if self.state.last_clicked_bar > 0.0 && self.seek_position.is_none() {
                eprintln!("Starting playback from bar {}", self.state.last_clicked_bar);
                self.seek_position = Some(self.state.last_clicked_bar);
                self.state.last_clicked_bar = 0.0; // Reset to avoid restarting continuously
                return; // Skip this frame, we'll handle the seek on the next update
            }

            // Update timeline position
            self.state.timeline_position += delta;

            // Handle looping within selection if enabled
            if self.state.loop_enabled && self.state.selection.is_some() {
                let selection = self.state.selection.as_ref().unwrap();
                // Convert selection beats to time
                let loop_start = selection.start_beat * (60.0 / self.state.bpm);
                let loop_end = selection.end_beat * (60.0 / self.state.bpm);

                // If we've passed the end of the selection, loop back to the start
                if self.state.timeline_position >= loop_end {
                    eprintln!("Looping back to selection start");
                    self.state.timeline_position = loop_start;

                    // Reset playback state for all samples
                    for track in &mut self.state.tracks {
                        for sample in &mut track.samples {
                            sample.reset_playback();
                        }
                    }
                }
            }

            let timeline_pos = self.state.timeline_position;
            let mut any_sample_playing = false;

            let any_track_soloed = self.state.tracks.iter().any(|t| t.soloed);

            for track in &mut self.state.tracks {
                if track.muted || (any_track_soloed && !track.soloed) {
                    continue;
                }

                for sample in &mut track.samples {
                    sample.update_grid_times(self.state.bpm);
                    let should_play = timeline_pos >= sample.grid_start_time
                        && timeline_pos < sample.grid_end_time;

                    if should_play {
                        if !sample.is_playing {
                            let relative_position = timeline_pos - sample.grid_start_time;
                            sample.seek_to(relative_position);
                            sample.play();
                        }
                        let relative_position = timeline_pos - sample.grid_start_time;
                        sample.current_position = relative_position;

                        if let Some(_waveform) = &sample.waveform {
                            let effective_position = sample.trim_start + relative_position;
                            if sample.trim_end > 0.0 && effective_position >= sample.trim_end {
                                if sample.is_playing {
                                    sample.pause();
                                }
                            } else {
                                any_sample_playing = true;
                            }
                        } else {
                            any_sample_playing = true;
                        }
                    } else {
                        if sample.is_playing {
                            sample.pause();
                        }
                    }
                }
            }

            // Check if we've reached the end of all samples
            if !any_sample_playing && !self.state.loop_enabled {
                let all_samples_past = self.state.tracks.iter().all(|track| {
                    track
                        .samples
                        .iter()
                        .all(|sample| self.state.timeline_position >= sample.grid_end_time)
                });

                if all_samples_past && !self.state.tracks.iter().all(|t| t.samples.is_empty()) {
                    // We've reached the end of all samples, restart from beginning
                    eprintln!("Reached end of all samples, rewinding");
                    self.dispatch(DawAction::RewindTimeline);
                }
            }
        } else {
            // If not playing, make sure all samples are paused
            for track in &mut self.state.tracks {
                for sample in &mut track.samples {
                    if sample.is_playing {
                        sample.pause();
                    }
                }
            }
        }
    }

    pub fn on_exit(&mut self) {
        eprintln!("Application exiting, auto-saving project...");
        self.autosave_project();
    }

    // Update track and sample grid_start_time and grid_end_time when BPM changes
    pub fn update_track_timings(&mut self) {
        let bpm = self.state.bpm;
        for track in &mut self.state.tracks {
            track.update_grid_times(bpm);
        }
    }

    #[cfg(test)]
    pub fn new_test() -> Self {
        let bpm = 120.0;
        let mut app = DawApp {
            state: DawState {
                tracks: Vec::new(),
                timeline_position: 0.0,
                is_playing: false,
                bpm,
                grid_division: 0.25,
                drag_offset: None,
                last_clicked_bar: 0.0,
                project_name: "Test Project".to_string(),
                file_path: None,
                h_scroll_offset: 0.0,
                v_scroll_offset: 0.0,
                selection: None,
                loop_enabled: false,
            },
            audio: Audio::new(),
            last_update: std::time::Instant::now(),
            seek_position: None,
        };

        // Create a default test track
        let mut track = Track::default();
        track.id = 0;
        track.name = "Test Track".to_string();

        // Add a sample to the track
        let mut sample = Sample::default();
        sample.id = 0;
        sample.name = "Test Sample".to_string();
        sample.grid_position = 0.0; // position in beats
        sample.grid_length = 4.0; // length in beats
        sample.update_grid_times(bpm);

        track.samples.push(sample);
        app.state.tracks.push(track);

        app
    }

    // Helper method to convert beats to time in seconds based on BPM
    pub fn beat_to_time(&self, beats: f32) -> f32 {
        beats * (60.0 / self.state.bpm)
    }

    // Helper method to convert time in seconds to beats based on BPM
    pub fn time_to_beat(&self, time: f32) -> f32 {
        time * (self.state.bpm / 60.0)
    }

    pub fn render_selection(&self, output_path: &Path, selection: &SelectionRect) -> bool {
        eprintln!("Starting render to {}", output_path.display());

        // Calculate time range from the selection (in seconds)
        let start_time = selection.start_beat * (60.0 / self.state.bpm);
        let end_time = selection.end_beat * (60.0 / self.state.bpm);
        let duration = end_time - start_time;

        if duration <= 0.0 {
            eprintln!("Cannot render: Invalid selection duration");
            return false;
        }

        // Target sample rate and channels for rendering
        let target_sample_rate = 44100;
        let num_channels = 2; // Stereo output

        // Size of each processing buffer (similar to real-time audio processing)
        const BUFFER_SIZE: usize = 1024;

        // Calculate total number of frames and buffers
        let total_frames = (duration * target_sample_rate as f32) as usize;
        let num_buffers = (total_frames + BUFFER_SIZE - 1) / BUFFER_SIZE;

        // Create the WAV file with appropriate specs
        use hound::{WavSpec, WavWriter};
        let spec = WavSpec {
            channels: num_channels as u16,
            sample_rate: target_sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = match WavWriter::create(output_path, spec) {
            Ok(writer) => writer,
            Err(e) => {
                eprintln!("Failed to create WAV file: {}", e);
                return false;
            }
        };

        // Get track range for the selection
        let track_start = selection.start_track_idx;
        let track_end = selection.end_track_idx;

        // Check if we have valid track indices
        if track_end >= self.state.tracks.len() || track_start > track_end {
            eprintln!("Cannot render: Invalid track selection");
            return false;
        }

        // Check if any track is soloed
        let any_track_soloed = self.state.tracks.iter().any(|t| t.soloed);

        // Process the audio in buffer-sized chunks to simulate real-time playback
        for buffer_idx in 0..num_buffers {
            // Create a buffer for this chunk
            let mut mix_buffer = vec![0.0; BUFFER_SIZE * num_channels];

            // Calculate current time position
            let current_time =
                start_time + (buffer_idx * BUFFER_SIZE) as f32 / target_sample_rate as f32;
            let buffer_duration = BUFFER_SIZE as f32 / target_sample_rate as f32;

            // Process each track
            for track_idx in track_start..=track_end {
                if track_idx >= self.state.tracks.len() {
                    continue;
                }

                let track = &self.state.tracks[track_idx];

                // Skip muted tracks or non-soloed tracks when soloing is active
                if track.muted || (any_track_soloed && !track.soloed) {
                    continue;
                }

                // Process each sample in the track
                for sample in &track.samples {
                    // Check if this sample is active during this time slice
                    if current_time + buffer_duration <= sample.grid_start_time
                        || current_time >= sample.grid_end_time
                    {
                        continue; // Sample not active in this time slice
                    }

                    // Calculate relative position within the sample
                    let sample_offset = if current_time > sample.grid_start_time {
                        current_time - sample.grid_start_time
                    } else {
                        0.0
                    };

                    // Apply trim offset
                    let trimmed_offset = sample_offset + sample.trim_start;

                    // Don't process if we're past the trim end
                    if sample.trim_end > 0.0 && trimmed_offset >= sample.trim_end {
                        continue;
                    }

                    // Get sample audio data
                    let sample_buffer = if let Ok(buffer) = sample.audio_buffer.lock() {
                        if buffer.is_empty() {
                            continue;
                        }
                        buffer.clone()
                    } else {
                        continue; // Skip if we can't lock the buffer
                    };

                    // Get source sample rate for resampling
                    let source_sample_rate = if let Some(waveform) = &sample.waveform {
                        waveform.sample_rate
                    } else {
                        target_sample_rate
                    };

                    // Convert time offset to sample position
                    let start_frame = (trimmed_offset * source_sample_rate as f32) as usize;

                    // Calculate resampling ratio
                    let rate_ratio = target_sample_rate as f32 / source_sample_rate as f32;

                    // Process each frame in the current buffer
                    for dest_frame in 0..BUFFER_SIZE {
                        // Calculate the exact source position with resampling
                        let source_frame_f32 =
                            start_frame as f32 + (dest_frame as f32 / rate_ratio);
                        let source_frame = source_frame_f32 as usize;

                        // Skip if we're past the end of the sample or trim point
                        let trim_end_frame = if sample.trim_end <= 0.0 {
                            usize::MAX
                        } else {
                            (sample.trim_end * source_sample_rate as f32) as usize
                        };

                        if source_frame >= trim_end_frame
                            || source_frame >= sample_buffer.len() / num_channels.max(1)
                        {
                            continue;
                        }

                        // Mix this sample frame into our buffer
                        for channel in 0..num_channels {
                            let dest_idx = dest_frame * num_channels + channel;

                            if dest_idx < mix_buffer.len() {
                                let sample_value =
                                    if sample_buffer.len() >= num_channels * (source_frame + 1) {
                                        // Stereo sample
                                        sample_buffer[source_frame * num_channels + channel]
                                    } else if !sample_buffer.is_empty() {
                                        // Mono sample - duplicate to both channels
                                        sample_buffer[source_frame.min(sample_buffer.len() - 1)]
                                    } else {
                                        0.0
                                    };

                                // Add sample to mix buffer
                                mix_buffer[dest_idx] += sample_value;
                            }
                        }
                    }
                }
            }

            // Determine the actual buffer size (last buffer might be smaller)
            let frames_left = total_frames - (buffer_idx * BUFFER_SIZE);
            let actual_buffer_size = BUFFER_SIZE.min(frames_left);

            // Normalize just this buffer to prevent clipping
            self.normalize_audio_buffer(&mut mix_buffer[0..actual_buffer_size * num_channels]);

            // Write buffer to WAV file
            for i in 0..(actual_buffer_size * num_channels) {
                // Convert f32 [-1.0, 1.0] to i16 range
                let amplitude = (mix_buffer[i] * 32767.0) as i16;
                if let Err(e) = writer.write_sample(amplitude) {
                    eprintln!("Error writing sample: {}", e);
                    return false;
                }
            }
        }

        // Finalize the WAV file
        if let Err(e) = writer.finalize() {
            eprintln!("Error finalizing WAV writer: {}", e);
            return false;
        }

        eprintln!(
            "Successfully rendered selection to {}",
            output_path.display()
        );
        true
    }

    // Helper method to normalize audio to prevent clipping
    fn normalize_audio_buffer(&self, buffer: &mut [f32]) {
        // Find the maximum absolute amplitude
        let max_amplitude = buffer
            .iter()
            .fold(0.0f32, |max, &sample| max.max(sample.abs()));

        // Only normalize if we risk clipping
        if max_amplitude > 1.0 {
            let gain = 1.0 / max_amplitude;
            for sample in buffer.iter_mut() {
                *sample *= gain;
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Config {
    latest_project: Option<PathBuf>,
}
