mod audio;

use audio::{load_audio, Audio};
use cpal::traits::StreamTrait;
use eframe::egui;
use egui::{Color32, Key, RichText, Stroke};
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
struct Track {
    name: String,
    audio_file: Option<PathBuf>,
    muted: bool,
    soloed: bool,
    recording: bool,
    #[serde(skip)]
    stream: Option<cpal::Stream>,
    #[serde(skip)]
    sample_index: Arc<AtomicUsize>,
    #[serde(skip)]
    audio_buffer: Arc<Mutex<Vec<f32>>>,
    duration: f32,
    current_position: f32,
    waveform_samples: Vec<f32>,
    sample_rate: u32,
    is_playing: bool,
    grid_position: f32,   // Position in the grid (in beats)
    grid_length: f32,     // Length in the grid (in beats)
    grid_start_time: f32, // When this track should start playing (in seconds)
    grid_end_time: f32,   // When this track should stop playing (in seconds)
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
            grid_length: 0.0, // Initialize to 0.0 instead of 4.0
            grid_start_time: 0.0,
            grid_end_time: 0.0, // Initialize to 0.0
        }
    }
}

impl Track {
    fn create_stream(&mut self, audio: &Audio) {
        // Only create stream if we have an audio file
        if self.audio_file.is_none() {
            eprintln!("Cannot create stream: No audio file loaded");
            return;
        }

        // Stop and drop existing stream if any
        if self.stream.is_some() {
            if let Err(e) = self.stream.as_ref().unwrap().pause() {
                eprintln!("Failed to pause existing stream: {}", e);
            }
            self.stream = None;
            self.is_playing = false;
        }

        // Initialize audio buffer if it doesn't exist
        if self.audio_buffer.lock().is_err() {
            self.audio_buffer = Arc::new(Mutex::new(vec![0.0; 1024 * 1024]));
        }

        let audio_buffer = Arc::clone(&self.audio_buffer);
        let sample_index = Arc::new(AtomicUsize::new(0));

        if let Some(stream) = audio.create_stream(audio_buffer, Arc::clone(&sample_index)) {
            self.stream = Some(stream);
            self.sample_index = sample_index;
            self.is_playing = false; // Ensure track is marked as not playing
            eprintln!("Created new audio stream (paused)");
        }
    }

    fn seek_to(&mut self, position: f32) {
        let mut index = self.sample_index.load(Ordering::Relaxed);
        index = (position * self.sample_rate as f32) as usize;
        self.sample_index.store(index, Ordering::Relaxed);
    }

    fn play(&mut self) {
        if let Some(stream) = &self.stream {
            if let Err(e) = stream.play() {
                eprintln!("Failed to play stream: {}", e);
                return;
            }
            self.is_playing = true;
            eprintln!("Started playing audio");
        } else {
            eprintln!("No audio stream available");
        }
    }

    fn pause(&mut self) {
        if let Some(stream) = &self.stream {
            if let Err(e) = stream.pause() {
                eprintln!("Failed to pause stream: {}", e);
                return;
            }
            self.is_playing = false;
            eprintln!("Paused audio");
        }
    }

    fn load_waveform(&mut self) {
        if let Some(path) = &self.audio_file {
            let path = path.clone();
            let (samples, sample_rate) = load_audio(&path);
            self.sample_rate = sample_rate;
            self.duration = samples.len() as f32 / sample_rate as f32;

            // Set grid length to match track duration (in beats)
            self.grid_length = self.duration * (120.0 / 60.0); // Convert seconds to beats at 120 BPM

            // Downsample waveform for display
            let downsample_factor = samples.len() / 1000; // Show 1000 points
            self.waveform_samples = samples
                .chunks(downsample_factor)
                .map(|chunk| {
                    chunk
                        .iter()
                        .map(|&s| s.abs()) // Get absolute value
                        .fold(0.0, f32::max) // Find peak in chunk
                })
                .collect();

            // Initialize audio buffer if it doesn't exist
            if self.audio_buffer.lock().is_err() {
                self.audio_buffer = Arc::new(Mutex::new(vec![0.0; 1024 * 1024]));
            }

            // Fill audio buffer with audio data
            if let Ok(mut buffer) = self.audio_buffer.lock() {
                buffer.clear();
                buffer.extend_from_slice(&samples);
                eprintln!("Loaded {} samples into audio buffer", samples.len());
            } else {
                eprintln!("Failed to lock audio buffer for writing");
            }
        }
    }

    fn current_position(&self) -> f32 {
        self.sample_index.load(Ordering::Relaxed) as f32 / self.sample_rate as f32
    }

    fn update_grid_times(&mut self, bpm: f32) {
        // Convert beats to seconds
        self.grid_start_time = self.grid_position * (60.0 / bpm);
        self.grid_end_time = (self.grid_position + self.grid_length) * (60.0 / bpm);
    }
}

#[derive(Serialize, Deserialize)]
struct DawState {
    timeline_position: f32,
    is_playing: bool,
    bpm: f32,
    tracks: Vec<Track>,
    grid_division: f32, // Grid division in beats (e.g., 0.25 for 16th notes)
}

#[derive(Serialize, Deserialize)]
struct Config {
    latest_project: Option<PathBuf>,
}

struct DawApp {
    state: DawState,
    last_update: std::time::Instant,
    seek_position: Option<f32>,
    audio: Audio,
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

    fn save_project(&self) {
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

    fn load_project(&mut self) {
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
                // Store the grid lengths before reloading audio files
                let grid_lengths: Vec<f32> =
                    loaded_state.tracks.iter().map(|t| t.grid_length).collect();

                // Reload audio files and recreate streams
                for (i, track) in loaded_state.tracks.iter_mut().enumerate() {
                    if let Some(path) = &track.audio_file {
                        eprintln!("Loading audio file: {}", path.display());
                        track.load_waveform();
                        track.create_stream(&self.audio);
                        // Ensure track is not playing when loaded
                        track.is_playing = false;
                        track.current_position = 0.0;
                        // Restore the user's grid length
                        track.grid_length = grid_lengths[i];
                    }
                }
                // Ensure playback is stopped when loading
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

    fn new() -> Self {
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
                grid_division: 0.25, // 16th notes by default
            },
            last_update: std::time::Instant::now(),
            seek_position: None,
            audio,
        };

        // Try to load the latest project if available
        if let Some(path) = Self::load_config() {
            app.load_project_from_path(path);
        }

        // Ensure all tracks are not playing after loading
        for track in &mut app.state.tracks {
            track.is_playing = false;
            track.current_position = 0.0;
        }

        app
    }

    fn update_playback(&mut self) {
        if self.state.is_playing {
            for track in &mut self.state.tracks {
                // Update grid times based on current BPM
                track.update_grid_times(self.state.bpm);

                // Check if current timeline position is within track's grid time
                if self.state.timeline_position >= track.grid_start_time
                    && self.state.timeline_position < track.grid_end_time
                {
                    if !track.is_playing {
                        // Calculate relative position within the track
                        let relative_position =
                            self.state.timeline_position - track.grid_start_time;
                        track.seek_to(relative_position);
                        track.play();
                    }
                } else {
                    if track.is_playing {
                        track.pause();
                    }
                    track.current_position = 0.0;
                }
            }
        } else {
            // When stopping playback, pause all tracks
            for track in &mut self.state.tracks {
                track.pause();
                track.current_position = 0.0;
            }
        }
    }
}

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Force continuous repaints at 60 FPS
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(1.0 / 60.0));

        // Handle seek position if set
        if let Some(click_position) = self.seek_position.take() {
            self.state.timeline_position = click_position;
            for track in &mut self.state.tracks {
                track.update_grid_times(self.state.bpm);
                if click_position >= track.grid_start_time && click_position < track.grid_end_time {
                    let relative_position = click_position - track.grid_start_time;
                    track.seek_to(relative_position);
                } else {
                    track.seek_to(0.0);
                }
            }
        }

        // Handle spacebar input
        if ctx.input(|i| i.key_pressed(Key::Space)) {
            self.state.is_playing = !self.state.is_playing;
            self.last_update = std::time::Instant::now();
            self.update_playback();
        }

        // Update timeline position based on audio playback
        if self.state.is_playing {
            let now = std::time::Instant::now();
            let delta = now.duration_since(self.last_update).as_secs_f32();
            self.state.timeline_position += delta;

            // Update track positions and playback
            for track in &mut self.state.tracks {
                track.update_grid_times(self.state.bpm);

                // Check if current timeline position is within track's grid time
                if self.state.timeline_position >= track.grid_start_time
                    && self.state.timeline_position < track.grid_end_time
                {
                    let relative_position = self.state.timeline_position - track.grid_start_time;
                    if !track.is_playing {
                        // Calculate relative position within the track
                        track.seek_to(relative_position);
                        track.play();
                    }
                    // Update current position
                    track.current_position = relative_position;
                } else {
                    if track.is_playing {
                        track.pause();
                    }
                    track.current_position = 0.0;
                }
            }

            self.last_update = now;
        } else {
            // When stopping playback, pause all tracks
            for track in &mut self.state.tracks {
                track.pause();
                track.current_position = 0.0;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Top toolbar
            ui.horizontal(|ui| {
                // Transport controls
                ui.horizontal(|ui| {
                    if ui.button("⏮").clicked() {
                        self.state.timeline_position = 0.0;
                        for track in &mut self.state.tracks {
                            track.is_playing = false;
                            track.current_position = 0.0;
                            track.seek_to(0.0);
                        }
                    }
                    if ui
                        .button(if self.state.is_playing { "⏸" } else { "▶" })
                        .clicked()
                    {
                        self.state.is_playing = !self.state.is_playing;
                        self.last_update = std::time::Instant::now();
                        self.update_playback();
                    }
                    if ui.button("⏭").clicked() {
                        self.state.timeline_position += 4.0;
                        for track in &mut self.state.tracks {
                            track.is_playing = false;
                            track.current_position = self.state.timeline_position;
                            track.seek_to(self.state.timeline_position);
                        }
                    }
                });

                // BPM control
                ui.label("BPM:");
                ui.add(
                    egui::DragValue::new(&mut self.state.bpm)
                        .speed(1.0)
                        .clamp_range(20.0..=240.0),
                );

                // Grid control
                ui.label("Grid:");
                ui.add(
                    egui::DragValue::new(&mut self.state.grid_division)
                        .speed(0.25)
                        .clamp_range(0.25..=4.0),
                );

                // Save/load buttons
                if ui.button("💾 Save").clicked() {
                    self.save_project();
                }
                if ui.button("📂 Open").clicked() {
                    self.load_project();
                }
            });

            ui.separator();

            // Create dedicated timeline area at the top
            let timeline_height = 50.0;
            let (timeline_response, timeline_painter) = ui.allocate_painter(
                egui::vec2(ui.available_width(), timeline_height),
                egui::Sense::click_and_drag(),
            );

            // Draw timeline grid and playhead in this area
            let timeline_rect = timeline_response.rect;
            let pixels_per_beat = timeline_rect.width() / (8.0 * 4.0); // 8 bars * 4 beats

            // Draw timeline grid lines (existing code moved here)
            for bar in 0..=8 {
                let x = timeline_rect.left() + (bar as f32 * 4.0 * pixels_per_beat);
                timeline_painter.line_segment(
                    [
                        egui::pos2(x, timeline_rect.top()),
                        egui::pos2(x, timeline_rect.bottom()),
                    ],
                    Stroke::new(2.0, Color32::from_rgb(100, 100, 100)),
                );
            }

            // Draw playhead in timeline area
            let playhead_x =
                timeline_rect.left() + (self.state.timeline_position * pixels_per_beat);
            timeline_painter.line_segment(
                [
                    egui::pos2(playhead_x, timeline_rect.top()),
                    egui::pos2(playhead_x, timeline_rect.bottom()),
                ],
                Stroke::new(2.0, Color32::RED),
            );

            // Handle timeline interaction ONLY in this area
            if timeline_response.dragged() || timeline_response.clicked() {
                if let Some(pos) = timeline_response.interact_pointer_pos() {
                    let click_x = pos.x - timeline_rect.left();
                    let click_beats = click_x / pixels_per_beat;
                    self.state.timeline_position = click_beats;

                    for track in &mut self.state.tracks {
                        track.seek_to(self.state.timeline_position * (60.0 / self.state.bpm));
                    }
                }
            }

            // Main grid area below timeline
            let available_height = ui.available_height() - 150.0 - timeline_height;
            let (grid_response, painter) = ui.allocate_painter(
                egui::vec2(ui.available_width(), available_height),
                egui::Sense::click_and_drag(), // Changed back from hover
            );

            if grid_response.rect.width() > 0.0 {
                let rect = grid_response.rect;
                let width = rect.width();
                let height = rect.height();

                // Draw grid lines
                let beats_per_bar = 4.0;
                let total_bars = 8.0;
                let total_beats = total_bars * beats_per_bar;
                let pixels_per_beat = width / total_beats;

                // Draw bar lines
                for bar in 0..=total_bars as i32 {
                    let x = rect.left() + (bar as f32 * beats_per_bar * pixels_per_beat);
                    painter.line_segment(
                        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                        Stroke::new(2.0, Color32::from_rgb(100, 100, 100)),
                    );
                }

                // Draw beat lines
                for beat in 0..=(total_beats * self.state.grid_division) as i32 {
                    let x =
                        rect.left() + (beat as f32 * pixels_per_beat / self.state.grid_division);
                    painter.line_segment(
                        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                        Stroke::new(1.0, Color32::from_rgb(50, 50, 50)),
                    );
                }

                // Draw time markers
                for bar in 0..=total_bars as i32 {
                    let x = rect.left() + (bar as f32 * beats_per_bar * pixels_per_beat);
                    let time = (bar as f32 * beats_per_bar * 60.0) / self.state.bpm;
                    let time_text = format!("{:.1}s", time);
                    painter.text(
                        egui::pos2(x + 2.0, rect.top() + 15.0),
                        egui::Align2::LEFT_TOP,
                        time_text,
                        egui::FontId::proportional(12.0),
                        Color32::WHITE,
                    );
                }

                // Draw tracks over the grid
                let track_height = 80.0;
                let track_spacing = 10.0;
                let total_tracks_height = (self.state.tracks.len() as f32
                    * (track_height + track_spacing))
                    + track_spacing;

                // Draw tracks in grid
                for (index, track) in self.state.tracks.iter_mut().enumerate() {
                    let track_y = rect.top()
                        + track_spacing
                        + (index as f32 * (track_height + track_spacing));

                    // Calculate track position in grid
                    let track_x = rect.left()
                        + (track.grid_position * (60.0 / self.state.bpm) * pixels_per_beat);
                    let track_width = track.grid_length * (60.0 / self.state.bpm) * pixels_per_beat;

                    // Draw track background
                    let track_rect = egui::Rect::from_min_size(
                        egui::pos2(track_x, track_y),
                        egui::vec2(track_width, track_height),
                    );

                    // Draw track background with color based on state
                    let bg_color = if track.muted {
                        Color32::from_rgb(50, 50, 50)
                    } else if track.soloed {
                        Color32::from_rgb(50, 50, 100)
                    } else {
                        Color32::from_rgb(30, 30, 30)
                    };
                    painter.rect_filled(track_rect, 5.0, bg_color);

                    // Draw track name
                    painter.text(
                        egui::pos2(track_x + 5.0, track_y + 20.0),
                        egui::Align2::LEFT_TOP,
                        &track.name,
                        egui::FontId::proportional(14.0),
                        Color32::WHITE,
                    );

                    // Draw waveform if available
                    if !track.waveform_samples.is_empty() {
                        let waveform_height = track_height - 40.0;
                        let waveform_y = track_y + 30.0;
                        let waveform_width = track_width - 10.0;
                        let waveform_x = track_x + 5.0;

                        // Draw waveform
                        for (i, &sample) in track.waveform_samples.iter().enumerate() {
                            let x = waveform_x
                                + (i as f32 / track.waveform_samples.len() as f32) * waveform_width;
                            let amplitude = sample * waveform_height * 0.8;
                            painter.line_segment(
                                [
                                    egui::pos2(x, waveform_y + waveform_height / 2.0 - amplitude),
                                    egui::pos2(x, waveform_y + waveform_height / 2.0 + amplitude),
                                ],
                                Stroke::new(2.0, Color32::from_rgb(150, 150, 150)),
                            );
                        }
                    }

                    // Handle track dragging
                    if grid_response.dragged() {
                        if let Some(pos) = grid_response.interact_pointer_pos() {
                            if track_rect.contains(pos) {
                                let click_x = pos.x - rect.left();
                                let grid_position = click_x / pixels_per_beat;
                                track.grid_position = grid_position;
                            }
                        }
                    }
                }

                // Draw playhead (red bar) - now drawn last so it appears on top
                let playhead_x =
                    timeline_rect.left() + (self.state.timeline_position * pixels_per_beat);
                ui.painter().line_segment(
                    [
                        egui::pos2(playhead_x, timeline_rect.top()),
                        egui::pos2(playhead_x, grid_response.rect.bottom()),
                    ],
                    Stroke::new(2.0, Color32::RED),
                );
            }

            // Track controls at the bottom
            ui.separator();
            ui.add_space(10.0);
            for (_index, track) in self.state.tracks.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    // Track name with color based on whether it has an audio file
                    ui.label(
                        RichText::new(&track.name).color(if track.audio_file.is_some() {
                            Color32::GREEN
                        } else {
                            Color32::WHITE
                        }),
                    );

                    // Start and End time controls in seconds
                    ui.label("Start:");
                    let mut start_time = track.grid_position * (60.0 / self.state.bpm);
                    if ui
                        .add(egui::DragValue::new(&mut start_time).speed(0.1))
                        .changed()
                    {
                        let new_grid_pos = start_time / (60.0 / self.state.bpm);
                        track.grid_position = new_grid_pos;
                    }

                    ui.label("End:");
                    let mut end_time =
                        (track.grid_position + track.grid_length) * (60.0 / self.state.bpm);
                    if ui
                        .add(egui::DragValue::new(&mut end_time).speed(0.1))
                        .changed()
                    {
                        let new_grid_len =
                            (end_time / (60.0 / self.state.bpm)) - track.grid_position;
                        track.grid_length = new_grid_len;
                    }

                    // File selector button
                    if ui.button("📂").clicked() {
                        if let Some(path) = FileDialog::new()
                            .add_filter("Audio", &["mp3", "wav", "ogg", "flac"])
                            .pick_file()
                        {
                            track.is_playing = false;
                            track.current_position = 0.0;
                            track.waveform_samples.clear();
                            track.audio_file = Some(path.clone());
                            track.name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Unknown")
                                .to_string();
                            track.load_waveform();
                            track.create_stream(&self.audio);
                            eprintln!("Loaded audio file: {}", path.display());
                        }
                    }

                    // Track position indicator
                    if track.audio_file.is_some() {
                        ui.label(format!(
                            "{:.1}s / {:.1}s",
                            track.current_position(),
                            track.duration
                        ));
                    }

                    ui.add_space(ui.available_width() - 100.0);

                    // Track controls
                    if ui.button(if track.muted { "🔇" } else { "M" }).clicked() {
                        track.muted = !track.muted;
                    }
                    if ui.button(if track.soloed { "S!" } else { "S" }).clicked() {
                        track.soloed = !track.soloed;
                    }
                    if ui.button(if track.recording { "⏺" } else { "R" }).clicked() {
                        track.recording = !track.recording;
                    }
                });
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Get the latest project path from config
        if let Some(path) = Self::load_config() {
            // Serialize current state
            if let Ok(serialized) = serde_json::to_string_pretty(&self.state) {
                // Save to the latest project path
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

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Monlam DAW",
        options,
        Box::new(|_cc| Box::new(DawApp::new())),
    )
}
