use crate::daw::{DawApp, DawState};
use eframe::egui;
use egui::{Color32, Key, RichText, Stroke};
use rfd::FileDialog;

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Set dark theme with Ableton-like colors
        ctx.set_visuals(egui::Visuals::dark());

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

                if self.state.timeline_position >= track.grid_start_time
                    && self.state.timeline_position < track.grid_end_time
                {
                    let relative_position = self.state.timeline_position - track.grid_start_time;
                    if !track.is_playing {
                        track.seek_to(relative_position);
                        track.play();
                    }
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
            for track in &mut self.state.tracks {
                track.pause();
                track.current_position = 0.0;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Top toolbar with dark background
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                // Transport controls with modern styling
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("⏮").size(20.0)).clicked() {
                        self.state.timeline_position = 0.0;
                        for track in &mut self.state.tracks {
                            track.is_playing = false;
                            track.current_position = 0.0;
                            track.seek_to(0.0);
                        }
                    }
                    if ui
                        .button(
                            RichText::new(if self.state.is_playing { "⏸" } else { "▶" }).size(20.0),
                        )
                        .clicked()
                    {
                        self.state.is_playing = !self.state.is_playing;
                        self.last_update = std::time::Instant::now();
                        self.update_playback();
                    }
                    if ui.button(RichText::new("⏭").size(20.0)).clicked() {
                        self.state.timeline_position += 4.0;
                        for track in &mut self.state.tracks {
                            track.is_playing = false;
                            track.current_position = self.state.timeline_position;
                            track.seek_to(self.state.timeline_position);
                        }
                    }
                });
                ui.add_space(16.0);

                // BPM control with modern styling
                ui.label(RichText::new("BPM:").size(14.0));
                ui.add(
                    egui::DragValue::new(&mut self.state.bpm)
                        .speed(1.0)
                        .clamp_range(20.0..=240.0)
                        .prefix("")
                        .suffix(""),
                );
                ui.add_space(16.0);

                // Grid control with modern styling
                ui.label(RichText::new("Grid:").size(14.0));
                ui.add(
                    egui::DragValue::new(&mut self.state.grid_division)
                        .speed(0.25)
                        .clamp_range(0.25..=4.0)
                        .prefix("")
                        .suffix(""),
                );
                ui.add_space(16.0);

                // Save/load buttons with modern styling
                if ui.button(RichText::new("💾 Save").size(14.0)).clicked() {
                    self.save_project();
                }
                if ui.button(RichText::new("📂 Open").size(14.0)).clicked() {
                    self.load_project();
                }
                ui.add_space(8.0);
            });
            ui.add_space(8.0);
            ui.separator();

            // Timeline area with modern styling
            let timeline_height = 60.0;
            let (timeline_response, timeline_painter) = ui.allocate_painter(
                egui::vec2(ui.available_width(), timeline_height),
                egui::Sense::click_and_drag(),
            );

            // Draw timeline background
            timeline_painter.rect_filled(
                timeline_response.rect,
                0.0,
                Color32::from_rgb(40, 40, 40),
            );

            // Draw timeline grid lines
            let timeline_rect = timeline_response.rect;
            let pixels_per_beat = timeline_rect.width() / (8.0 * 4.0);

            // Draw bar lines
            for bar in 0..=8 {
                let x = timeline_rect.left() + (bar as f32 * 4.0 * pixels_per_beat);
                timeline_painter.line_segment(
                    [
                        egui::pos2(x, timeline_rect.top()),
                        egui::pos2(x, timeline_rect.bottom()),
                    ],
                    Stroke::new(2.0, Color32::from_rgb(60, 60, 60)),
                );
            }

            // Draw beat lines
            for beat in 0..=(8.0 * 4.0 * self.state.grid_division) as i32 {
                let x = timeline_rect.left()
                    + (beat as f32 * pixels_per_beat / self.state.grid_division);
                timeline_painter.line_segment(
                    [
                        egui::pos2(x, timeline_rect.top()),
                        egui::pos2(x, timeline_rect.bottom()),
                    ],
                    Stroke::new(1.0, Color32::from_rgb(50, 50, 50)),
                );
            }

            // Draw playhead
            let playhead_x =
                timeline_rect.left() + (self.state.timeline_position * pixels_per_beat);
            timeline_painter.line_segment(
                [
                    egui::pos2(playhead_x, timeline_rect.top()),
                    egui::pos2(playhead_x, timeline_rect.bottom()),
                ],
                Stroke::new(2.0, Color32::from_rgb(255, 50, 50)),
            );

            // Draw time markers
            for bar in 0..=8 {
                let x = timeline_rect.left() + (bar as f32 * 4.0 * pixels_per_beat);
                let time = (bar as f32 * 4.0 * 60.0) / self.state.bpm;
                let time_text = format!("{:.1}s", time);
                timeline_painter.text(
                    egui::pos2(x + 2.0, timeline_rect.top() + 15.0),
                    egui::Align2::LEFT_TOP,
                    time_text,
                    egui::FontId::proportional(12.0),
                    Color32::from_rgb(200, 200, 200),
                );
            }

            // Handle timeline interaction
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

            // Main grid area
            let available_height = ui.available_height() - 150.0 - timeline_height;
            let (grid_response, painter) = ui.allocate_painter(
                egui::vec2(ui.available_width(), available_height),
                egui::Sense::click_and_drag(),
            );

            if grid_response.rect.width() > 0.0 {
                let rect = grid_response.rect;
                let width = rect.width();
                let height = rect.height();

                // Draw grid background
                painter.rect_filled(rect, 0.0, Color32::from_rgb(30, 30, 30));

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
                        Stroke::new(2.0, Color32::from_rgb(60, 60, 60)),
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

                // Draw tracks
                let track_height = 100.0;
                let track_spacing = 8.0;
                let total_tracks_height = (self.state.tracks.len() as f32
                    * (track_height + track_spacing))
                    + track_spacing;

                for (index, track) in self.state.tracks.iter_mut().enumerate() {
                    let track_y = rect.top()
                        + track_spacing
                        + (index as f32 * (track_height + track_spacing));
                    let track_x = rect.left()
                        + (track.grid_position * (60.0 / self.state.bpm) * pixels_per_beat);
                    let track_width = track.grid_length * (60.0 / self.state.bpm) * pixels_per_beat;

                    // Draw track background
                    let track_rect = egui::Rect::from_min_size(
                        egui::pos2(track_x, track_y),
                        egui::vec2(track_width, track_height),
                    );

                    // Track background color based on state
                    let bg_color = if track.muted {
                        Color32::from_rgb(40, 40, 40)
                    } else if track.soloed {
                        Color32::from_rgb(40, 40, 60)
                    } else {
                        Color32::from_rgb(35, 35, 35)
                    };
                    painter.rect_filled(track_rect, 4.0, bg_color);

                    // Draw track border
                    painter.rect_stroke(
                        track_rect,
                        4.0,
                        Stroke::new(1.0, Color32::from_rgb(60, 60, 60)),
                    );

                    // Draw track name
                    painter.text(
                        egui::pos2(track_x + 8.0, track_y + 20.0),
                        egui::Align2::LEFT_TOP,
                        &track.name,
                        egui::FontId::proportional(14.0),
                        Color32::from_rgb(200, 200, 200),
                    );

                    // Draw waveform
                    if !track.waveform_samples.is_empty() {
                        let waveform_height = track_height - 40.0;
                        let waveform_y = track_y + 30.0;
                        let waveform_width = track_width - 16.0;
                        let waveform_x = track_x + 8.0;

                        for (i, &sample) in track.waveform_samples.iter().enumerate() {
                            let x = waveform_x
                                + (i as f32 / track.waveform_samples.len() as f32) * waveform_width;
                            let amplitude = sample * waveform_height * 0.8;
                            painter.line_segment(
                                [
                                    egui::pos2(x, waveform_y + waveform_height / 2.0 - amplitude),
                                    egui::pos2(x, waveform_y + waveform_height / 2.0 + amplitude),
                                ],
                                Stroke::new(1.0, Color32::from_rgb(100, 100, 100)),
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
                    Stroke::new(2.0, Color32::from_rgb(255, 50, 50)),
                );
            }

            // Track controls at the bottom
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            for (_index, track) in self.state.tracks.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    // Track name with color based on whether it has an audio file
                    ui.label(RichText::new(&track.name).size(14.0).color(
                        if track.audio_file.is_some() {
                            Color32::from_rgb(100, 255, 100)
                        } else {
                            Color32::from_rgb(200, 200, 200)
                        },
                    ));

                    // Start and End time controls
                    ui.label(RichText::new("Start:").size(14.0));
                    let mut start_time = track.grid_position * (60.0 / self.state.bpm);
                    if ui
                        .add(egui::DragValue::new(&mut start_time).speed(0.1))
                        .changed()
                    {
                        let new_grid_pos = start_time / (60.0 / self.state.bpm);
                        track.grid_position = new_grid_pos;
                    }

                    ui.label(RichText::new("End:").size(14.0));
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
                    if ui.button(RichText::new("📂").size(14.0)).clicked() {
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
                        ui.label(
                            RichText::new(format!(
                                "{:.1}s / {:.1}s",
                                track.current_position(),
                                track.duration
                            ))
                            .size(14.0),
                        );
                    }

                    ui.add_space(ui.available_width() - 100.0);

                    // Track controls with modern styling
                    if ui
                        .button(RichText::new(if track.muted { "🔇" } else { "M" }).size(14.0))
                        .clicked()
                    {
                        track.muted = !track.muted;
                    }
                    if ui
                        .button(RichText::new(if track.soloed { "S!" } else { "S" }).size(14.0))
                        .clicked()
                    {
                        track.soloed = !track.soloed;
                    }
                    if ui
                        .button(RichText::new(if track.recording { "⏺" } else { "R" }).size(14.0))
                        .clicked()
                    {
                        track.recording = !track.recording;
                    }
                    ui.add_space(8.0);
                });
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.on_exit();
    }
}
