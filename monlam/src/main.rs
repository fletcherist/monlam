use eframe::egui;
use egui::{Color32, Key, RichText, Stroke};
use rfd::FileDialog;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

const BUFFER_SIZE: usize = 1024;
const SAMPLE_RATE: u32 = 44100;

#[derive(Default)]
struct Track {
    name: String,
    audio_file: Option<PathBuf>,
    muted: bool,
    soloed: bool,
    recording: bool,
    sink: Option<Sink>,
    is_playing: bool,
    duration: f32,
    current_position: f32,
    waveform_samples: Vec<f32>,
}

impl Track {
    fn seek_to(&mut self, position: f32, stream_handle: &OutputStreamHandle) {
        if let Some(path) = &self.audio_file {
            if let Ok(file) = File::open(path) {
                let reader = BufReader::new(file);
                if let Ok(decoder) = Decoder::new(reader) {
                    if let Ok(sink) = Sink::try_new(stream_handle) {
                        sink.append(decoder);
                        sink.pause();
                        self.sink = Some(sink);
                        self.current_position = position;
                    }
                }
            }
        }
    }

    fn load_waveform(&mut self) {
        if let Some(path) = &self.audio_file {
            if let Ok(file) = File::open(path) {
                let reader = BufReader::new(file);
                if let Ok(decoder) = Decoder::new(reader) {
                    // Get duration from decoder
                    self.duration = decoder
                        .total_duration()
                        .map(|d| d.as_secs_f32())
                        .unwrap_or(0.0);

                    // Convert audio to mono samples
                    let samples: Vec<f32> = decoder.convert_samples::<f32>().collect();

                    // Downsample to 1000 points for visualization
                    let downsample_factor = samples.len() / 1000;
                    self.waveform_samples = samples
                        .chunks(downsample_factor)
                        .map(|chunk| {
                            chunk.iter().map(|&x| x.abs()).sum::<f32>() / chunk.len() as f32
                        })
                        .collect();

                    // Initialize sink for playback
                    if let Ok(sink) = Sink::try_new(&OutputStream::try_default().unwrap().1) {
                        self.sink = Some(sink);
                    }
                }
            }
        }
    }
}

struct DawApp {
    timeline_position: f32,
    is_playing: bool,
    bpm: f32,
    tracks: Vec<Track>,
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    last_update: std::time::Instant,
    seek_position: Option<f32>,
}

impl Default for DawApp {
    fn default() -> Self {
        let (_stream, stream_handle) = OutputStream::try_default().unwrap();
        Self {
            timeline_position: 0.0,
            is_playing: false,
            bpm: 120.0,
            tracks: (1..=4)
                .map(|i| Track {
                    name: format!("Track {}", i),
                    ..Default::default()
                })
                .collect(),
            _stream,
            stream_handle,
            last_update: std::time::Instant::now(),
            seek_position: None,
        }
    }
}

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Force continuous repaints at 60 FPS
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(1.0 / 60.0));

        // Handle seek position if set
        if let Some(click_position) = self.seek_position.take() {
            self.timeline_position = click_position;
            for track in &mut self.tracks {
                track.seek_to(click_position, &self.stream_handle);
                if self.is_playing {
                    if let Some(sink) = &track.sink {
                        sink.play();
                    }
                    track.is_playing = true;
                }
            }
        }

        // Handle spacebar input
        if ctx.input(|i| i.key_pressed(Key::Space)) {
            self.is_playing = !self.is_playing;
            self.last_update = std::time::Instant::now();

            for track in &mut self.tracks {
                if let Some(sink) = &track.sink {
                    if self.is_playing {
                        sink.play();
                    } else {
                        sink.pause();
                    }
                }
                track.is_playing = self.is_playing;
            }
        }

        // Update timeline position based on playback
        if self.is_playing {
            let now = std::time::Instant::now();
            let delta = now.duration_since(self.last_update).as_secs_f32();
            self.timeline_position += delta;
            self.last_update = now;

            // Update track positions
            for track in &mut self.tracks {
                if track.is_playing {
                    track.current_position += delta;
                    if track.current_position >= track.duration {
                        track.current_position = 0.0;
                        if let Some(sink) = &track.sink {
                            sink.stop();
                        }
                        track.is_playing = false;
                    }
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Top toolbar
            ui.horizontal(|ui| {
                // Transport controls
                ui.horizontal(|ui| {
                    if ui.button("⏮").clicked() {
                        self.timeline_position = 0.0;
                        // Stop all tracks and reset positions
                        for track in &mut self.tracks {
                            track.is_playing = false;
                            track.current_position = 0.0;
                            track.seek_to(0.0, &self.stream_handle);
                        }
                    }
                    if ui.button(if self.is_playing { "⏸" } else { "▶" }).clicked() {
                        self.is_playing = !self.is_playing;
                        self.last_update = std::time::Instant::now();
                        // Toggle playback for all tracks
                        for track in &mut self.tracks {
                            track.is_playing = self.is_playing;
                        }
                    }
                    if ui.button("⏭").clicked() {
                        self.timeline_position += 4.0;
                        // Stop all tracks and seek to new position
                        for track in &mut self.tracks {
                            track.is_playing = false;
                            track.current_position = self.timeline_position;
                            track.seek_to(self.timeline_position, &self.stream_handle);
                        }
                    }
                });

                // BPM control
                ui.label("BPM:");
                ui.add(
                    egui::DragValue::new(&mut self.bpm)
                        .speed(1.0)
                        .clamp_range(20.0..=240.0),
                );
            });

            ui.separator();

            // Timeline
            ui.horizontal(|ui| {
                ui.label("Timeline:");
                let timeline_response =
                    ui.add(egui::Slider::new(&mut self.timeline_position, 0.0..=100.0));
                ui.label(format!("{:.1}s", self.timeline_position));

                // If timeline was dragged, update track positions
                if timeline_response.changed() {
                    for track in &mut self.tracks {
                        track.is_playing = false;
                        track.current_position = self.timeline_position;
                        track.seek_to(self.timeline_position, &self.stream_handle);
                    }
                }
            });

            ui.separator();

            // Track list
            ui.label(RichText::new("Tracks").color(Color32::WHITE));

            for (_index, track) in self.tracks.iter_mut().enumerate() {
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
                        }
                    }

                    // Track position indicator
                    if track.audio_file.is_some() {
                        ui.label(format!(
                            "{:.1}s / {:.1}s",
                            track.current_position, track.duration
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

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Monlam DAW",
        options,
        Box::new(|_cc| Box::new(DawApp::default())),
    )
}
