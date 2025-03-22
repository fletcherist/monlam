use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eframe::egui;
use egui::{Color32, Key, RichText, Stroke};
use rfd::FileDialog;
use ringbuf::HeapRb;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use symphonia::core::audio::AudioBufferRef;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;

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
        }
    }
}

impl Track {
    fn create_stream(&mut self, device: &cpal::Device, config: &cpal::StreamConfig) {
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
        let num_channels = config.channels as usize;

        match device.build_output_stream(
            config,
            {
                let audio_buffer = Arc::clone(&audio_buffer);
                let sample_index = Arc::clone(&sample_index);
                let num_channels = num_channels;
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut index = sample_index.load(Ordering::Relaxed);

                    for frame in data.chunks_mut(num_channels) {
                        for (channel, sample) in frame.iter_mut().enumerate() {
                            let sample_idx = index * num_channels + channel;
                            if let Ok(buffer) = audio_buffer.lock() {
                                *sample = buffer.get(sample_idx).copied().unwrap_or(0.0);
                            }
                        }
                        index += 1;
                    }

                    sample_index.store(index, Ordering::Relaxed);
                }
            },
            |err| eprintln!("Stream error: {}", err),
            None,
        ) {
            Ok(stream) => {
                self.stream = Some(stream);
                self.sample_index = sample_index;
                eprintln!("Created new audio stream");
            }
            Err(e) => {
                eprintln!("Failed to create audio stream: {}", e);
            }
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

            // Downsample waveform for display
            let downsample_factor = samples.len() / 1000; // Show 1000 points
            self.waveform_samples = samples
                .chunks(downsample_factor)
                .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
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
}

#[derive(Serialize, Deserialize)]
struct DawState {
    timeline_position: f32,
    #[serde(skip)]
    is_playing: bool,
    bpm: f32,
    tracks: Vec<Track>,
}

#[derive(Serialize, Deserialize)]
struct Config {
    latest_project: Option<PathBuf>,
}

struct DawApp {
    state: DawState,
    last_update: std::time::Instant,
    seek_position: Option<f32>,
    host: cpal::Host,
    output_device: cpal::Device,
    output_config: cpal::StreamConfig,
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
                // Reload audio files and recreate streams
                for track in &mut loaded_state.tracks {
                    if let Some(path) = &track.audio_file {
                        eprintln!("Loading audio file: {}", path.display());
                        track.load_waveform();
                        track.create_stream(&self.output_device, &self.output_config);
                    }
                }
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
        let host = cpal::default_host();
        let output_device = host.default_output_device().expect("No output device");
        let output_config = output_device.default_output_config().unwrap().into();

        let mut app = Self {
            state: DawState {
                timeline_position: 0.0,
                is_playing: false,
                bpm: 120.0,
                tracks: (1..=4)
                    .map(|i| Track {
                        name: format!("Track {}", i),
                        ..Default::default()
                    })
                    .collect(),
            },
            last_update: std::time::Instant::now(),
            seek_position: None,
            host,
            output_device,
            output_config,
        };

        // Try to load the latest project if available
        if let Some(path) = Self::load_config() {
            app.load_project_from_path(path);
        }

        app
    }

    fn update_playback(&mut self) {
        if self.state.is_playing {
            for track in &mut self.state.tracks {
                track.play();
            }
        } else {
            for track in &mut self.state.tracks {
                track.pause();
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
                track.seek_to(click_position);
                track.play();
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

            for track in &mut self.state.tracks {
                if track.is_playing {
                    track.current_position += delta;

                    if track.current_position >= track.duration {
                        track.current_position = 0.0;
                        track.pause();
                    }
                }
            }
            self.last_update = now;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Top toolbar
            ui.horizontal(|ui| {
                // Transport controls
                ui.horizontal(|ui| {
                    if ui.button("⏮").clicked() {
                        self.state.timeline_position = 0.0;
                        // Stop all tracks and reset positions
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
                        // Stop all tracks and seek to new position
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

                // Save/load buttons
                if ui.button("💾 Save").clicked() {
                    self.save_project();
                }
                if ui.button("📂 Open").clicked() {
                    self.load_project();
                }
            });

            ui.separator();

            // Timeline
            ui.horizontal(|ui| {
                ui.label("Timeline:");
                let timeline_response = ui.add(egui::Slider::new(
                    &mut self.state.timeline_position,
                    0.0..=100.0,
                ));
                ui.label(format!("{:.1}s", self.state.timeline_position));

                // If timeline was dragged, update track positions
                if timeline_response.changed() {
                    for track in &mut self.state.tracks {
                        track.is_playing = false;
                        track.current_position = self.state.timeline_position;
                        track.seek_to(self.state.timeline_position);
                    }
                }
            });

            ui.separator();

            // Track list
            ui.label(RichText::new("Tracks").color(Color32::WHITE));

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

                    // File selector button
                    if ui.button("📂").clicked() {
                        if let Some(path) = FileDialog::new()
                            .add_filter("Audio", &["mp3", "wav", "ogg", "flac"])
                            .pick_file()
                        {
                            // Stop current playback if any
                            track.is_playing = false;
                            track.current_position = 0.0;
                            track.waveform_samples.clear();

                            // Load new audio file
                            track.audio_file = Some(path.clone());
                            track.name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Unknown")
                                .to_string();

                            // Load audio and waveform
                            track.load_waveform();
                            track.create_stream(&self.output_device, &self.output_config);
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

                // Draw waveform if available
                if !track.waveform_samples.is_empty() {
                    let (response, painter) = ui.allocate_painter(
                        egui::vec2(ui.available_width(), 50.0),
                        egui::Sense::click(),
                    );

                    if response.rect.width() > 0.0 {
                        let rect = response.rect;
                        let width = rect.width();
                        let height = rect.height();
                        let center_y = rect.center().y;

                        // Draw waveform
                        for (i, &sample) in track.waveform_samples.iter().enumerate() {
                            let x = rect.left()
                                + (i as f32 / track.waveform_samples.len() as f32) * width;
                            let amplitude = sample * height * 0.5;
                            painter.line_segment(
                                [
                                    egui::pos2(x, center_y - amplitude),
                                    egui::pos2(x, center_y + amplitude),
                                ],
                                Stroke::new(1.0, Color32::from_rgb(100, 100, 100)),
                            );
                        }

                        // Draw playhead
                        let playhead_x =
                            rect.left() + (track.current_position / track.duration) * width;
                        painter.line_segment(
                            [
                                egui::pos2(playhead_x, rect.top()),
                                egui::pos2(playhead_x, rect.bottom()),
                            ],
                            Stroke::new(2.0, Color32::RED),
                        );

                        // Handle click to seek
                        if response.clicked() {
                            if let Some(pos) = response.interact_pointer_pos() {
                                let click_x = pos.x - rect.left();
                                let click_position = (click_x / width) * track.duration;
                                self.seek_position = Some(click_position);
                            }
                        }
                    }
                }

                // Show file info on hover
                if track.audio_file.is_some() {
                    ui.small(format!(
                        "File: {}",
                        track
                            .audio_file
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .unwrap_or("Unknown")
                    ));
                }
            }
        });
    }
}

fn load_audio(path: &Path) -> (Vec<f32>, u32) {
    let file = File::open(path).unwrap();
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension(path.extension().and_then(|s| s.to_str()).unwrap_or(""));

    let format_opts = Default::default();
    let metadata_opts = Default::default();
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .unwrap();

    let mut format = probed.format;

    let track = format.default_track().unwrap();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &Default::default())
        .unwrap();

    let sample_rate = track.codec_params.sample_rate.unwrap();
    let mut samples = Vec::new();

    while let Ok(packet) = format.next_packet() {
        let buffer = decoder.decode(&packet).unwrap();
        match buffer {
            AudioBufferRef::F32(buf) => {
                let planes_binding = buf.planes();
                let planes = planes_binding.planes();
                for i in 0..planes[0].len() {
                    for plane in planes.iter() {
                        samples.push(plane[i]);
                    }
                }
            }
            AudioBufferRef::S32(buf) => {
                let planes_binding = buf.planes();
                let planes = planes_binding.planes();
                for i in 0..planes[0].len() {
                    for plane in planes.iter() {
                        samples.push(plane[i] as f32 / i32::MAX as f32);
                    }
                }
            }
            AudioBufferRef::S16(buf) => {
                let planes_binding = buf.planes();
                let planes = planes_binding.planes();
                for i in 0..planes[0].len() {
                    for plane in planes.iter() {
                        samples.push(plane[i] as f32 / i16::MAX as f32);
                    }
                }
            }
            AudioBufferRef::U8(buf) => {
                let planes_binding = buf.planes();
                let planes = planes_binding.planes();
                for i in 0..planes[0].len() {
                    for plane in planes.iter() {
                        samples.push((plane[i] as f32 - 128.0) / 128.0);
                    }
                }
            }
            _ => panic!("Unsupported audio format"),
        }
    }

    (samples, sample_rate)
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
