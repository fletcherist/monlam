use crate::audio::{load_audio, Audio};
use crate::group::Group;
use crate::config::{load_waveform_data, save_waveform_data, WaveformData};
use cpal::traits::StreamTrait;
use rfd::FileDialog;
use rfd::MessageDialog;
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
    SetClickedPosition(f32), // New action to update only the clicked position
    TogglePlayback,
    SetBpm(f32),
    SetGridDivision(f32),
    RewindTimeline,
    ForwardTimeline(f32),
    ToggleTrackMute(usize),
    ToggleTrackSolo(usize),
    ToggleTrackRecord(usize),
    AddSampleToTrack(usize, PathBuf),
    MoveSample(usize, usize, f32), // track_id, sample_id, new_position
    MoveSampleBetweenTracks(usize, usize, usize, f32), // source_track_id, sample_id, target_track_id, new_position
    SetSampleLength(usize, usize, f32),                // track_id, sample_id, new_length
    DeleteSample(usize, usize),                        // track_id, sample_id
    SetSampleTrimPoints(usize, usize, f32, f32),       // track_id, sample_id, start, end
    UpdateScrollPosition(f32, f32),                    // h_scroll, v_scroll
    SetSelection(Option<SelectionRect>),               // Use Option<SelectionRect>
    ToggleLoopSelection,       // Toggle looping within the current selection
    RenderSelection(PathBuf),  // Path to save the rendered WAV file
    SetZoomLevel(f32),         // Set the zoom level for the grid
    SetLoopRangeFromSelection, // Set loop range from current selection without toggling loop state
    CreateGroup(String),       // Create a new Group with the given name
    RenameGroup(String, String), // Rename a Group (old_name, new_name)
    DeleteGroup(String),       // Delete a Group by name
    AddGroupToTrack(usize, String), // Add a Group to a track (track_id, group_name)
    RenderGroupFromSelection(String), // Render the current selection to a Group
    OpenGroupInNewTab(String), // Open a Group in a new tab (group_name)
    SwitchToTab(usize),        // Switch to a different tab by ID
    CloseTab(usize),           // Close a tab by ID
    SaveGroup(String),         // Save current Group state and update render.wav
    CreateTrack,
}

const SAMPLE_RATE: u32 = 44100;

#[derive(Serialize, Deserialize, Clone)]
pub struct SampleWaveform {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub duration: f32,
}

/// Type of item in a track - either a sample or a group
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackItemType {
    Sample,
    Group,
}

impl Default for TrackItemType {
    fn default() -> Self {
        TrackItemType::Sample
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
    #[serde(default)]
    pub item_type: TrackItemType, // Type of track item (Sample or AudioBox)
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
            item_type: self.item_type,
        }
    }
}

impl Default for Sample {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Sample".to_string(),
            audio_file: None,
            waveform_file: None,
            stream: None,
            sample_index: Arc::new(AtomicUsize::new(0)),
            audio_buffer: Arc::new(Mutex::new(vec![])),
            current_position: 0.0,
            waveform: None,
            is_playing: false,
            grid_position: 0.0,
            grid_length: 4.0,
            grid_start_time: 0.0,
            grid_end_time: 0.0,
            trim_start: 0.0,
            trim_end: 0.0,
            total_frames: 0,
            item_type: TrackItemType::Sample,
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
                    match load_audio(path) {
                        Ok((samples, sample_rate)) => {
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
                        Err(err) => {
                            // Just log the error but don't show a dialog since that's handled elsewhere
                            eprintln!("Failed to load audio in create_stream: {}", err);
                            return; // Exit early if we can't load the audio
                        }
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
            match load_audio(path) {
                Ok((samples, sample_rate)) => {
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
                }
                Err(err) => {
                    // Just log the error but don't show a dialog since that's handled elsewhere
                    eprintln!("Failed to load audio in load_waveform: {}", err);
                }
            }
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
}

#[derive(Serialize, Deserialize, Clone)]
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
    // Create a new track with the given ID and name
    pub fn new(id: usize, name: String) -> Self {
        Self {
            id,
            name,
            muted: false,
            soloed: false,
            recording: false,
            samples: Vec::new(),
        }
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

/// Represents a tab in the DAW UI
#[derive(Serialize, Deserialize, Clone)]
pub struct Tab {
    pub id: usize,
    pub name: String,
    pub is_group: bool,
    pub group_name: Option<String>,
}

impl Default for Tab {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Main".to_string(),
            is_group: false,
            group_name: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DawState {
    pub timeline_position: f32,
    pub is_playing: bool,
    pub bpm: f32,
    pub tracks: Vec<Track>,
    pub grid_division: f32,
    #[serde(skip)]
    pub last_clicked_bar: f32,
    #[serde(skip)]
    pub last_clicked_position: f32, // Store the position of the last clicked track marker
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
    #[serde(default = "default_zoom_level")]
    pub zoom_level: f32, // Zoom level for the grid view
    #[serde(default = "default_loop_range")]
    pub loop_range: Option<(f32, f32)>, // Loop start and end times in seconds (None if no range set)
    #[serde(default = "default_tabs")]
    pub tabs: Vec<Tab>, // List of open tabs
    #[serde(default)]
    pub active_tab_id: usize, // Currently active tab ID
    #[serde(default)]
    pub audio_boxes: Vec<String>, // List of AudioBox names in this project
    pub next_track_id: usize,
    pub modified: bool,
}

fn default_zoom_level() -> f32 {
    1.0 // Default zoom level is 1.0 (100%)
}

fn default_loop_range() -> Option<(f32, f32)> {
    None // Default is no loop range
}

fn default_tabs() -> Vec<Tab> {
    vec![Tab::default()]
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
            last_clicked_bar: 0.0,
            last_clicked_position: 0.0, // Store the position of the last clicked track marker
            project_name: String::new(),
            file_path: None,
            h_scroll_offset: 0.0,
            v_scroll_offset: 0.0,
            selection: None,
            loop_enabled: false,
            zoom_level: 1.0,
            loop_range: None,
            tabs: default_tabs(),
            active_tab_id: 0,
            audio_boxes: Vec::new(),
            next_track_id: 5,
            modified: false,
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

    pub fn save_project(&mut self) {
        // Check if we already have a project path (existing project)
        if let Some(existing_path) = &self.state.file_path.clone() {  // Clone here to avoid borrowing issues
            eprintln!("Saving to existing project path: {}", existing_path.display());
            let project_folder = existing_path.parent().unwrap_or(Path::new("")).to_path_buf();
            
            // Create waveforms directory
            let waveforms_dir = project_folder.join("waveforms");
            if !waveforms_dir.exists() {
                if let Err(e) = std::fs::create_dir_all(&waveforms_dir) {
                    eprintln!("Failed to create waveforms directory: {}", e);
                }
            }
            
            // Save waveform data for each sample in each track
            for track in &self.state.tracks {
                for sample in &track.samples {
                    if let Some(waveform) = &sample.waveform {
                        let waveform_data = WaveformData {
                            samples: waveform.samples.clone(),
                            sample_rate: waveform.sample_rate,
                            duration: waveform.duration,
                        };
                        if let Some(waveform_path) =
                            save_waveform_data(existing_path, &sample.name, &waveform_data)
                        {
                            eprintln!("Saved waveform data to {}", waveform_path.display());
                        }
                    }
                }
            }

            // Save the project state to the existing file
            if let Ok(serialized) = serde_json::to_string_pretty(&self.state) {
                if fs::write(existing_path, serialized).is_ok() {
                    eprintln!("Project saved successfully to {}", existing_path.display());
                } else {
                    eprintln!("Failed to write project file to {}", existing_path.display());
                }
            } else {
                eprintln!("Failed to serialize project state");
            }
            return;
        }

        // For a new project, prompt for folder location
        if let Some(folder_path) = FileDialog::new()
            .set_title("Save Project")
            .pick_folder()
        {
            // Create a folder with the project name
            let project_name = if self.state.project_name.trim().is_empty() {
                "Untitled Project"
            } else {
                &self.state.project_name
            };
            
            let project_folder = folder_path.join(project_name);
            
            // Create the project folder if it doesn't exist
            if !project_folder.exists() {
                if let Err(e) = std::fs::create_dir_all(&project_folder) {
                    eprintln!("Failed to create project folder: {}", e);
                    return;
                }
            }
            
            // Create waveforms directory
            let waveforms_dir = project_folder.join("waveforms");
            if !waveforms_dir.exists() {
                if let Err(e) = std::fs::create_dir_all(&waveforms_dir) {
                    eprintln!("Failed to create waveforms directory: {}", e);
                }
            }
            
            // Save the project file inside the folder
            let project_file_path = project_folder.join("project.json");
            
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
                            save_waveform_data(&project_file_path, &sample.name, &waveform_data)
                        {
                            eprintln!("Saved waveform data to {}", waveform_path.display());
                        }
                    }
                }
            }

            // Then save the project state to project.json file in the project folder
            if let Ok(serialized) = serde_json::to_string_pretty(&self.state) {
                if fs::write(&project_file_path, serialized).is_ok() {
                    // Update the file_path in the state
                    self.state.file_path = Some(project_file_path.clone());
                    
                    // Save updated state with new file path
                    if let Ok(updated_serialized) = serde_json::to_string_pretty(&self.state) {
                        let _ = fs::write(&project_file_path, updated_serialized);
                    }
                    
                    Self::save_config(Some(project_file_path.clone()));
                    eprintln!("Project saved successfully to {}", project_file_path.display());
                } else {
                    eprintln!("Failed to write project file to {}", project_file_path.display());
                }
            } else {
                eprintln!("Failed to serialize project state");
            }
        }
    }

    pub fn autosave_project(&self) -> bool {
        if let Some(project_file_path) = Self::load_config() {
            // Make sure the parent directory exists
            if let Some(project_folder) = project_file_path.parent() {
                if !project_folder.exists() {
                    if let Err(e) = std::fs::create_dir_all(project_folder) {
                        eprintln!("Failed to create project folder for auto-save: {}", e);
                        return false;
                    }
                }
                
                // Create waveforms directory
                let waveforms_dir = project_folder.join("waveforms");
                if !waveforms_dir.exists() {
                    if let Err(e) = std::fs::create_dir_all(&waveforms_dir) {
                        eprintln!("Failed to create waveforms directory for auto-save: {}", e);
                    }
                }
                
                // Save waveform data for each sample in each track
                for track in &self.state.tracks {
                    for sample in &track.samples {
                        if let Some(waveform) = &sample.waveform {
                            let waveform_data = WaveformData {
                                samples: waveform.samples.clone(),
                                sample_rate: waveform.sample_rate,
                                duration: waveform.duration,
                            };
                            if let Some(waveform_path) = 
                                save_waveform_data(&project_file_path, &sample.name, &waveform_data) 
                            {
                                eprintln!("Auto-saved waveform data to {}", waveform_path.display());
                            }
                        }
                    }
                }
                
                if let Ok(serialized) = serde_json::to_string_pretty(&self.state) {
                    if let Err(e) = fs::write(&project_file_path, serialized) {
                        eprintln!("Failed to auto-save project: {}", e);
                        return false;
                    } else {
                        eprintln!("Project auto-saved successfully to {}", project_file_path.display());
                        return true;
                    }
                } else {
                    eprintln!("Failed to serialize project state for auto-save");
                    return false;
                }
            }
        }
        false
    }

    pub fn load_project(&mut self) {
        if let Some(project_file) = FileDialog::new()
            .add_filter("DAW Project", &["json"])
            .pick_file()
        {
            if self.load_project_from_path(project_file.clone()) {
                Self::save_config(Some(project_file));
            }
        }
    }

    fn load_project_from_path(&mut self, path: PathBuf) -> bool {
        eprintln!("Attempting to load project from {}", path.display());
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(mut loaded_state) = serde_json::from_str::<DawState>(&contents) {
                // Get the project folder (parent directory of the project file)
                let project_folder = path.parent().unwrap_or(Path::new("")).to_path_buf();
                
                // Ensure waveforms directory exists
                let waveforms_dir = project_folder.join("waveforms");
                if !waveforms_dir.exists() {
                    if let Err(e) = std::fs::create_dir_all(&waveforms_dir) {
                        eprintln!("Failed to create waveforms directory for loading: {}", e);
                    }
                }
                
                // Set the file path in the loaded state
                loaded_state.file_path = Some(path.clone());
                
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
                
                // Scan for AudioBoxes in the project directory
                if loaded_state.audio_boxes.is_empty() {
                    // Only scan if we don't have any boxes in our state
                    if let Ok(entries) = std::fs::read_dir(&project_folder) {
                        for entry in entries.filter_map(|e| e.ok()) {
                            let path = entry.path();
                            if path.is_dir() {
                                // Skip the waveforms directory 
                                if path.file_name().and_then(|n| n.to_str()) == Some("waveforms") {
                                    continue;
                                }
                                
                                // Check if this directory has a render.wav or state.json file
                                let render_path = path.join("render.wav");
                                let state_path = path.join("state.json");
                                
                                if render_path.exists() || state_path.exists() {
                                    // This is likely an AudioBox
                                    if let Some(box_name) = path.file_name().and_then(|n| n.to_str()) {
                                        if !loaded_state.audio_boxes.contains(&box_name.to_string()) {
                                            loaded_state.audio_boxes.push(box_name.to_string());
                                            eprintln!("Found AudioBox: {}", box_name);
                                        }
                                    }
                                }
                            }
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

        // Create a default loop range from bar 1 to bar 4 if not set
        if app.state.loop_range.is_none() {
            let bars_per_beat = 4.0; // 4 beats per bar in 4/4 time
            let default_start = 1.0 * bars_per_beat * (60.0 / app.state.bpm); // 1 bar in seconds (start at bar 1)
            let default_end = 4.0 * bars_per_beat * (60.0 / app.state.bpm); // 4 bars in seconds (end at bar 4)
            app.state.loop_range = Some((default_start, default_end));
        }

        // Try to load last project if exists
        if let Some(path) = Self::load_config() {
            if path.exists() {
                eprintln!("Loading last project from {}", path.display());
                app.load_project_from_path(path);
            } else {
                eprintln!("Last project path not found: {}", path.display());
            }
        }
        
        // Debug output
        if let Some(path) = &app.state.file_path {
            eprintln!("Current project file path set to: {}", path.display());
        } else {
            eprintln!("No project file path set in state");
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
                // Update last_clicked_position too unless it's explicitly set elsewhere
                self.state.last_clicked_position = position;
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
            DawAction::SetClickedPosition(position) => {
                // Only update the clicked position marker without affecting the playhead
                self.state.last_clicked_position = position;
                // No need to update sample playback positions
                eprintln!("Setting clicked position to: {}", position);
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

                    // Start playback from the blue marker position if it exists
                    if self.state.last_clicked_position > 0.0 {
                        self.state.timeline_position = self.state.last_clicked_position;
                    }
                    // Otherwise, use the clicked bar position if set
                    else if self.state.last_clicked_bar > 0.0 {
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

                // Also convert horizontal scroll offset from time to beats
                let scroll_offset_in_beats = self.state.h_scroll_offset * (self.state.bpm / 60.0);

                // Update the BPM
                self.state.bpm = bpm;

                // Update track and sample timings
                for track in &mut self.state.tracks {
                    track.update_grid_times(bpm);
                }

                // Convert back to time at new BPM, maintaining the same beat position
                self.state.timeline_position = current_position_in_beats * (60.0 / bpm);

                // Also convert scroll offset back to time at new BPM
                self.state.h_scroll_offset = scroll_offset_in_beats * (60.0 / bpm);
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
                    sample.item_type = TrackItemType::Sample; // Mark this sample as a sample

                    // Try to load the audio file first
                    if let Some(path_ref) = &sample.audio_file {
                        match load_audio(path_ref) {
                            Ok((samples, _sample_rate)) => {
                                // Only initialize the sample if loading succeeded
                                sample.total_frames = samples.len();

                                // Load samples into the audio buffer
                                if let Ok(mut buffer) = sample.audio_buffer.lock() {
                                    buffer.clear();
                                    buffer.extend_from_slice(&samples);
                                }

                                // Initialize waveform and create stream
                                sample.load_waveform(None, Some(self.state.bpm));
                                sample.update_grid_times(self.state.bpm);
                                sample.create_stream(&self.audio);

                                // Add the sample to the track only if loading succeeded
                                track.add_sample(sample);
                                let new_sample_id = track.samples.last().map(|s| s.id).unwrap_or(0);

                                // Handle overlapping samples
                                if let Some(new_sample) = track.get_sample(new_sample_id) {
                                    let current_sample_start = new_sample.grid_position;
                                    let current_sample_end =
                                        new_sample.grid_position + new_sample.grid_length;

                                    // Find overlapping samples
                                    let overlapping_samples: Vec<usize> = track
                                        .samples
                                        .iter()
                                        .filter(|s| s.id != new_sample_id) // Skip the newly added sample
                                        .filter(|s| {
                                            let other_start = s.grid_position;
                                            let other_end = s.grid_position + s.grid_length;

                                            // Check if the samples overlap
                                            current_sample_start < other_end
                                                && current_sample_end > other_start
                                        })
                                        .map(|s| s.id)
                                        .collect();

                                    // Adjust the length of overlapping samples
                                    for overlap_id in overlapping_samples {
                                        if let Some(other_sample) = track.get_sample_mut(overlap_id)
                                        {
                                            // If this is a sample that starts before our new sample
                                            if other_sample.grid_position < current_sample_start {
                                                // Adjust its length to end exactly at the start of our new sample
                                                let new_length = current_sample_start
                                                    - other_sample.grid_position;
                                                eprintln!("Adjusting sample {} length from {} to {} due to overlap with new sample {}", 
                                                         other_sample.id, other_sample.grid_length, new_length, new_sample_id);
                                                other_sample.grid_length = new_length;
                                                other_sample.update_grid_times(self.state.bpm);
                                            }
                                        }
                                    }
                                }

                                eprintln!(
                                    "Added sample to track {}: {}",
                                    track.name,
                                    path.display()
                                );
                            }
                            Err(err) => {
                                eprintln!("Failed to add sample to track: {}", err);
                                // Show error message dialog
                                MessageDialog::new()
                                    .set_title("Audio Error")
                                    .set_description(&err.to_string())
                                    .show();
                            }
                        }
                    }
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
                        for other_id in overlapping_samples {
                            if let Some(other_sample) = track.get_sample_mut(other_id) {
                                // If the other sample starts before the current one
                                if other_sample.grid_position < current_sample_start {
                                    let new_length =
                                        current_sample_start - other_sample.grid_position;
                                    if new_length > 0.0 {
                                        other_sample.grid_length = new_length;
                                        other_sample.update_grid_times(self.state.bpm);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            DawAction::MoveSampleBetweenTracks(
                source_track_id,
                sample_id,
                target_track_id,
                new_position,
            ) => {
                // Find source track and get the sample
                let sample_to_move = if let Some(source_track) = self
                    .state
                    .tracks
                    .iter_mut()
                    .find(|t| t.id == source_track_id)
                {
                    // Get the sample index
                    let sample_index = source_track.samples.iter().position(|s| s.id == sample_id);

                    // If sample found, remove it from source track
                    if let Some(idx) = sample_index {
                        Some(source_track.samples.remove(idx))
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Now if we have the sample, add it to the target track
                if let Some(mut sample) = sample_to_move {
                    if let Some(target_track) = self
                        .state
                        .tracks
                        .iter_mut()
                        .find(|t| t.id == target_track_id)
                    {
                        // Update the sample's position to the new position
                        sample.grid_position = new_position;
                        sample.update_grid_times(self.state.bpm);

                        // Add sample to target track
                        target_track.samples.push(sample);

                        // Now handle any overlapping samples in the target track
                        let current_sample_id = target_track.samples.last().unwrap().id;
                        let current_sample_start = new_position;
                        let current_sample_end =
                            new_position + target_track.samples.last().unwrap().grid_length;

                        // Collect samples that overlap with the moved sample
                        let overlapping_samples: Vec<usize> = target_track
                            .samples
                            .iter()
                            .filter(|s| s.id != current_sample_id) // Skip the moved sample
                            .filter(|s| {
                                let other_start = s.grid_position;
                                let other_end = s.grid_position + s.grid_length;

                                // Check if the samples overlap
                                current_sample_start < other_end && current_sample_end > other_start
                            })
                            .map(|s| s.id)
                            .collect();

                        // Adjust the length of overlapping samples
                        for other_id in overlapping_samples {
                            if let Some(other_sample) = target_track.get_sample_mut(other_id) {
                                // If the other sample starts before the moved one
                                if other_sample.grid_position < current_sample_start {
                                    let new_length =
                                        current_sample_start - other_sample.grid_position;
                                    if new_length > 0.0 {
                                        other_sample.grid_length = new_length;
                                        other_sample.update_grid_times(self.state.bpm);
                                    }
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
                // No longer set the loop range based on selection
                // We use SetLoopRangeFromSelection for that
            }
            DawAction::RenderSelection(path) => {
                if let Some(selection) = &self.state.selection {
                    self.render_selection(&path, selection);
                } else {
                    eprintln!("Cannot render: No selection active");
                }
            }
            DawAction::SetZoomLevel(level) => {
                self.state.zoom_level = level.clamp(0.1, 10.0);
            }
            DawAction::SetLoopRangeFromSelection => {
                if let Some(selection) = &self.state.selection {
                    let start_time = selection.start_beat * (60.0 / self.state.bpm);
                    let end_time = selection.end_beat * (60.0 / self.state.bpm);
                    self.state.loop_range = Some((start_time, end_time));
                }
            }
            DawAction::CreateGroup(name) => {
                // Implementation of creating a new Group
                if let Some(project_path) = self.state.file_path.as_ref() {
                    if let Some(project_dir) = project_path.parent() {
                        match Group::new(&name, project_dir) {
                            Ok(_) => {
                                eprintln!("Created new Group: {}", name);
                                
                                // Add this Group to the list of known groups
                                if !self.state.audio_boxes.contains(&name) {
                                    self.state.audio_boxes.push(name.clone());
                                }
                                
                                // Create samples directory for the Group
                                let box_path = project_dir.join(&name);
                                let samples_dir = box_path.join("samples");
                                if !samples_dir.exists() {
                                    if let Err(e) = std::fs::create_dir_all(&samples_dir) {
                                        eprintln!("Failed to create samples directory: {}", e);
                                    }
                                }
                                
                                // Copy selected samples to the Group if there's a selection
                                if let Some(selection) = &self.state.selection {
                                    let mut copied_samples = 0;
                                    
                                    // Iterate through selected tracks
                                    for track_idx in selection.start_track_idx..=selection.end_track_idx {
                                        if let Some(track) = self.state.tracks.get(track_idx) {
                                            // Find samples within the beat range
                                            for sample in &track.samples {
                                                if sample.grid_position + sample.grid_length >= selection.start_beat && 
                                                   sample.grid_position <= selection.end_beat {
                                                    if let Some(source_path) = &sample.audio_file {
                                                        if source_path.exists() {
                                                            let filename = source_path.file_name().unwrap_or_else(|| std::ffi::OsStr::new("sample.wav"));
                                                            let target_path = samples_dir.join(filename);
                                                            
                                                            // Copy the file
                                                            if let Err(e) = std::fs::copy(source_path, &target_path) {
                                                                eprintln!("Failed to copy sample to Group: {}", e);
                                                            } else {
                                                                copied_samples += 1;
                                                                eprintln!("Copied sample to Group: {:?}", target_path);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    
                                    eprintln!("Copied {} samples to Group '{}'", copied_samples, name);
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to create Group: {}", e);
                            }
                        }
                    }
                } else {
                    eprintln!("No project file path set, cannot create Group");
                }
            }
            DawAction::RenameGroup(old_name, new_name) => {
                // Implementation of renaming a Group
                if let Some(project_path) = self.state.file_path.as_ref() {
                    if let Some(project_dir) = project_path.parent() {
                        let box_path = project_dir.join(&old_name);
                        if let Ok(mut audio_box) = Group::load(&box_path) {
                            if let Err(e) = audio_box.rename(&new_name, project_dir) {
                                eprintln!("Failed to rename Group: {}", e);
                            } else {
                                // Update the entry in audio_boxes
                                if let Some(index) = self.state.audio_boxes.iter().position(|n| n == &old_name) {
                                    self.state.audio_boxes[index] = new_name.clone();
                                } else {
                                    // If not found, add it
                                    self.state.audio_boxes.push(new_name.clone());
                                }
                                
                                eprintln!("Renamed AudioBox from {} to {}", old_name, new_name);
                                
                                // Update tab names for this AudioBox
                                for tab in &mut self.state.tabs {
                                    if tab.is_group && tab.group_name.as_ref().map_or(false, |n| n == &old_name) {
                                        tab.name = format!("Box: {}", new_name);
                                        tab.group_name = Some(new_name.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            DawAction::DeleteGroup(name) => {
                // Implementation of deleting a Group
                if let Some(project_path) = self.state.file_path.as_ref() {
                    if let Some(project_dir) = project_path.parent() {
                        // ... existing code ...
                    }
                }
            }
            DawAction::AddGroupToTrack(track_id, box_name) => {
                // Implementation of adding a Group to a track
                if let Some(project_path) = self.state.file_path.as_ref() {
                    if let Some(project_dir) = project_path.parent() {
                        let box_path = project_dir.join(&box_name);
                        if box_path.exists() {
                            // Load the Group
                            if let Ok(group) = Group::load(&box_path) {
                                // First, check if this group exists in ANY track and remove it
                                let mut source_track_id = None;
                                let mut source_sample_id = None;
                                
                                // Search all tracks for this group
                                for (track_idx, track) in self.state.tracks.iter().enumerate() {
                                    if let Some(sample_idx) = track.samples.iter().position(|s| 
                                        s.item_type == TrackItemType::Group && s.name == box_name) {
                                        source_track_id = Some(track.id);
                                        source_sample_id = Some(track.samples[sample_idx].id);
                                        eprintln!("Found existing group '{}' in track {} with sample ID {}", 
                                            box_name, track.id, track.samples[sample_idx].id);
                                        break;
                                    }
                                }
                                
                                // If we found the group in another track, remove it
                                if let (Some(source_track), Some(source_sample)) = (source_track_id, source_sample_id) {
                                    if source_track != track_id {
                                        // Remove from source track
                                        if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == source_track) {
                                            if let Some(pos) = track.samples.iter().position(|s| s.id == source_sample) {
                                                track.samples.remove(pos);
                                                eprintln!("Removed group '{}' from source track {}", box_name, source_track);
                                            }
                                        }
                                    } else {
                                        // Group is already in the target track, just update its position
                                        eprintln!("Group '{}' is already in target track {}", box_name, track_id);
                                        return;
                                    }
                                }
                                
                                // Find the target track
                                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                                    // Check if the group already exists in this track and remove it
                                    let existing_group_index = track.samples.iter().position(|s| 
                                        s.item_type == TrackItemType::Group && s.name == box_name);
                                    
                                    if let Some(index) = existing_group_index {
                                        // Remove the existing group
                                        track.samples.remove(index);
                                        eprintln!("Removed existing group '{}' from track {}", box_name, track_id);
                                    }
                                    
                                    // Create a new sample to represent the Group
                                    let mut sample = Sample::default();
                                    sample.name = box_name.clone();
                                    sample.item_type = TrackItemType::Group;
                                    sample.grid_position = 0.0; // This will be updated by the drag system
                                    sample.grid_length = 4.0; // Default length of 4 beats
                                    sample.waveform = Some(SampleWaveform {
                                        samples: group.waveform.clone(),
                                        sample_rate: 44100,
                                        duration: 4.0, // Default duration of 4 seconds
                                    });
                                    
                                    // Add the sample to the track
                                    track.add_sample(sample);
                                    eprintln!("Added Group '{}' to track {}", box_name, track_id);
                                }
                            } else {
                                eprintln!("Failed to load Group: {}", box_name);
                            }
                        } else {
                            eprintln!("Group path does not exist: {:?}", box_path);
                        }
                    }
                }
            }
            DawAction::RenderGroupFromSelection(box_name) => {
                // Implementation of rendering the current selection to an AudioBox
                if let Some(selection) = &self.state.selection {
                    if let Some(project_path) = self.state.file_path.as_ref() {
                        if let Some(project_dir) = project_path.parent() {
                            // Create the AudioBox
                            match Group::new(&box_name, project_dir) {
                                Ok(mut audio_box) => {
                                    // Get the selection data
                                    let start_track_idx = selection.start_track_idx;
                                    let end_track_idx = selection.end_track_idx;
                                    let start_beat = selection.start_beat;
                                    let end_beat = selection.end_beat;
                                    
                                    // Convert beats to time
                                    let start_time = start_beat * (60.0 / self.state.bpm);
                                    let end_time = end_beat * (60.0 / self.state.bpm);
                                    
                                    // Calculate the duration in seconds
                                    let duration = end_time - start_time;
                                    if duration <= 0.0 {
                                        eprintln!("Cannot render with zero or negative duration");
                                        return;
                                    }
                                    
                                    // Create a buffer for the mixed audio
                                    let sample_rate = 44100; // Standard sample rate
                                    let num_samples = (duration * sample_rate as f32) as usize;
                                    let mut mixed_buffer = vec![0.0; num_samples];
                                    
                                    // For simplicity, we'll mix down all tracks to mono
                                    for track_idx in start_track_idx..=end_track_idx {
                                        if let Some(track) = self.state.tracks.get(track_idx) {
                                            for sample in &track.samples {
                                                // Skip if sample is outside the selection
                                                if sample.grid_end_time <= start_time || sample.grid_start_time >= end_time {
                                                    continue;
                                                }
                                                
                                                // Get the audio data
                                                if let Ok(audio_data) = sample.audio_buffer.lock() {
                                                    // Calculate the relative position in the mixed buffer
                                                    let sample_offset = ((sample.grid_start_time - start_time) * sample_rate as f32).max(0.0) as usize;
                                                    
                                                    // Calculate how many samples to mix
                                                    let available_samples = audio_data.len();
                                                    let start_sample = ((sample.trim_start * sample_rate as f32).max(0.0)) as usize;
                                                    let end_sample = if sample.trim_end > 0.0 {
                                                        ((sample.trim_end * sample_rate as f32).min(available_samples as f32)) as usize
                                                    } else {
                                                        available_samples
                                                    };
                                                    
                                                    // Get the samples to mix
                                                    for (i, sample_idx) in (start_sample..end_sample).enumerate() {
                                                        let target_idx = sample_offset + i;
                                                        if target_idx < mixed_buffer.len() && sample_idx < available_samples {
                                                            // For now, simply add the samples (basic mixing)
                                                            mixed_buffer[target_idx] += audio_data[sample_idx];
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    
                                    // Normalize the mixed buffer
                                    let mut max_amplitude = 0.0f32;
                                    for sample in &mixed_buffer {
                                        max_amplitude = max_amplitude.max(sample.abs());
                                    }
                                    
                                    if max_amplitude > 0.0 {
                                        let normalize_factor = 0.9 / max_amplitude; // Leave some headroom
                                        for sample in &mut mixed_buffer {
                                            *sample *= normalize_factor;
                                        }
                                    }
                                    
                                    // Render the mixed buffer to the AudioBox
                                    if let Err(e) = audio_box.render(&mixed_buffer, sample_rate) {
                                        eprintln!("Failed to render AudioBox: {}", e);
                                    } else {
                                        eprintln!("Successfully rendered selection to AudioBox: {}", box_name);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to create AudioBox: {}", e);
                                }
                            }
                        }
                    }
                } else {
                    eprintln!("Cannot render: No selection active");
                }
            }
            DawAction::OpenGroupInNewTab(box_name) => {
                // Implementation of opening an AudioBox in a new tab
                // First check if we already have a tab open for this box
                if let Some(existing_tab) = self.state.tabs.iter().find(|t| 
                    t.is_group && 
                    t.group_name.as_ref().map_or(false, |name| name == &box_name)
                ) {
                    // Stop any current playback before switching tabs
                    if self.state.is_playing {
                        self.state.is_playing = false;
                        
                        // Make sure all samples are paused
                        for track in &mut self.state.tracks {
                            for sample in &mut track.samples {
                                if sample.is_playing {
                                    sample.pause();
                                }
                            }
                        }
                        
                        eprintln!("Paused playback when opening box in existing tab");
                    }
                    
                    // Box is already open in a tab, switch to it
                    self.state.active_tab_id = existing_tab.id;
                    eprintln!("Switched to existing tab for AudioBox '{}'", box_name);
                    return;
                }
                
                // Stop any current playback before creating a new tab
                if self.state.is_playing {
                    self.state.is_playing = false;
                    
                    // Make sure all samples are paused
                    for track in &mut self.state.tracks {
                        for sample in &mut track.samples {
                            if sample.is_playing {
                                sample.pause();
                            }
                        }
                    }
                    
                    eprintln!("Paused playback when opening box in new tab");
                }
                
                if let Some(project_path) = self.state.file_path.as_ref() {
                    if let Some(project_dir) = project_path.parent() {
                        let box_path = project_dir.join(&box_name);
                        if box_path.exists() && box_path.is_dir() {
                            // Check if there's a state.json file for the full state
                            let box_state_path = box_path.join("state.json");
                            if box_state_path.exists() {
                                eprintln!("Found state.json file for AudioBox '{}'", box_name);
                                
                                // Load the state from the file
                                if let Ok(contents) = fs::read_to_string(&box_state_path) {
                                    if let Ok(mut loaded_state) = serde_json::from_str::<DawState>(&contents) {
                                        // Create a new tab for this audio box
                                        let tab_id = if self.state.tabs.is_empty() {
                                            0
                                        } else {
                                            self.state.tabs.iter().map(|t| t.id).max().unwrap_or(0) + 1
                                        };
                                        
                                        // Create a new tab
                                        let tab = Tab {
                                            id: tab_id,
                                            name: format!("Box: {}", box_name),
                                            is_group: true,
                                            group_name: Some(box_name.clone()),
                                        };
                                        
                                        // Store the current state
                                        let current_state = self.state.clone();
                                        
                                        // Replace the state with the loaded state
                                        self.state = loaded_state;
                                        
                                        // But keep some settings from the current state
                                        self.state.tabs = current_state.tabs;
                                        self.state.tabs.push(tab);
                                        self.state.active_tab_id = tab_id;
                                        
                                        // Load audio data for all samples
                                        for track in &mut self.state.tracks {
                                            for sample in &mut track.samples {
                                                if let Some(path) = &sample.audio_file {
                                                    match load_audio(path) {
                                                        Ok((samples, sample_rate)) => {
                                                            // Initialize sample with loaded audio data
                                                            let buffer = Arc::new(Mutex::new(samples.clone()));
                                                            sample.audio_buffer = buffer;
                                                            
                                                            // Initialize sample index
                                                            sample.sample_index = Arc::new(AtomicUsize::new(0));
                                                            
                                                            // Create audio stream
                                                            sample.create_stream(&self.audio);
                                                            
                                                            // Generate waveform
                                                            let duration = samples.len() as f32 / sample_rate as f32;
                                                            sample.waveform = Some(SampleWaveform {
                                                                samples: generate_waveform(&samples, 1000),
                                                                sample_rate,
                                                                duration,
                                                            });
                                                        }
                                                        Err(e) => {
                                                            eprintln!("Error loading audio: {:?}", e);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        
                                        eprintln!("Loaded full state for AudioBox '{}'", box_name);
                                        return;
                                    } else {
                                        eprintln!("Failed to parse AudioBox state file");
                                    }
                                } else {
                                    eprintln!("Failed to read AudioBox state file");
                                }
                            }
                            
                            // Fallback to the old method if no state.json file or failed to load it
                            // Create a new tab for this audio box
                            let tab_id = if self.state.tabs.is_empty() {
                                0
                            } else {
                                self.state.tabs.iter().map(|t| t.id).max().unwrap_or(0) + 1
                            };
                            
                            // Create a new tab
                            let tab = Tab {
                                id: tab_id,
                                name: format!("Box: {}", box_name),
                                is_group: true,
                                group_name: Some(box_name.clone()),
                            };
                            
                            // Add the tab to tabs list
                            self.state.tabs.push(tab);
                            
                            // Set this tab as active
                            self.state.active_tab_id = tab_id;
                            
                            // Load samples folder from the box path
                            let samples_dir = box_path.join("samples");
                            if samples_dir.exists() && samples_dir.is_dir() {
                                // We'll load any samples in this folder to the first track in the AudioBox
                                if let Ok(entries) = std::fs::read_dir(&samples_dir) {
                                    for entry in entries.filter_map(|e| e.ok()) {
                                        let sample_path = entry.path();
                                        
                                        // Check if it's an audio file
                                        if sample_path.is_file() {
                                            let extension = sample_path.extension()
                                                .and_then(|ext| ext.to_str())
                                                .unwrap_or("")
                                                .to_lowercase();
                                                
                                            if ["wav", "mp3", "ogg", "flac"].contains(&extension.as_str()) {
                                                // The first track has ID 0 in our AudioBox
                                                // Load this sample in the box's context
                                                eprintln!("Loading sample for AudioBox: {:?}", sample_path);
                                                
                                                // This will be handled by the UI to add to the correct context
                                                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == 0) {
                                                    let mut sample = Sample::default();
                                                    sample.audio_file = Some(sample_path.clone());
                                                    sample.name = sample_path.file_name()
                                                        .and_then(|n| n.to_str())
                                                        .unwrap_or("Unknown")
                                                        .to_string();
                                                    
                                                    // Try to load the audio file
                                                    if let Some(path) = &sample.audio_file {
                                                        match load_audio(path) {
                                                            Ok((samples, sample_rate)) => {
                                                                // Initialize sample with loaded audio data
                                                                let duration = samples.len() as f32 / sample_rate as f32;
                                                                
                                                                // Initialize AudioBuffer
                                                                let buffer = Arc::new(Mutex::new(samples));
                                                                sample.audio_buffer = buffer;
                                                                
                                                                // Initialize sample index
                                                                sample.sample_index = Arc::new(AtomicUsize::new(0));
                                                                
                                                                // Calculate grid length based on duration
                                                                sample.grid_length = duration * (self.state.bpm / 60.0);
                                                                sample.update_grid_times(self.state.bpm);
                                                                
                                                                // Get a unique ID
                                                                sample.id = track.samples.len(); // Use length as new ID
                                                                
                                                                // Set the position to a sensible place
                                                                sample.grid_position = track.samples.iter()
                                                                    .map(|s| s.grid_position + s.grid_length)
                                                                    .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                                                                    .unwrap_or(0.0);
                                                                sample.update_grid_times(self.state.bpm);
                                                                
                                                                // Create audio stream
                                                                sample.create_stream(&self.audio);
                                                                
                                                                // Generate waveform
                                                                {
                                                                    let buffer_guard = sample.audio_buffer.lock().unwrap();
                                                                    
                                                                    // Generate a smaller waveform for display
                                                                    sample.waveform = Some(SampleWaveform {
                                                                        samples: generate_waveform(&buffer_guard, 1000),
                                                                        sample_rate,
                                                                        duration,
                                                                    });
                                                                }
                                                                
                                                                // Add sample to track
                                                                track.samples.push(sample);
                                                            }
                                                            Err(e) => {
                                                                eprintln!("Error loading audio: {:?}", e);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            
                            eprintln!("Opened AudioBox '{}' in new tab", box_name);
                        } else {
                            eprintln!("AudioBox not found: {}", box_name);
                        }
                    }
                } else {
                    eprintln!("No project path available to locate AudioBox");
                }
            }
            DawAction::SwitchToTab(tab_id) => {
                // Implementation of switching to a different tab
                if let Some(tab) = self.state.tabs.iter().find(|t| t.id == tab_id) {
                    // Check if we're switching between different tab types (audio box vs project)
                    let current_tab = self.state.tabs.iter().find(|t| t.id == self.state.active_tab_id);
                    let is_switching_tab_types = current_tab.map_or(false, |current| {
                        current.is_group != tab.is_group
                    });
                    
                    // If we're switching between tab types and playback is active, pause playback
                    if is_switching_tab_types && self.state.is_playing {
                        self.state.is_playing = false;
                        
                        // Make sure all samples are paused
                        for track in &mut self.state.tracks {
                            for sample in &mut track.samples {
                                if sample.is_playing {
                                    sample.pause();
                                }
                            }
                        }
                        
                        eprintln!("Paused playback when switching between different editor types");
                    }
                    
                    self.state.active_tab_id = tab_id;
                    eprintln!("Switched to tab: {}", tab.name);
                } else {
                    eprintln!("Tab not found: {}", tab_id);
                }
            }
            DawAction::CloseTab(tab_id) => {
                // Implementation of closing a tab
                if let Some(tab_index) = self.state.tabs.iter().position(|t| t.id == tab_id) {
                    // Stop any current playback when closing a tab
                    if self.state.is_playing {
                        self.state.is_playing = false;
                        
                        // Make sure all samples are paused
                        for track in &mut self.state.tracks {
                            for sample in &mut track.samples {
                                if sample.is_playing {
                                    sample.pause();
                                }
                            }
                        }
                        
                        eprintln!("Paused playback when closing tab");
                    }
                    
                    self.state.tabs.remove(tab_index);
                    
                    // If we're closing the active tab, switch to another tab
                    if self.state.active_tab_id == tab_id {
                        if let Some(first_tab) = self.state.tabs.first() {
                            self.state.active_tab_id = first_tab.id;
                        }
                    }
                    
                    eprintln!("Closed tab: {}", tab_id);
                } else {
                    eprintln!("Tab not found: {}", tab_id);
                }
            }
            DawAction::SaveGroup(box_name) => {
                // Implementation of saving an AudioBox state and updating its render.wav
                if let Some(project_path) = self.state.file_path.as_ref() {
                    if let Some(project_dir) = project_path.parent() {
                        let box_path = project_dir.join(&box_name);
                        
                        // Ensure the box directory exists
                        if !box_path.exists() {
                            if let Err(e) = std::fs::create_dir_all(&box_path) {
                                eprintln!("Failed to create AudioBox directory: {:?}", e);
                                return;
                            }
                        }
                        
                        // Ensure samples directory exists
                        let samples_dir = box_path.join("samples");
                        if !samples_dir.exists() {
                            if let Err(e) = std::fs::create_dir_all(&samples_dir) {
                                eprintln!("Failed to create samples directory: {:?}", e);
                                return;
                            }
                        }
                        
                        // Save full project state to support multi-track audio boxes
                        let box_state_path = box_path.join("state.json");
                        
                        if let Some(tab) = self.state.tabs.iter().find(|t| 
                            t.is_group && 
                            t.group_name.as_ref().map_or(false, |name| name == &box_name)
                        ) {
                            // Create a copy of the current state to save
                            let mut box_state = self.state.clone();
                            
                            // Keep track of this AudioBox in the main project
                            if !self.state.audio_boxes.contains(&box_name) {
                                self.state.audio_boxes.push(box_name.clone());
                            }
                            
                            // Update metadata
                            box_state.project_name = box_name.clone();
                            box_state.file_path = Some(box_state_path.clone());
                            
                            // Serialize and save the state
                            if let Ok(serialized) = serde_json::to_string_pretty(&box_state) {
                                if let Err(e) = fs::write(&box_state_path, serialized) {
                                    eprintln!("Failed to write AudioBox state file: {:?}", e);
                                } else {
                                    eprintln!("Saved full state for AudioBox '{}'", box_name);
                                }
                            } else {
                                eprintln!("Failed to serialize AudioBox state");
                            }
                            
                            // Render tracks to a single audio file
                            let render_path = box_path.join("render.wav");
                            
                            // For now, just mix all active samples
                            let mut final_samples = Vec::new();
                            let mut final_rate = 44100;
                            
                            for track in &self.state.tracks {
                                if track.muted {
                                    continue;
                                }
                                
                                for sample in &track.samples {
                                    if let Some(waveform) = &sample.waveform {
                                        if let Ok(buffer_guard) = sample.audio_buffer.lock() {
                                            // Apply trim points
                                            let start_frame = (sample.trim_start * waveform.sample_rate as f32) as usize;
                                            let end_frame = if sample.trim_end <= 0.0 {
                                                buffer_guard.len()
                                            } else {
                                                (sample.trim_end * waveform.sample_rate as f32) as usize
                                            };
                                            
                                            // Get trimmed audio
                                            let trimmed_audio: Vec<f32> = buffer_guard
                                                .iter()
                                                .skip(start_frame)
                                                .take(end_frame.saturating_sub(start_frame))
                                                .cloned()
                                                .collect();
                                            
                                            // Mix with existing audio
                                            if final_samples.is_empty() {
                                                final_samples = trimmed_audio;
                                                final_rate = waveform.sample_rate;
                                            } else {
                                                // Simple mixing - in a real implementation we'd need to handle sample rate conversion
                                                // and different lengths better
                                                let max_len = final_samples.len().max(trimmed_audio.len());
                                                let mut mixed = Vec::with_capacity(max_len);
                                                
                                                for i in 0..max_len {
                                                    let a = if i < final_samples.len() { final_samples[i] } else { 0.0 };
                                                    let b = if i < trimmed_audio.len() { trimmed_audio[i] } else { 0.0 };
                                                    
                                                    // Simple mixing - 0.5 scale to avoid clipping
                                                    mixed.push((a + b) * 0.5);
                                                }
                                                
                                                final_samples = mixed;
                                            }
                                        }
                                    }
                                }
                            }
                            
                            // Save the render.wav file
                            if !final_samples.is_empty() {
                                if let Err(e) = crate::audio::save_audio(&render_path, &final_samples, final_rate) {
                                    eprintln!("Failed to save AudioBox render: {:?}", e);
                                } else {
                                    eprintln!("Updated AudioBox '{}' render file", box_name);
                                }
                            }
                        }
                    }
                }
            }
            DawAction::CreateTrack => {
                // Create a new track with a unique ID
                let new_id = self.state.next_track_id;
                self.state.next_track_id += 1;
                
                // Add the track to the project
                self.state.tracks.push(Track::new(new_id, format!("Track {}", new_id)));
                
                // Mark the project as modified
                self.state.modified = true;
            },
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
            let elapsed = now.duration_since(self.last_update).as_secs_f32();
            self.last_update = now;

            // Update timeline position
            self.state.timeline_position += elapsed;

            // Handle looping based on loop range only (independent from selection)
            if self.state.loop_enabled {
                // Use loop range for looping
                if let Some((start, end)) = self.state.loop_range {
                    if self.state.timeline_position >= end {
                        eprintln!("Looping back from loop range end to start");
                        self.state.timeline_position = start;

                        // Reset playback state for all samples
                        for track in &mut self.state.tracks {
                            for sample in &mut track.samples {
                                sample.reset_playback();
                            }
                        }
                    }
                }
            }

            let timeline_pos = self.state.timeline_position;
            let mut any_sample_playing = false;

            let any_track_soloed = self.state.tracks.iter().any(|t| t.soloed);
            
            // Check if we're in an audio box tab
            let active_tab = self.state.tabs.iter().find(|t| t.id == self.state.active_tab_id);
            let is_audio_box_tab = active_tab.map_or(false, |tab| tab.is_group);
            let audio_box_name = active_tab.and_then(|tab| tab.group_name.clone());

            for track in &mut self.state.tracks {
                if track.muted || (any_track_soloed && !track.soloed) {
                    continue;
                }

                for sample in &mut track.samples {
                    // Skip samples that don't belong to the current tab type
                    if is_audio_box_tab {
                        // In an audio box tab, only play samples with item_type == AudioBox that
                        // match the current audio box name
                        match &sample.item_type {
                            TrackItemType::Group => {
                                // If this sample doesn't represent the current audio box, skip it
                                if sample.name != audio_box_name.clone().unwrap_or_default() {
                                    // Ensure we pause it if it's somehow playing
                                    if sample.is_playing {
                                        sample.pause();
                                    }
                                    continue;
                                }
                            },
                            TrackItemType::Sample => {
                                // If we're in an audio box tab, don't play regular samples
                                // from the project
                                if sample.is_playing {
                                    sample.pause();
                                }
                                continue;
                            }
                        }
                    } else {
                        // In the main project tab, only play regular samples, not audio boxes
                        if let TrackItemType::Group = sample.item_type {
                            // If we're in the main project tab, don't play AudioBox samples
                            if sample.is_playing {
                                sample.pause();
                            }
                            continue;
                        }
                    }
                    
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
                last_clicked_bar: 0.0,
                last_clicked_position: 0.0, // Store the position of the last clicked track marker
                project_name: "Test Project".to_string(),
                file_path: None,
                h_scroll_offset: 0.0,
                v_scroll_offset: 0.0,
                selection: None,
                loop_enabled: false,
                zoom_level: 1.0,
                loop_range: None,
                tabs: default_tabs(),
                active_tab_id: 0,
                audio_boxes: Vec::new(),
                next_track_id: 5,
                modified: false,
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

        // Create the WAV file with appropriate specs
        use hound::{WavSpec, WavWriter};
        let spec = WavSpec {
            channels: num_channels as u16,
            sample_rate: target_sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let writer = match WavWriter::create(output_path, spec) {
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

        // Rest of your rendering code here...
        
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

    pub fn save_project_as(&mut self) {
        // Always prompt for folder location for "Save As"
        if let Some(folder_path) = FileDialog::new()
            .set_title("Save Project As")
            .pick_folder()
        {
            // Create a folder with the project name
            let project_name = if self.state.project_name.trim().is_empty() {
                "Untitled Project"
            } else {
                &self.state.project_name
            };
            
            let project_folder = folder_path.join(project_name);
            
            // Create the project folder if it doesn't exist
            if !project_folder.exists() {
                if let Err(e) = std::fs::create_dir_all(&project_folder) {
                    eprintln!("Failed to create project folder: {}", e);
                    return;
                }
            }
            
            // Create waveforms directory
            let waveforms_dir = project_folder.join("waveforms");
            if !waveforms_dir.exists() {
                if let Err(e) = std::fs::create_dir_all(&waveforms_dir) {
                    eprintln!("Failed to create waveforms directory: {}", e);
                }
            }
            
            // Save the project file inside the folder
            let project_file_path = project_folder.join("project.json");
            
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
                            save_waveform_data(&project_file_path, &sample.name, &waveform_data)
                        {
                            eprintln!("Saved waveform data to {}", waveform_path.display());
                        }
                    }
                }
            }

            // Then save the project state to project.json file in the project folder
            if let Ok(serialized) = serde_json::to_string_pretty(&self.state) {
                if fs::write(&project_file_path, serialized).is_ok() {
                    // Update the file_path in the state
                    self.state.file_path = Some(project_file_path.clone());
                    
                    // Save updated state with new file path
                    if let Ok(updated_serialized) = serde_json::to_string_pretty(&self.state) {
                        let _ = fs::write(&project_file_path, updated_serialized);
                    }
                    
                    Self::save_config(Some(project_file_path.clone()));
                    eprintln!("Project saved successfully to {}", project_file_path.display());
                } else {
                    eprintln!("Failed to write project file to {}", project_file_path.display());
                }
            } else {
                eprintln!("Failed to serialize project state");
            }
        }
    }

    // Add these methods after existing DawApp implementation methods but before any other impl blocks

    // Switch to the previous tab in the tabs list
    pub fn switch_to_previous_tab(&mut self) {
        if self.state.tabs.is_empty() {
            return;
        }
        
        // Find current tab index
        let current_index = self.state.tabs.iter().position(|t| t.id == self.state.active_tab_id);
        
        if let Some(idx) = current_index {
            // Calculate previous index (wrap around to the end)
            let prev_idx = if idx == 0 { self.state.tabs.len() - 1 } else { idx - 1 };
            
            // Switch to the previous tab
            if let Some(tab) = self.state.tabs.get(prev_idx) {
                self.dispatch(DawAction::SwitchToTab(tab.id));
            }
        }
    }
    
    // Switch to the next tab in the tabs list
    pub fn switch_to_next_tab(&mut self) {
        if self.state.tabs.is_empty() {
            return;
        }
        
        // Find current tab index
        let current_index = self.state.tabs.iter().position(|t| t.id == self.state.active_tab_id);
        
        if let Some(idx) = current_index {
            // Calculate next index (wrap around to the beginning)
            let next_idx = (idx + 1) % self.state.tabs.len();
            
            // Switch to the next tab
            if let Some(tab) = self.state.tabs.get(next_idx) {
                self.dispatch(DawAction::SwitchToTab(tab.id));
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Config {
    latest_project: Option<PathBuf>,
}

// Helper function to create a downsampled waveform for visualization
fn generate_waveform(samples: &[f32], target_size: usize) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }
    
    let samples_per_point = (samples.len() as f32 / target_size as f32).max(1.0) as usize;
    let mut waveform = Vec::with_capacity(target_size);
    
    for i in 0..target_size {
        let start = (i * samples_per_point).min(samples.len());
        let end = ((i + 1) * samples_per_point).min(samples.len());
        
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
