use crate::daw::{DawAction, DawApp, SelectionRect};
use eframe::egui;
use egui::{Color32, Key, RichText, Stroke};
use rfd::FileDialog;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

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
const SAMPLE_BORDER_COLOR: Color32 = Color32::from_rgb(60, 60, 60);
const SCROLLBAR_SIZE: f32 = 14.0;

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
    on_render: &'a mut dyn FnMut(),
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

            // Render button
            ui.add_space(16.0);
            if ui
                .button(RichText::new("🔊").size(20.0))
                .on_hover_text("Render Selection to WAV")
                .clicked()
            {
                (self.on_render)();
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
    on_track_mute: &'a mut dyn FnMut(usize),             // track_id
    on_track_solo: &'a mut dyn FnMut(usize),             // track_id
    on_track_record: &'a mut dyn FnMut(usize),           // track_id
    h_scroll_offset: f32,                                // Horizontal scroll offset in seconds
    v_scroll_offset: f32,                                // Vertical scroll offset in pixels
    selection: Option<SelectionRect>,
    on_selection_change: &'a mut dyn FnMut(Option<SelectionRect>),
}

impl<'a> Grid<'a> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        // Declare selection_drag_start using ui.memory_mut, storing Option<(usize, f32)> directly
        let mut selection_drag_start = ui.memory_mut(|mem| {
            mem.data
                .get_persisted_mut_or_insert_with(ui.id().with("selection_drag_start"), || {
                    None::<(usize, f32)> // Store Option directly, not RefCell<Option<...>>
                })
                .clone() // Clone the Option<(usize, f32)>
        });

        // Create local copies of scroll offsets to avoid borrowing issues
        let mut h_scroll_offset = self.h_scroll_offset;
        let mut v_scroll_offset = self.v_scroll_offset;

        // Track if we've clicked on a sample to prevent duplicate selection
        let mut clicked_on_sample_in_track = false;

        let available_width = ui.available_width();
        let available_height = ui.available_height().min(500.0); // Limit max height

        // Calculate grid height based on number of tracks
        let total_grid_height = TRACK_HEIGHT * self.tracks.len() as f32
            + TRACK_SPACING * (self.tracks.len() as f32 - 1.0);

        // Capture tracks.len() for use in closures
        let tracks_len = self.tracks.len();

        // Determine if scrollbars are needed
        let need_v_scroll = total_grid_height > available_height;
        let actual_width = if need_v_scroll {
            available_width - SCROLLBAR_SIZE
        } else {
            available_width
        };

        // Calculate time scale (in seconds per pixel)
        let pixels_per_beat = 50.0;
        let beats_per_second = self.bpm / 60.0;
        let seconds_per_pixel = 1.0 / (pixels_per_beat * beats_per_second);

        // Calculate total timeline width in pixels
        let num_visible_seconds = actual_width * seconds_per_pixel;
        let num_visible_beats = num_visible_seconds * beats_per_second;
        let beat_width = pixels_per_beat;

        // Estimate total timeline width (arbitrarily use 5 minutes or calculate based on samples)
        let total_duration = 5.0 * 60.0; // 5 minutes default
        let total_timeline_width = total_duration / seconds_per_pixel;

        // Determine if horizontal scrollbar is needed
        let need_h_scroll = true; // Always show horizontal scrollbar
        let actual_height = if need_h_scroll {
            available_height - SCROLLBAR_SIZE
        } else {
            available_height
        };

        let visible_height = if need_v_scroll {
            actual_height
        } else {
            total_grid_height.min(actual_height)
        };

        // Main grid area
        let (grid_rect, grid_response) = ui.allocate_exact_size(
            egui::Vec2::new(actual_width, visible_height),
            egui::Sense::click_and_drag(),
        );

        // --- Define Coordinate Helper Functions HERE (Moved Earlier) ---
        let screen_x_to_beat = move |screen_x: f32| -> f32 {
            let x_relative_to_grid = screen_x - grid_rect.left();
            let seconds_offset = x_relative_to_grid * seconds_per_pixel;
            let total_seconds = h_scroll_offset + seconds_offset;
            total_seconds * beats_per_second
        };

        let screen_y_to_track_index = move |screen_y: f32| -> Option<usize> {
            let y_relative_to_grid = screen_y - grid_rect.top();
            if y_relative_to_grid < 0.0 {
                return None; // Clicked above the grid
            }
            let scrolled_y = v_scroll_offset + y_relative_to_grid;
            let track_index_f = scrolled_y / (TRACK_HEIGHT + TRACK_SPACING);
            let track_index = track_index_f.floor() as usize;

            if track_index < tracks_len {
                Some(track_index)
            } else {
                None // Clicked below the last track
            }
        };

        // Define a local snap_to_grid function
        let snap_to_grid = |beat: f32| -> f32 {
            let division = self.grid_division;
            let lower_grid_line = (beat / division).floor() * division;
            let upper_grid_line = (beat / division).ceil() * division;

            // Find which grid line is closer
            if (beat - lower_grid_line) < (upper_grid_line - beat) {
                lower_grid_line
            } else {
                upper_grid_line
            }
        };
        // --- End Coordinate Helper Functions ---

        // Handle pan gesture for grid
        if grid_response.dragged_by(egui::PointerButton::Middle) {
            let delta = grid_response.drag_delta();
            // Convert pixel delta to seconds for horizontal scroll
            h_scroll_offset -= delta.x * seconds_per_pixel;
            // Constrain horizontal scroll
            h_scroll_offset = h_scroll_offset
                .max(0.0)
                .min(total_duration - num_visible_seconds);

            // Vertical scroll offset directly in pixels
            v_scroll_offset -= delta.y;
            // Constrain vertical scroll
            v_scroll_offset = v_scroll_offset
                .max(0.0)
                .min((total_grid_height - visible_height).max(0.0));
        }

        let painter = ui.painter_at(grid_rect);

        // Draw grid background
        painter.rect_filled(grid_rect, 0.0, GRID_BACKGROUND);

        // Draw grid lines accounting for horizontal scroll
        let h_scroll_pixels = h_scroll_offset / seconds_per_pixel;
        let first_visible_beat = (h_scroll_offset * beats_per_second).floor() as i32;
        let last_visible_beat = (first_visible_beat as f32 + num_visible_beats).ceil() as i32;

        for beat in first_visible_beat..last_visible_beat {
            let beat_pos = beat as f32 / beats_per_second;
            let x = grid_rect.left() + (beat_pos - h_scroll_offset) / seconds_per_pixel;
            let color = if beat % 4 == 0 {
                BAR_LINE_COLOR
            } else {
                BEAT_LINE_COLOR
            };
            painter.line_segment(
                [
                    egui::Pos2::new(x, grid_rect.top()),
                    egui::Pos2::new(x, grid_rect.bottom()),
                ],
                Stroke::new(1.0, color),
            );

            // Draw beat number for every bar (4 beats)
            if beat % 4 == 0 {
                painter.text(
                    egui::Pos2::new(x + 4.0, grid_rect.top() + 12.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}", beat / 4 + 1),
                    egui::FontId::proportional(10.0),
                    Color32::from_rgb(150, 150, 150),
                );
            }
        }

        // Calculate controls width for track controls
        let controls_width = 110.0;
        let track_area_width = actual_width - controls_width;

        // Draw tracks and samples
        let visible_track_start =
            (v_scroll_offset / (TRACK_HEIGHT + TRACK_SPACING)).floor() as usize;
        let visible_track_end =
            ((v_scroll_offset + visible_height) / (TRACK_HEIGHT + TRACK_SPACING)).ceil() as usize;
        let visible_track_end = visible_track_end.min(self.tracks.len());

        // Only draw visible tracks
        for (track_idx, (track_id, name, muted, soloed, recording, samples)) in self
            .tracks
            .iter()
            .enumerate()
            .skip(visible_track_start)
            .take(visible_track_end - visible_track_start)
        {
            let track_top = grid_rect.top() + track_idx as f32 * (TRACK_HEIGHT + TRACK_SPACING)
                - v_scroll_offset;
            let track_bottom = track_top + TRACK_HEIGHT;

            // Skip tracks that are completely outside the visible area
            if track_bottom < grid_rect.top() || track_top > grid_rect.bottom() {
                continue;
            }

            // Draw track border - horizontal line at the bottom of each track
            if track_idx < self.tracks.len() - 1 {
                painter.line_segment(
                    [
                        egui::Pos2::new(grid_rect.left(), track_bottom + TRACK_SPACING / 2.0),
                        egui::Pos2::new(grid_rect.right(), track_bottom + TRACK_SPACING / 2.0),
                    ],
                    Stroke::new(1.0, TRACK_BORDER_COLOR),
                );
            }

            // Draw track controls on the right side
            let control_top = track_top + 30.0; // Move control buttons down to make room for title
            let control_left = grid_rect.left() + track_area_width;

            // Display track name on the right side, above the buttons
            painter.text(
                egui::Pos2::new(control_left + 10.0, track_top + 10.0),
                egui::Align2::LEFT_TOP,
                name,
                egui::FontId::proportional(14.0),
                TRACK_TEXT_COLOR,
            );

            // Add track control buttons
            let button_size = egui::Vec2::new(30.0, 24.0);
            let mute_rect =
                egui::Rect::from_min_size(egui::Pos2::new(control_left, control_top), button_size);

            let solo_rect = egui::Rect::from_min_size(
                egui::Pos2::new(control_left + button_size.x + 5.0, control_top),
                button_size,
            );

            let record_rect = egui::Rect::from_min_size(
                egui::Pos2::new(control_left + 2.0 * (button_size.x + 5.0), control_top),
                button_size,
            );

            // Draw button backgrounds and text
            let mute_color = if *muted {
                Color32::from_rgb(150, 50, 50)
            } else {
                Color32::from_rgb(60, 60, 60)
            };

            let solo_color = if *soloed {
                Color32::from_rgb(50, 150, 50)
            } else {
                Color32::from_rgb(60, 60, 60)
            };

            let record_color = if *recording {
                Color32::from_rgb(150, 50, 50)
            } else {
                Color32::from_rgb(60, 60, 60)
            };

            // Draw button backgrounds
            painter.rect_filled(mute_rect, 4.0, mute_color);
            painter.rect_filled(solo_rect, 4.0, solo_color);
            painter.rect_filled(record_rect, 4.0, record_color);

            // Draw button borders
            painter.rect_stroke(
                mute_rect,
                4.0,
                Stroke::new(1.0, Color32::from_rgb(80, 80, 80)),
            );
            painter.rect_stroke(
                solo_rect,
                4.0,
                Stroke::new(1.0, Color32::from_rgb(80, 80, 80)),
            );
            painter.rect_stroke(
                record_rect,
                4.0,
                Stroke::new(1.0, Color32::from_rgb(80, 80, 80)),
            );

            // Draw button text
            painter.text(
                mute_rect.center(),
                egui::Align2::CENTER_CENTER,
                if *muted { "🔇" } else { "M" },
                egui::FontId::proportional(14.0),
                TRACK_TEXT_COLOR,
            );

            painter.text(
                solo_rect.center(),
                egui::Align2::CENTER_CENTER,
                if *soloed { "S!" } else { "S" },
                egui::FontId::proportional(14.0),
                TRACK_TEXT_COLOR,
            );

            painter.text(
                record_rect.center(),
                egui::Align2::CENTER_CENTER,
                if *recording { "⏺" } else { "R" },
                egui::FontId::proportional(14.0),
                TRACK_TEXT_COLOR,
            );

            // Handle button clicks
            let id_mute = ui.id().with(format!("mute_track_{}", track_id));
            let id_solo = ui.id().with(format!("solo_track_{}", track_id));
            let id_record = ui.id().with(format!("record_track_{}", track_id));

            let mute_response = ui.interact(mute_rect, id_mute, egui::Sense::click());
            let solo_response = ui.interact(solo_rect, id_solo, egui::Sense::click());
            let record_response = ui.interact(record_rect, id_record, egui::Sense::click());

            if mute_response.clicked() {
                (self.on_track_mute)(*track_id);
            }

            if solo_response.clicked() {
                (self.on_track_solo)(*track_id);
            }

            if record_response.clicked() {
                (self.on_track_record)(*track_id);
            }

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
                    // Calculate sample position in beats
                    let beats_position = *position;
                    // Convert to seconds
                    let seconds_position = beats_position / beats_per_second;

                    // Skip samples that are not visible due to horizontal scrolling
                    if seconds_position + (*length / beats_per_second) < h_scroll_offset
                        || seconds_position > h_scroll_offset + num_visible_seconds
                    {
                        continue;
                    }

                    // Calculate visible region
                    let region_left =
                        grid_rect.left() + (seconds_position - h_scroll_offset) / seconds_per_pixel;
                    let region_width = (*length / beats_per_second) / seconds_per_pixel;

                    // Clip to visible area
                    let visible_left = region_left.max(grid_rect.left());
                    let visible_right = (region_left + region_width).min(grid_rect.right());
                    let visible_width = visible_right - visible_left;

                    if visible_width <= 0.0 {
                        continue;
                    }

                    let region_rect = egui::Rect::from_min_size(
                        egui::Pos2::new(visible_left, track_top),
                        egui::Vec2::new(visible_width, TRACK_HEIGHT),
                    );

                    // Draw sample background with alternating colors for better visibility
                    let sample_color = if sample_index % 2 == 0 {
                        Color32::from_rgb(60, 60, 70)
                    } else {
                        Color32::from_rgb(70, 70, 80)
                    };

                    painter.rect_filled(region_rect, 4.0, sample_color);
                    painter.rect_stroke(region_rect, 4.0, Stroke::new(1.0, SAMPLE_BORDER_COLOR));

                    // Show sample name if there's enough space
                    if visible_width > 20.0 {
                        painter.text(
                            egui::Pos2::new(region_rect.left() + 4.0, region_rect.top() + 12.0),
                            egui::Align2::LEFT_TOP,
                            sample_name,
                            egui::FontId::proportional(10.0),
                            TRACK_TEXT_COLOR,
                        );
                    }

                    // Draw waveform if data is available
                    if !waveform.is_empty() && *duration > 0.0 {
                        let waveform_length = waveform.len();

                        // Calculate what portion of the original waveform we're showing
                        let trim_start_ratio = *audio_start_time / *duration;
                        let trim_end_ratio = *audio_end_time / *duration;

                        // Draw the waveform to fit the visible region
                        for x in 0..visible_width as usize {
                            // Map pixel position to position within visible region
                            let position_in_visible = x as f32 / visible_width;

                            // Map to position in full region
                            let full_region_pos = (visible_left - region_left) / region_width
                                + position_in_visible * (visible_width / region_width);

                            // Map to position in trimmed region
                            let position_in_trim = full_region_pos;

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
                    let region_response =
                        ui.interact(region_rect, id, egui::Sense::click_and_drag());

                    if region_response.clicked() {
                        let selection = SelectionRect {
                            start_track_idx: track_idx,
                            start_beat: snap_to_grid(*position),
                            end_track_idx: track_idx,
                            end_beat: snap_to_grid(*position + *length),
                        };
                        (self.on_selection_change)(Some(selection));
                        clicked_on_sample_in_track = true;
                    }

                    if region_response.dragged() {
                        let delta = region_response.drag_delta().x;
                        let time_delta = delta * seconds_per_pixel;
                        let beat_delta = time_delta * beats_per_second;
                        let new_position = *position + beat_delta;
                        (self.on_track_drag)(*track_id, *sample_id, new_position);
                    }
                }
            }
        }

        // --- Handle Grid Background Interaction for Selection ---
        if !clicked_on_sample_in_track {
            if grid_response.drag_started_by(egui::PointerButton::Primary) {
                if let Some(pointer_pos) = grid_response.interact_pointer_pos() {
                    if let Some(start_track_idx) = screen_y_to_track_index(pointer_pos.y) {
                        let raw_start_beat = screen_x_to_beat(pointer_pos.x);
                        let start_beat = snap_to_grid(raw_start_beat); // Snap to grid

                        // Update the stored value directly using get_persisted_mut_or_default
                        ui.memory_mut(|mem| {
                            *mem.data
                                .get_persisted_mut_or_default::<Option<(usize, f32)>>(
                                    ui.id().with("selection_drag_start"),
                                ) = Some((start_track_idx, start_beat));
                        });
                        selection_drag_start = Some((start_track_idx, start_beat)); // Update local copy
                        (self.on_selection_change)(None); // Clear visual selection on drag start
                    }
                }
            }

            if grid_response.dragged_by(egui::PointerButton::Primary) {
                if let (Some((start_idx, start_beat)), Some(current_pos)) = (
                    selection_drag_start, // Use the local copy
                    grid_response.interact_pointer_pos(),
                ) {
                    if let Some(current_idx) = screen_y_to_track_index(current_pos.y) {
                        let raw_current_beat = screen_x_to_beat(current_pos.x);
                        let current_beat = snap_to_grid(raw_current_beat); // Snap to grid

                        let final_start_idx = start_idx.min(current_idx);
                        let final_end_idx = start_idx.max(current_idx);
                        let final_start_beat = start_beat.min(current_beat);
                        let final_end_beat = start_beat.max(current_beat);

                        if (final_end_beat - final_start_beat).abs() > 0.01 {
                            let selection = SelectionRect {
                                start_track_idx: final_start_idx,
                                start_beat: final_start_beat,
                                end_track_idx: final_end_idx,
                                end_beat: final_end_beat,
                            };
                            (self.on_selection_change)(Some(selection));
                        } else {
                            (self.on_selection_change)(None);
                        }
                    } else {
                        (self.on_selection_change)(None); // Dragged outside track area vertically
                    }
                }
            }

            if grid_response.drag_released_by(egui::PointerButton::Primary) {
                // Reset the stored value using get_persisted_mut_or_default
                ui.memory_mut(|mem| {
                    *mem.data
                        .get_persisted_mut_or_default::<Option<(usize, f32)>>(
                            ui.id().with("selection_drag_start"),
                        ) = None;
                });
                selection_drag_start = None; // Update local copy
            }

            if grid_response.clicked_by(egui::PointerButton::Primary) {
                // Check the stored value directly
                if selection_drag_start.is_none() {
                    // Use the local copy
                    (self.on_selection_change)(None);
                } else {
                    // Reset just in case drag release wasn't caught perfectly using get_persisted_mut_or_default
                    ui.memory_mut(|mem| {
                        *mem.data
                            .get_persisted_mut_or_default::<Option<(usize, f32)>>(
                                ui.id().with("selection_drag_start"),
                            ) = None;
                    });
                    selection_drag_start = None; // Update local copy
                }
            }
        }
        // --- End Grid Background Interaction ---

        // Draw selection rectangle if it exists
        if let Some(selection) = &self.selection {
            // Calculate pixel positions from beat positions
            let start_beat_seconds = selection.start_beat / beats_per_second;
            let end_beat_seconds = selection.end_beat / beats_per_second;

            let start_x =
                grid_rect.left() + (start_beat_seconds - h_scroll_offset) / seconds_per_pixel;
            let end_x = grid_rect.left() + (end_beat_seconds - h_scroll_offset) / seconds_per_pixel;

            // Calculate track positions
            let start_y = grid_rect.top()
                + selection.start_track_idx as f32 * (TRACK_HEIGHT + TRACK_SPACING)
                - v_scroll_offset;
            let end_y = grid_rect.top()
                + (selection.end_track_idx as f32 + 1.0) * TRACK_HEIGHT
                + selection.end_track_idx as f32 * TRACK_SPACING
                - v_scroll_offset;

            // Create selection rectangle
            let selection_rect = egui::Rect::from_min_max(
                egui::Pos2::new(start_x, start_y),
                egui::Pos2::new(end_x, end_y),
            );

            // Draw semi-transparent fill
            painter.rect_filled(
                selection_rect,
                0.0,
                Color32::from_rgba_premultiplied(100, 150, 255, 64), // Light blue, semi-transparent
            );

            // Draw border
            painter.rect_stroke(
                selection_rect,
                0.0,
                Stroke::new(2.0, Color32::from_rgb(100, 150, 255)), // Light blue border
            );
        }

        // Draw playhead adjusted for horizontal scroll
        let visible_playhead_x =
            grid_rect.left() + (self.timeline_position - h_scroll_offset) / seconds_per_pixel;

        // Only draw playhead if it's in the visible area
        if visible_playhead_x >= grid_rect.left() && visible_playhead_x <= grid_rect.right() {
            painter.line_segment(
                [
                    egui::Pos2::new(visible_playhead_x, grid_rect.top()),
                    egui::Pos2::new(visible_playhead_x, grid_rect.bottom()),
                ],
                Stroke::new(2.0, PLAYHEAD_COLOR),
            );
        }

        // Add horizontal scrollbar if needed
        if need_h_scroll {
            let h_scrollbar_rect = egui::Rect::from_min_max(
                egui::Pos2::new(grid_rect.left(), grid_rect.bottom()),
                egui::Pos2::new(grid_rect.right(), grid_rect.bottom() + SCROLLBAR_SIZE),
            );

            let h_scrollbar_response =
                ui.allocate_rect(h_scrollbar_rect, egui::Sense::click_and_drag());

            if h_scrollbar_response.hovered() {
                ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::ResizeHorizontal);
            }

            // Draw scrollbar background
            painter.rect_filled(h_scrollbar_rect, 0.0, Color32::from_rgb(40, 40, 40));

            // Calculate thumb size and position
            let h_visible_ratio = num_visible_seconds / total_duration;
            let h_thumb_width = h_visible_ratio * h_scrollbar_rect.width();
            let h_scroll_ratio =
                h_scroll_offset / (total_duration - num_visible_seconds).max(0.001);
            let h_thumb_left = h_scrollbar_rect.left()
                + h_scroll_ratio * (h_scrollbar_rect.width() - h_thumb_width);

            let h_thumb_rect = egui::Rect::from_min_size(
                egui::Pos2::new(h_thumb_left, h_scrollbar_rect.top()),
                egui::Vec2::new(h_thumb_width.max(20.0), SCROLLBAR_SIZE),
            );

            // Draw scrollbar thumb
            painter.rect_filled(h_thumb_rect, 2.0, Color32::from_rgb(100, 100, 100));

            // Handle scrollbar interaction
            if h_scrollbar_response.dragged() {
                let mouse_pos = h_scrollbar_response
                    .interact_pointer_pos()
                    .unwrap_or_default();
                let click_pos_ratio =
                    (mouse_pos.x - h_scrollbar_rect.left()) / h_scrollbar_rect.width();
                h_scroll_offset = click_pos_ratio * (total_duration - num_visible_seconds);
                h_scroll_offset = h_scroll_offset
                    .max(0.0)
                    .min(total_duration - num_visible_seconds);
            }
        }

        // Add vertical scrollbar if needed
        if need_v_scroll {
            let v_scrollbar_rect = egui::Rect::from_min_max(
                egui::Pos2::new(grid_rect.right(), grid_rect.top()),
                egui::Pos2::new(grid_rect.right() + SCROLLBAR_SIZE, grid_rect.bottom()),
            );

            let v_scrollbar_response =
                ui.allocate_rect(v_scrollbar_rect, egui::Sense::click_and_drag());

            if v_scrollbar_response.hovered() {
                ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::ResizeVertical);
            }

            // Draw scrollbar background
            painter.rect_filled(v_scrollbar_rect, 0.0, Color32::from_rgb(40, 40, 40));

            // Calculate thumb size and position
            let v_visible_ratio = visible_height / total_grid_height;
            let v_thumb_height = v_visible_ratio * v_scrollbar_rect.height();
            let v_scroll_ratio = v_scroll_offset / (total_grid_height - visible_height).max(0.001);
            let v_thumb_top = v_scrollbar_rect.top()
                + v_scroll_ratio * (v_scrollbar_rect.height() - v_thumb_height);

            let v_thumb_rect = egui::Rect::from_min_size(
                egui::Pos2::new(v_scrollbar_rect.left(), v_thumb_top),
                egui::Vec2::new(SCROLLBAR_SIZE, v_thumb_height.max(20.0)),
            );

            // Draw scrollbar thumb
            painter.rect_filled(v_thumb_rect, 2.0, Color32::from_rgb(100, 100, 100));

            // Handle scrollbar interaction
            if v_scrollbar_response.dragged() {
                let mouse_pos = v_scrollbar_response
                    .interact_pointer_pos()
                    .unwrap_or_default();
                let click_pos_ratio =
                    (mouse_pos.y - v_scrollbar_rect.top()) / v_scrollbar_rect.height();
                v_scroll_offset = click_pos_ratio * (total_grid_height - visible_height);
                v_scroll_offset = v_scroll_offset
                    .max(0.0)
                    .min((total_grid_height - visible_height).max(0.0));
            }
        }

        // Handle scrolling with mouse wheel
        if grid_response.hovered() {
            let scroll_delta = ui.input(|i| i.scroll_delta);
            // Vertical scrolling with mouse wheel
            if scroll_delta.y != 0.0 {
                v_scroll_offset += scroll_delta.y * -0.5; // Adjust sensitivity
                v_scroll_offset = v_scroll_offset
                    .max(0.0)
                    .min((total_grid_height - visible_height).max(0.0));
            }

            // Horizontal scrolling with shift+wheel or horizontal wheel
            if scroll_delta.x != 0.0 || (ui.input(|i| i.modifiers.shift) && scroll_delta.y != 0.0) {
                let h_delta = if scroll_delta.x != 0.0 {
                    scroll_delta.x
                } else {
                    scroll_delta.y
                };
                let time_delta = h_delta * -0.1 * seconds_per_pixel; // Adjust sensitivity
                h_scroll_offset += time_delta;
                h_scroll_offset = h_scroll_offset
                    .max(0.0)
                    .min(total_duration - num_visible_seconds);
            }
        }

        // Update the actual scroll offsets at the end of the function
        self.h_scroll_offset = h_scroll_offset;
        self.v_scroll_offset = v_scroll_offset;
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
            RenderSelection,
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
            UpdateScrollPosition {
                h_scroll: f32,
                v_scroll: f32,
            },
            SetSelection(Option<SelectionRect>),
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
                on_render: &mut || {
                    actions_clone.borrow_mut().push(UiAction::RenderSelection);
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
                let mut grid = Grid {
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
                    h_scroll_offset: self.state.h_scroll_offset,
                    v_scroll_offset: self.state.v_scroll_offset,
                    selection: self.state.selection.clone(),
                    on_selection_change: &mut |new_selection: Option<SelectionRect>| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::SetSelection(new_selection));
                    },
                };
                grid.draw(ui);

                // Check if scroll position changed and update
                if grid.h_scroll_offset != self.state.h_scroll_offset
                    || grid.v_scroll_offset != self.state.v_scroll_offset
                {
                    actions_clone
                        .borrow_mut()
                        .push(UiAction::UpdateScrollPosition {
                            h_scroll: grid.h_scroll_offset,
                            v_scroll: grid.v_scroll_offset,
                        });
                }

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
                UiAction::RenderSelection => {
                    // Open file dialog to select where to save the rendered WAV
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("WAV Audio", &["wav"])
                        .set_title("Save Rendered Audio")
                        .save_file()
                    {
                        self.dispatch(DawAction::RenderSelection(path));
                    }
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
                UiAction::UpdateScrollPosition { h_scroll, v_scroll } => {
                    self.dispatch(DawAction::UpdateScrollPosition(*h_scroll, *v_scroll));
                }
                UiAction::SetSelection(selection) => {
                    self.dispatch(DawAction::SetSelection(selection.clone()));
                }
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Call the DawApp's on_exit method to ensure the project is saved
        DawApp::on_exit(self);
    }
}
