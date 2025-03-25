use crate::daw::{DawAction, DawApp};
use eframe::egui;
use egui::{Color32, Key, RichText, Stroke};
use rfd::FileDialog;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::channel;

// UI Constants
const TIMELINE_HEIGHT: f32 = 60.0;
const TRACK_HEIGHT: f32 = 100.0;
const TRACK_SPACING: f32 = 8.0;
const GRID_BACKGROUND: Color32 = Color32::from_rgb(30, 30, 30);
const BAR_LINE_COLOR: Color32 = Color32::from_rgb(60, 60, 60);
const BEAT_LINE_COLOR: Color32 = Color32::from_rgb(50, 50, 50);
const PLAYHEAD_COLOR: Color32 = Color32::from_rgb(255, 50, 50);
const TRACK_BORDER_COLOR: Color32 = Color32::from_rgb(60, 60, 60);
const TRACK_TEXT_COLOR: Color32 = Color32::from_rgb(200, 200, 200);
const WAVEFORM_COLOR: Color32 = Color32::from_rgb(100, 100, 100);

// UI Components
struct TransportControls<'a> {
    is_playing: bool,
    bpm: f32,
    grid_division: f32,
    on_rewind: &'a mut dyn FnMut(),
    on_play_pause: &'a mut dyn FnMut(),
    on_forward: &'a mut dyn FnMut(),
    on_bpm_change: &'a mut dyn FnMut(f32),
    on_grid_change: &'a mut dyn FnMut(f32),
    on_save: &'a mut dyn FnMut(),
    on_load: &'a mut dyn FnMut(),
}

impl<'a> TransportControls<'a> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);

            // Rewind button
            if ui
                .button(RichText::new("⏮").size(20.0))
                .on_hover_text("Rewind")
                .clicked()
            {
                (self.on_rewind)();
            }

            // Play/Pause button
            if ui
                .button(RichText::new(if self.is_playing { "⏸" } else { "▶" }).size(20.0))
                .on_hover_text(if self.is_playing { "Pause" } else { "Play" })
                .clicked()
            {
                (self.on_play_pause)();
            }

            // Forward button
            if ui
                .button(RichText::new("⏭").size(20.0))
                .on_hover_text("Forward")
                .clicked()
            {
                (self.on_forward)();
            }

            ui.add_space(16.0);

            // BPM control
            ui.label(RichText::new("BPM:").size(14.0));
            let mut bpm = self.bpm;
            ui.add(egui::Slider::new(&mut bpm, 30.0..=240.0).step_by(1.0));
            if bpm != self.bpm {
                (self.on_bpm_change)(bpm);
            }

            ui.add_space(16.0);

            // Grid division control
            ui.label(RichText::new("Grid:").size(14.0));
            let divisions = ["1/4", "1/8", "1/16", "1/32"];
            let values = [0.25, 0.125, 0.0625, 0.03125];
            let mut selected = 0;
            for (i, &value) in values.iter().enumerate() {
                if (self.grid_division - value).abs() < 0.001 {
                    selected = i;
                    break;
                }
            }
            egui::ComboBox::from_label("")
                .selected_text(divisions[selected])
                .show_ui(ui, |ui| {
                    for (i, &div) in divisions.iter().enumerate() {
                        if ui.selectable_value(&mut selected, i, div).clicked() && selected != i {
                            (self.on_grid_change)(values[i]);
                        }
                    }
                });

            ui.add_space(16.0);

            // Save and Load buttons
            if ui
                .button(RichText::new("💾").size(20.0))
                .on_hover_text("Save Project")
                .clicked()
            {
                (self.on_save)();
            }
            if ui
                .button(RichText::new("📂").size(20.0))
                .on_hover_text("Load Project")
                .clicked()
            {
                (self.on_load)();
            }
        });
    }
}

struct Timeline<'a> {
    is_playing: bool,
    timeline_position: f32,
    bpm: f32,
    grid_division: f32,
    on_timeline_click: &'a mut dyn FnMut(f32),
    last_clicked_bar: f32,
}

impl<'a> Timeline<'a> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let available_width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(
            egui::Vec2::new(available_width, TIMELINE_HEIGHT),
            egui::Sense::click_and_drag(),
        );

        // Calculate time scale (in seconds per pixel)
        let pixels_per_beat = 50.0;
        let beats_per_second = self.bpm / 60.0;
        let seconds_per_pixel = 1.0 / (pixels_per_beat * beats_per_second);

        if let Some(pos) = response.interact_pointer_pos() {
            let timeline_position = (pos.x - rect.left()) * seconds_per_pixel;
            (self.on_timeline_click)(timeline_position);
        }

        let painter = ui.painter_at(rect);

        // Draw timeline background
        painter.rect_filled(rect, 0.0, GRID_BACKGROUND);

        // Draw grid lines
        let num_visible_seconds = available_width * seconds_per_pixel;
        let num_visible_beats = num_visible_seconds * beats_per_second;
        let beat_width = pixels_per_beat;

        for beat in 0..=(num_visible_beats.ceil() as i32) {
            let x = rect.left() + beat as f32 * beat_width;
            let color = if beat % 4 == 0 {
                BAR_LINE_COLOR
            } else {
                BEAT_LINE_COLOR
            };
            painter.line_segment(
                [
                    egui::Pos2::new(x, rect.top()),
                    egui::Pos2::new(x, rect.bottom()),
                ],
                Stroke::new(1.0, color),
            );

            // Draw beat number for every bar (4 beats)
            if beat % 4 == 0 {
                painter.text(
                    egui::Pos2::new(x + 4.0, rect.top() + 12.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}", beat / 4 + 1),
                    egui::FontId::proportional(10.0),
                    Color32::from_rgb(150, 150, 150),
                );
            }
        }

        // Draw playhead
        let playhead_x = rect.left() + self.timeline_position / seconds_per_pixel;
        painter.line_segment(
            [
                egui::Pos2::new(playhead_x, rect.top()),
                egui::Pos2::new(playhead_x, rect.bottom()),
            ],
            Stroke::new(2.0, PLAYHEAD_COLOR),
        );

        // Draw last clicked bar marker if set
        if self.last_clicked_bar > 0.0 {
            let last_clicked_x = rect.left() + self.last_clicked_bar / seconds_per_pixel;
            painter.line_segment(
                [
                    egui::Pos2::new(last_clicked_x, rect.top()),
                    egui::Pos2::new(last_clicked_x, rect.bottom()),
                ],
                Stroke::new(2.0, Color32::from_rgb(0, 200, 0)),
            );
        }
    }
}

struct Grid<'a> {
    timeline_position: f32,
    bpm: f32,
    grid_division: f32,
    tracks: Vec<(
        usize,
        String,
        bool,
        bool,
        bool,
        Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32)>,
    )>, // Track ID, Name, muted, soloed, recording, samples: (Sample ID, name, position, length, waveform, sample_rate, duration, audio_start_time, audio_end_time)
    on_track_drag: &'a mut dyn FnMut(usize, usize, f32), // track_id, sample_id, new_position
}

impl<'a> Grid<'a> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let available_width = ui.available_width();
        let grid_height = TRACK_HEIGHT * self.tracks.len() as f32
            + TRACK_SPACING * (self.tracks.len() as f32 - 1.0);
        let (rect, _response) = ui.allocate_exact_size(
            egui::Vec2::new(available_width, grid_height),
            egui::Sense::click_and_drag(),
        );

        // Calculate time scale (in seconds per pixel)
        let pixels_per_beat = 50.0;
        let beats_per_second = self.bpm / 60.0;
        let seconds_per_pixel = 1.0 / (pixels_per_beat * beats_per_second);

        let painter = ui.painter_at(rect);

        // Draw grid background
        painter.rect_filled(rect, 0.0, GRID_BACKGROUND);

        // Draw grid lines
        let num_visible_seconds = available_width * seconds_per_pixel;
        let num_visible_beats = num_visible_seconds * beats_per_second;
        let beat_width = pixels_per_beat;

        for beat in 0..=(num_visible_beats.ceil() as i32) {
            let x = rect.left() + beat as f32 * beat_width;
            let color = if beat % 4 == 0 {
                BAR_LINE_COLOR
            } else {
                BEAT_LINE_COLOR
            };
            painter.line_segment(
                [
                    egui::Pos2::new(x, rect.top()),
                    egui::Pos2::new(x, rect.bottom()),
                ],
                Stroke::new(1.0, color),
            );
        }

        // Draw tracks and samples
        for (i, (track_id, name, muted, soloed, recording, samples)) in
            self.tracks.iter().enumerate()
        {
            let track_top = rect.top() + i as f32 * (TRACK_HEIGHT + TRACK_SPACING);
            let track_rect = egui::Rect::from_min_size(
                egui::Pos2::new(rect.left(), track_top),
                egui::Vec2::new(rect.width(), TRACK_HEIGHT),
            );

            // Draw track background
            let track_color = if *muted {
                Color32::from_rgb(40, 40, 40)
            } else if *soloed {
                Color32::from_rgb(40, 40, 50)
            } else if *recording {
                Color32::from_rgb(50, 30, 30)
            } else {
                Color32::from_rgb(40, 40, 40)
            };
            painter.rect_filled(track_rect, 4.0, track_color);
            painter.rect_stroke(track_rect, 4.0, Stroke::new(1.0, TRACK_BORDER_COLOR));

            // Draw track name
            painter.text(
                egui::Pos2::new(track_rect.left() + 8.0, track_rect.top() + 16.0),
                egui::Align2::LEFT_TOP,
                name,
                egui::FontId::proportional(14.0),
                TRACK_TEXT_COLOR,
            );

            // Draw each sample in the track
            for (
                sample_index,
                (
                    sample_id,
                    sample_name,
                    position,
                    length,
                    waveform,
                    sample_rate,
                    duration,
                    audio_start_time,
                    audio_end_time,
                ),
            ) in samples.iter().enumerate()
            {
                if *length > 0.0 {
                    let region_left = rect.left() + *position * pixels_per_beat;
                    let region_width = *length * pixels_per_beat;
                    let region_rect = egui::Rect::from_min_size(
                        egui::Pos2::new(region_left, track_rect.top() + 25.0),
                        egui::Vec2::new(region_width, TRACK_HEIGHT - 30.0),
                    );

                    // Draw sample background with alternating colors for better visibility
                    let sample_color = if sample_index % 2 == 0 {
                        Color32::from_rgb(60, 60, 70)
                    } else {
                        Color32::from_rgb(70, 70, 80)
                    };

                    painter.rect_filled(region_rect, 4.0, sample_color);
                    painter.rect_stroke(
                        region_rect,
                        4.0,
                        Stroke::new(1.0, Color32::from_rgb(80, 80, 90)),
                    );

                    // Show sample name
                    painter.text(
                        egui::Pos2::new(region_rect.left() + 4.0, region_rect.top() + 12.0),
                        egui::Align2::LEFT_TOP,
                        sample_name,
                        egui::FontId::proportional(10.0),
                        TRACK_TEXT_COLOR,
                    );

                    // Draw waveform
                    if !waveform.is_empty() && *duration > 0.0 {
                        let waveform_length = waveform.len();

                        // Since the grid_length is now based on the trimmed duration,
                        // the region rect already represents just the trimmed portion.
                        // We need to map waveform positions correctly to this trimmed display.

                        // Calculate what portion of the original waveform we're showing
                        let trim_start_ratio = *audio_start_time / *duration;
                        let trim_end_ratio = *audio_end_time / *duration;

                        // Draw the waveform to fit the entire region
                        for x in 0..region_width as usize {
                            // Map pixel position to position within trimmed waveform
                            let position_in_trim = x as f32 / region_width;

                            // Map back to position in full waveform
                            let full_waveform_pos = trim_start_ratio
                                + position_in_trim * (trim_end_ratio - trim_start_ratio);

                            // Get index in the waveform data
                            let sample_index =
                                (full_waveform_pos * waveform_length as f32) as usize;

                            if sample_index < waveform_length {
                                let amplitude = waveform[sample_index];

                                // Scale the amplitude for visualization
                                let y_offset = amplitude * (region_rect.height() / 2.5);
                                let center_y = region_rect.center().y;
                                let x_pos = region_rect.left() + x as f32;

                                // Draw the waveform segment
                                painter.line_segment(
                                    [
                                        egui::Pos2::new(x_pos, center_y - y_offset),
                                        egui::Pos2::new(x_pos, center_y + y_offset),
                                    ],
                                    Stroke::new(1.0, WAVEFORM_COLOR),
                                );
                            }
                        }
                    }

                    // Handle audio region dragging
                    let id = ui
                        .id()
                        .with(format!("track_{}_sample_{}", track_id, sample_id));
                    let region_response = ui.interact(region_rect, id, egui::Sense::drag());

                    if region_response.dragged() {
                        let delta = region_response.drag_delta().x;
                        let grid_delta = delta / pixels_per_beat;
                        let new_position = *position + grid_delta;
                        (self.on_track_drag)(*track_id, *sample_id, new_position);
                    }
                }
            }
        }

        // Draw playhead
        let playhead_x = rect.left() + self.timeline_position / seconds_per_pixel;
        painter.line_segment(
            [
                egui::Pos2::new(playhead_x, rect.top()),
                egui::Pos2::new(playhead_x, rect.bottom()),
            ],
            Stroke::new(2.0, PLAYHEAD_COLOR),
        );
    }
}

struct TrackControls<'a> {
    tracks: Vec<(
        usize,
        String,
        bool,
        bool,
        bool,
        Vec<(usize, String, f32, f32, f32, f32, f32, f32)>,
    )>, // Track ID, Name, muted, soloed, recording, samples: (Sample ID, name, position, length, current position, duration, trim_start, trim_end)
    on_sample_start_change: &'a mut dyn FnMut(usize, usize, f32), // track_id, sample_id, position
    on_sample_length_change: &'a mut dyn FnMut(usize, usize, f32), // track_id, sample_id, length
    on_track_file_select: &'a mut dyn FnMut(usize),               // track_id
    on_track_mute: &'a mut dyn FnMut(usize),                      // track_id
    on_track_solo: &'a mut dyn FnMut(usize),                      // track_id
    on_track_record: &'a mut dyn FnMut(usize),                    // track_id
    on_sample_trim_change: &'a mut dyn FnMut(usize, usize, f32, f32), // track_id, sample_id, trim_start, trim_end
}

impl<'a> TrackControls<'a> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        for (track_id, name, muted, soloed, recording, samples) in &self.tracks {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("{}:", name)).size(14.0));

                ui.add_space(8.0);
                if ui.button(RichText::new("Add Sample").size(14.0)).clicked() {
                    (self.on_track_file_select)(*track_id);
                }

                ui.add_space(ui.available_width() - 100.0);

                // Track controls
                if ui
                    .button(RichText::new(if *muted { "🔇" } else { "M" }).size(14.0))
                    .clicked()
                {
                    (self.on_track_mute)(*track_id);
                }
                if ui
                    .button(RichText::new(if *soloed { "S!" } else { "S" }).size(14.0))
                    .clicked()
                {
                    (self.on_track_solo)(*track_id);
                }
                if ui
                    .button(RichText::new(if *recording { "⏺" } else { "R" }).size(14.0))
                    .clicked()
                {
                    (self.on_track_record)(*track_id);
                }
                ui.add_space(8.0);
            });

            // Sample controls (indented)
            for (
                sample_id,
                sample_name,
                position,
                length,
                current_position,
                duration,
                trim_start,
                trim_end,
            ) in samples
            {
                ui.horizontal(|ui| {
                    ui.add_space(20.0); // Indent
                    ui.label(RichText::new(format!("Sample: {}", sample_name)).size(12.0));

                    ui.add_space(8.0);
                    ui.label(RichText::new("Start:").size(12.0));
                    let mut pos = *position;
                    ui.add(egui::DragValue::new(&mut pos).speed(0.1));
                    if pos != *position {
                        (self.on_sample_start_change)(*track_id, *sample_id, pos);
                    }

                    ui.add_space(8.0);
                    ui.label(RichText::new("Length:").size(12.0));
                    let mut len = *length;
                    ui.add(
                        egui::DragValue::new(&mut len)
                            .speed(0.1)
                            .clamp_range(0.1..=100.0),
                    );
                    if len != *length {
                        (self.on_sample_length_change)(*track_id, *sample_id, len);
                    }

                    if *duration > 0.0 {
                        ui.label(
                            RichText::new(format!("{:.1}s / {:.1}s", current_position, duration))
                                .size(12.0),
                        );
                    }
                });

                // Add trim controls in a separate row
                ui.horizontal(|ui| {
                    ui.add_space(30.0); // More indent
                    ui.label(RichText::new("Trim Start:").size(12.0));
                    let mut start = *trim_start;
                    ui.add(
                        egui::DragValue::new(&mut start)
                            .speed(0.1)
                            .suffix(" s")
                            .clamp_range(0.0..=*duration),
                    );

                    ui.add_space(8.0);
                    ui.label(RichText::new("Trim End:").size(12.0));
                    let mut end = if *trim_end <= 0.0 {
                        *duration
                    } else {
                        *trim_end
                    };
                    ui.add(
                        egui::DragValue::new(&mut end)
                            .speed(0.1)
                            .suffix(" s")
                            .clamp_range(start..=*duration),
                    );

                    // Only trigger the callback if values actually changed
                    if start != *trim_start
                        || end
                            != (if *trim_end <= 0.0 {
                                *duration
                            } else {
                                *trim_end
                            })
                    {
                        (self.on_sample_trim_change)(*track_id, *sample_id, start, end);
                    }
                });
            }

            ui.add_space(4.0);
            ui.separator();
        }
    }
}

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Set dark theme
        ctx.set_visuals(egui::Visuals::dark());

        // Force continuous repaints at 60 FPS
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(1.0 / 60.0));

        // Handle seek position if set
        if let Some(click_position) = self.seek_position.take() {
            self.dispatch(DawAction::SetTimelinePosition(click_position));
        }

        // Handle spacebar input
        if ctx.input(|i| i.key_pressed(Key::Space)) {
            self.dispatch(DawAction::TogglePlayback);
        }

        // Update timeline position based on audio playback
        self.update_playback();

        // Store state values locally to use in UI closures
        let is_playing = self.state.is_playing;
        let timeline_position = self.state.timeline_position;
        let bpm = self.state.bpm;
        let grid_division = self.state.grid_division;
        let last_clicked_bar = self.state.last_clicked_bar;
        let drag_offset = self.state.drag_offset;

        // Create track info to avoid borrowing self in closures
        let track_info: Vec<_> = self
            .state
            .tracks
            .iter()
            .map(|track| {
                let samples_info: Vec<_> = track
                    .samples
                    .iter()
                    .map(|sample| {
                        let (waveform_data, sample_rate) = sample
                            .waveform
                            .as_ref()
                            .map(|w| (w.samples.clone(), w.sample_rate))
                            .unwrap_or((vec![], 44100));

                        // Get the full audio duration
                        let full_duration =
                            sample.waveform.as_ref().map(|w| w.duration).unwrap_or(0.0);

                        // Use trim points for the visual representation
                        let audio_start_time = sample.trim_start;
                        let audio_end_time = if sample.trim_end <= 0.0 {
                            full_duration
                        } else {
                            sample.trim_end
                        };

                        (
                            sample.id,
                            sample.name.clone(),
                            sample.grid_position,
                            sample.grid_length,
                            waveform_data,
                            sample_rate,
                            full_duration,
                            audio_start_time,
                            audio_end_time,
                        )
                    })
                    .collect();

                (
                    track.id,
                    track.name.clone(),
                    track.muted,
                    track.soloed,
                    track.recording,
                    samples_info,
                )
            })
            .collect();

        let track_controls_info: Vec<_> = self
            .state
            .tracks
            .iter()
            .map(|track| {
                let samples_info: Vec<_> = track
                    .samples
                    .iter()
                    .map(|sample| {
                        (
                            sample.id,
                            sample.name.clone(),
                            sample.grid_position,
                            sample.grid_length,
                            sample.current_position,
                            sample.waveform.as_ref().map_or(0.0, |w| w.duration),
                            sample.trim_start,
                            sample.trim_end,
                        )
                    })
                    .collect();

                (
                    track.id,
                    track.name.clone(),
                    track.muted,
                    track.soloed,
                    track.recording,
                    samples_info,
                )
            })
            .collect();

        // Define UI actions without capturing self
        #[derive(Clone)]
        enum UiAction {
            Rewind,
            TogglePlayback,
            Forward,
            SetBpm(f32),
            SetGridDivision(f32),
            SaveProject,
            LoadProject,
            SetTimelinePosition(f32),
            TrackDrag {
                track_id: usize,
                sample_id: usize,
                position: f32,
            },
            SetSamplePosition {
                track_id: usize,
                sample_id: usize,
                position: f32,
            },
            SetSampleLength {
                track_id: usize,
                sample_id: usize,
                length: f32,
            },
            LoadTrackAudio(usize),
            ToggleTrackMute(usize),
            ToggleTrackSolo(usize),
            ToggleTrackRecord(usize),
            SetSampleTrim {
                track_id: usize,
                sample_id: usize,
                trim_start: f32,
                trim_end: f32,
            },
        }

        // Collect actions during UI rendering using Rc<RefCell>
        let actions = Rc::new(RefCell::new(Vec::new()));

        // Helper function to snap to grid without borrowing self
        let grid_division_value = grid_division;
        let snap_to_grid = move |position: f32| -> f32 {
            // Calculate the nearest grid line
            let grid_lines = position / grid_division_value;
            let lower_grid_line = grid_lines.floor();
            let upper_grid_line = grid_lines.ceil();

            // Determine whether to snap to the lower or upper grid line
            if position - (lower_grid_line * grid_division_value)
                < (upper_grid_line * grid_division_value) - position
            {
                lower_grid_line * grid_division_value
            } else {
                upper_grid_line * grid_division_value
            }
        };

        // Add the top toolbar with transport controls
        egui::TopBottomPanel::top("transport_controls").show(ctx, |ui| {
            let actions_clone = actions.clone();

            TransportControls {
                is_playing,
                bpm,
                grid_division,
                on_rewind: &mut || {
                    actions_clone.borrow_mut().push(UiAction::Rewind);
                },
                on_play_pause: &mut || {
                    actions_clone.borrow_mut().push(UiAction::TogglePlayback);
                },
                on_forward: &mut || {
                    actions_clone.borrow_mut().push(UiAction::Forward);
                },
                on_bpm_change: &mut |new_bpm| {
                    actions_clone.borrow_mut().push(UiAction::SetBpm(new_bpm));
                },
                on_grid_change: &mut |new_grid| {
                    actions_clone
                        .borrow_mut()
                        .push(UiAction::SetGridDivision(new_grid));
                },
                on_save: &mut || {
                    actions_clone.borrow_mut().push(UiAction::SaveProject);
                },
                on_load: &mut || {
                    actions_clone.borrow_mut().push(UiAction::LoadProject);
                },
            }
            .draw(ui);
        });

        // Complete the UI with the central grid and track panels
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                // Timeline controls at the top
                let actions_clone = actions.clone();
                Timeline {
                    is_playing,
                    timeline_position,
                    bpm,
                    grid_division,
                    last_clicked_bar,
                    on_timeline_click: &mut |position| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::SetTimelinePosition(position));
                    },
                }
                .draw(ui);

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Grid in the middle
                let actions_clone = actions.clone();
                Grid {
                    timeline_position,
                    bpm,
                    grid_division,
                    tracks: track_info,
                    on_track_drag: &mut |track_id, sample_id, position| {
                        actions_clone.borrow_mut().push(UiAction::TrackDrag {
                            track_id,
                            sample_id,
                            position,
                        });
                    },
                }
                .draw(ui);

                // Track controls
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                let actions_clone = actions.clone();
                TrackControls {
                    tracks: track_controls_info,
                    on_sample_start_change: &mut |track_id, sample_id, position| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::SetSamplePosition {
                                track_id,
                                sample_id,
                                position,
                            });
                    },
                    on_sample_length_change: &mut |track_id, sample_id, length| {
                        actions_clone.borrow_mut().push(UiAction::SetSampleLength {
                            track_id,
                            sample_id,
                            length,
                        });
                    },
                    on_track_file_select: &mut |track_id| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::LoadTrackAudio(track_id));
                    },
                    on_track_mute: &mut |track_id| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::ToggleTrackMute(track_id));
                    },
                    on_track_solo: &mut |track_id| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::ToggleTrackSolo(track_id));
                    },
                    on_track_record: &mut |track_id| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::ToggleTrackRecord(track_id));
                    },
                    on_sample_trim_change: &mut |track_id, sample_id, trim_start, trim_end| {
                        actions_clone.borrow_mut().push(UiAction::SetSampleTrim {
                            track_id,
                            sample_id,
                            trim_start,
                            trim_end,
                        });
                    },
                }
                .draw(ui);
            });
        });

        // Process the collected actions
        for action in actions.borrow().iter() {
            match action {
                UiAction::Rewind => {
                    self.dispatch(DawAction::RewindTimeline);
                }
                UiAction::TogglePlayback => {
                    self.dispatch(DawAction::TogglePlayback);
                }
                UiAction::Forward => {
                    self.dispatch(DawAction::ForwardTimeline(1.0));
                }
                UiAction::SetBpm(bpm) => {
                    self.dispatch(DawAction::SetBpm(*bpm));
                }
                UiAction::SetGridDivision(division) => {
                    self.dispatch(DawAction::SetGridDivision(*division));
                }
                UiAction::SaveProject => {
                    self.save_project();
                }
                UiAction::LoadProject => {
                    self.load_project();
                }
                UiAction::SetTimelinePosition(pos) => {
                    // Snap to grid if close enough
                    let beat_pos = self.time_to_beat(*pos);
                    let snapped_beat = self.snap_to_grid(beat_pos);
                    let snapped_time = self.beat_to_time(snapped_beat);

                    // Only snap if we're close to a grid line
                    let diff = (beat_pos - snapped_beat).abs();
                    if diff < self.state.grid_division / 4.0 {
                        self.dispatch(DawAction::SetLastClickedBar(snapped_time));
                    } else {
                        self.dispatch(DawAction::SetLastClickedBar(*pos));
                    }
                }
                UiAction::TrackDrag {
                    track_id,
                    sample_id,
                    position,
                } => {
                    self.dispatch(DawAction::MoveSample(*track_id, *sample_id, *position));
                }
                UiAction::SetSamplePosition {
                    track_id,
                    sample_id,
                    position,
                } => {
                    self.dispatch(DawAction::MoveSample(*track_id, *sample_id, *position));
                }
                UiAction::SetSampleLength {
                    track_id,
                    sample_id,
                    length,
                } => {
                    self.dispatch(DawAction::SetSampleLength(*track_id, *sample_id, *length));
                }
                UiAction::LoadTrackAudio(track_id) => {
                    if let Some(path) = FileDialog::new()
                        .add_filter("Audio", &["mp3", "wav", "ogg", "flac"])
                        .pick_file()
                    {
                        self.dispatch(DawAction::AddSampleToTrack(*track_id, path));
                    }
                }
                UiAction::ToggleTrackMute(track_id) => {
                    self.dispatch(DawAction::ToggleTrackMute(*track_id));
                }
                UiAction::ToggleTrackSolo(track_id) => {
                    self.dispatch(DawAction::ToggleTrackSolo(*track_id));
                }
                UiAction::ToggleTrackRecord(track_id) => {
                    self.dispatch(DawAction::ToggleTrackRecord(*track_id));
                }
                UiAction::SetSampleTrim {
                    track_id,
                    sample_id,
                    trim_start,
                    trim_end,
                } => {
                    self.dispatch(DawAction::SetSampleTrimPoints(
                        *track_id,
                        *sample_id,
                        *trim_start,
                        *trim_end,
                    ));
                }
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Call the DawApp's on_exit method to ensure the project is saved
        DawApp::on_exit(self);
    }
}
