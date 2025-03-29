use crate::daw::{DawAction, DawApp, SelectionRect};
use crate::ui::grid::Grid;
use eframe::egui;
use egui::{Color32, Key, RichText, Stroke};
use rfd::FileDialog;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

// UI Constants
pub const TIMELINE_HEIGHT: f32 = 60.0;
pub const TRACK_HEIGHT: f32 = 100.0;
pub const TRACK_SPACING: f32 = 8.0;
pub const GRID_BACKGROUND: Color32 = Color32::from_rgb(30, 30, 30);
pub const BAR_LINE_COLOR: Color32 = Color32::from_rgb(60, 60, 60);
pub const BEAT_LINE_COLOR: Color32 = Color32::from_rgb(50, 50, 50);
pub const PLAYHEAD_COLOR: Color32 = Color32::from_rgb(255, 50, 50);
pub const TRACK_BORDER_COLOR: Color32 = Color32::from_rgb(60, 60, 60);
pub const TRACK_TEXT_COLOR: Color32 = Color32::from_rgb(200, 200, 200);
pub const WAVEFORM_COLOR: Color32 = Color32::from_rgb(100, 100, 100);
pub const SAMPLE_BORDER_COLOR: Color32 = Color32::from_rgb(60, 60, 60);
pub const SCROLLBAR_SIZE: f32 = 14.0;
pub const BASE_PIXELS_PER_BEAT: f32 = 50.0; // Base pixels per beat at zoom level 1.0

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
    loop_enabled: bool,
    on_toggle_loop: &'a mut dyn FnMut(),
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

            // Make loop indicator clickable to toggle loop mode
            let loop_text = RichText::new("🔄 LOOP").size(14.0);
            let loop_button = if self.loop_enabled {
                ui.add(egui::Button::new(
                    loop_text.color(Color32::from_rgb(255, 100, 100)),
                ))
                .on_hover_text("Click to disable loop mode")
            } else {
                ui.add(egui::Button::new(
                    loop_text.color(Color32::from_rgb(100, 100, 100)),
                ))
                .on_hover_text("Click to enable loop mode (requires selection)")
            };

            if loop_button.clicked() {
                (self.on_toggle_loop)();
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

        // Handle Cmd+L to toggle loop with current selection
        if ctx.input(|i| i.key_pressed(Key::L) && i.modifiers.command) {
            // Only enable looping if there's a selection
            if self.state.selection.is_some() {
                self.dispatch(DawAction::ToggleLoopSelection);

                // If we're enabling looping and playback is active, set the timeline position to the start of the selection
                if self.state.loop_enabled && self.state.is_playing {
                    let selection = self.state.selection.as_ref().unwrap();
                    let loop_start = selection.start_beat * (60.0 / self.state.bpm);
                    self.dispatch(DawAction::SetTimelinePosition(loop_start));
                }
            }
        }

        // Handle Alt+Up/Down arrow keys for zoom
        if ctx.input(|i| i.modifiers.alt) {
            if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
                // Reduced sensitivity (1.05 instead of 1.1)
                let zoom_delta = 1.05;
                let old_zoom = self.state.zoom_level;
                let new_zoom = (self.state.zoom_level * zoom_delta).clamp(0.1, 10.0);

                eprintln!("Alt+Up arrow zoom: {:.2} to {:.2}", old_zoom, new_zoom);
                self.dispatch(DawAction::SetZoomLevel(new_zoom));
            }
            if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
                // Reduced sensitivity (0.95 instead of 0.9)
                let zoom_delta = 0.95;
                let old_zoom = self.state.zoom_level;
                let new_zoom = (self.state.zoom_level * zoom_delta).clamp(0.1, 10.0);

                eprintln!("Alt+Down arrow zoom: {:.2} to {:.2}", old_zoom, new_zoom);
                self.dispatch(DawAction::SetZoomLevel(new_zoom));
            }
        }

        // Update timeline position based on audio playback
        self.update_playback();

        // Store state values locally to use in UI closures
        let is_playing = self.state.is_playing;
        let timeline_position = self.state.timeline_position;
        let bpm = self.state.bpm;
        let grid_division = self.state.grid_division;
        let last_clicked_bar = self.state.last_clicked_bar;

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
            CrossTrackMove {
                source_track_id: usize,
                sample_id: usize,
                target_track_id: usize,
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
            DeleteSample {
                track_id: usize,
                sample_id: usize,
            },
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
            ToggleLoopSelection,
            SetZoomLevel(f32),
        }

        // Collect actions during UI rendering using Rc<RefCell>
        let actions = Rc::new(RefCell::new(Vec::new()));

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
                loop_enabled: self.state.loop_enabled,
                on_toggle_loop: &mut || {
                    actions_clone
                        .borrow_mut()
                        .push(UiAction::ToggleLoopSelection);
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

                // Add zoom level display and control
                let actions_clone = actions.clone();
                ui.horizontal(|ui| {
                    ui.label(format!("Zoom: {:.2}x", self.state.zoom_level));

                    // Add a button to reset zoom
                    if ui.button("Reset Zoom").clicked() {
                        actions_clone.borrow_mut().push(UiAction::SetZoomLevel(1.0));
                    }

                    ui.label("(Use Alt/Option + scroll or Alt + ↑/↓ keys to zoom)");
                });

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
                    on_cross_track_move:
                        &mut |source_track_id, sample_id, target_track_id, position| {
                            actions_clone.borrow_mut().push(UiAction::CrossTrackMove {
                                source_track_id,
                                sample_id,
                                target_track_id,
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
                    on_delete_sample: &mut |track_id, sample_id| {
                        actions_clone.borrow_mut().push(UiAction::DeleteSample {
                            track_id,
                            sample_id,
                        });
                    },
                    h_scroll_offset: self.state.h_scroll_offset,
                    v_scroll_offset: self.state.v_scroll_offset,
                    selection: self.state.selection.clone(),
                    on_selection_change: &mut |new_selection: Option<SelectionRect>| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::SetSelection(new_selection));
                    },
                    loop_enabled: self.state.loop_enabled,
                    zoom_level: self.state.zoom_level,
                    on_zoom_change: &mut |new_zoom| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::SetZoomLevel(new_zoom));
                    },
                    on_playhead_position_change: &mut |new_position| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::SetTimelinePosition(new_position));
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
                UiAction::CrossTrackMove {
                    source_track_id,
                    sample_id,
                    target_track_id,
                    position,
                } => {
                    self.dispatch(DawAction::MoveSampleBetweenTracks(
                        *source_track_id,
                        *sample_id,
                        *target_track_id,
                        *position,
                    ));
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
                UiAction::ToggleLoopSelection => {
                    // Only toggle loop if there's a selection
                    if self.state.selection.is_some() {
                        self.dispatch(DawAction::ToggleLoopSelection);

                        // If we're enabling looping and playback is active, set the timeline position to the start of the selection
                        if self.state.loop_enabled && self.state.is_playing {
                            let selection = self.state.selection.as_ref().unwrap();
                            let loop_start = selection.start_beat * (60.0 / self.state.bpm);
                            self.dispatch(DawAction::SetTimelinePosition(loop_start));
                        }
                    }
                }
                UiAction::SetZoomLevel(new_zoom) => {
                    self.dispatch(DawAction::SetZoomLevel(*new_zoom));
                }
                UiAction::DeleteSample {
                    track_id,
                    sample_id,
                } => {
                    self.dispatch(DawAction::DeleteSample(*track_id, *sample_id));
                }
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Call the DawApp's on_exit method to ensure the project is saved
        DawApp::on_exit(self);
    }
}
