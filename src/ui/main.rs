use crate::daw::{DawAction, DawApp, SelectionRect, TrackItemType};
use crate::group::Group;
use crate::ui::grid::Grid;
use crate::ui::file_browser::FileBrowserPanel;
use crate::ui::group_panel::GroupPanel;
use crate::audio::Audio;
use crate::ui::drag_drop;
use eframe::egui;
use egui::{Color32, Key, RichText, Stroke};
use rfd::FileDialog;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

// UI Constants
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
pub const GROUP_COLOR: Color32 = Color32::from_rgb(100, 120, 180); // Distinct blue color for Group samples
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
                .on_hover_text("Save Project (⌘S) • Save As (⌘⇧S)")
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

struct TabsBar<'a> {
    tabs: Vec<(usize, String, bool, bool)>, // id, name, is_active, is_group
    on_tab_select: &'a mut dyn FnMut(usize),
    on_tab_close: &'a mut dyn FnMut(usize),
}

impl<'a> TabsBar<'a> {
    pub fn draw(mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (id, name, is_active, is_group) in &self.tabs {
                // Create a styled button for each tab
                let button_text = if *is_group {
                    format!("📦 {}", name)
                } else {
                    format!("📄 {}", name)
                };
                
                // Use different styling for active tab
                let mut button = egui::Button::new(button_text);
                if *is_active {
                    button = button.fill(egui::Color32::from_rgb(60, 70, 90));
                }
                
                // Add the tab button
                let tab_response = ui.add(button);
                
                if tab_response.clicked() {
                    (self.on_tab_select)(*id);
                }
                
                // Add close button for all tabs except the main one (id 0)
                if *id != 0 {
                    // Close button next to tab
                    if ui.small_button("✕").clicked() {
                        (self.on_tab_close)(*id);
                    }
                }
                
                ui.add_space(2.0);
            }
        });
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

        // Handle Cmd+S to save project
        if ctx.input(|i| i.key_pressed(Key::S) && i.modifiers.command && !i.modifiers.shift) {
            self.save_project();
        }

        // Handle Cmd+Shift+S for Save As
        if ctx.input(|i| i.key_pressed(Key::S) && i.modifiers.command && i.modifiers.shift) {
            self.save_project_as();
        }

        // Handle Cmd+Shift+[ to switch to previous tab
        if ctx.input(|i| i.key_pressed(Key::ArrowLeft) && i.modifiers.command && i.modifiers.shift) {
            self.switch_to_previous_tab();
        }

        // Handle Cmd+Shift+] to switch to next tab
        if ctx.input(|i| i.key_pressed(Key::ArrowRight) && i.modifiers.command && i.modifiers.shift) {
            self.switch_to_next_tab();
        }

        // Handle Cmd+L to toggle loop with current selection
        if ctx.input(|i| i.key_pressed(Key::L) && i.modifiers.command) {
            // Only enable looping if there's a selection
            if self.state.selection.is_some() {
                // First set the loop range from the selection
                self.dispatch(DawAction::SetLoopRangeFromSelection);
                // Then toggle the loop state
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

        // Handle drag and drop operations
        
        // Filter dragged files to only include audio files
        drag_drop::filter_dragged_audio_files(ctx);
        
        // Check if any drag operation is active (internal or external)
        let drag_active = drag_drop::is_drag_active(ctx);
        
        // Draw the drop overlay if needed
        drag_drop::draw_drop_overlay(ctx, drag_active);
        
        // Handle external file drops (from OS)
        drag_drop::handle_external_file_drop(self, ctx);
        
        // Handle internal file drops (from file browser)
        drag_drop::handle_internal_file_drop(self, ctx);

        // Store state values locally to use in UI closures
        let is_playing = self.state.is_playing;
        let timeline_position = self.state.timeline_position;
        let bpm = self.state.bpm;
        let grid_division = self.state.grid_division;
        let last_clicked_bar = self.state.last_clicked_bar;

        // Check if we're in an audio box tab or the main project tab
        let active_tab = self.state.tabs.iter().find(|t| t.id == self.state.active_tab_id);
        let is_group_tab = active_tab.map_or(false, |tab| tab.is_group);
        let group_name = active_tab.and_then(|tab| tab.group_name.clone());
        
        // Prepare track info based on active tab
        let track_info: Vec<(usize, String, bool, bool, bool, Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32, TrackItemType)>)>;
        
        if is_group_tab {
            // Create temporary tracks for the audio box - use the same number of tracks as default project
            let mut temp_tracks = Vec::new();
            
            // Create 4 default tracks (same as default project)
            for i in 0..4 {
                let track_id = i;
                let track_name = format!("Track {}", i + 1);
                let mut samples = Vec::new();
                
                // Only put the audio box in the first track
                if i == 0 && group_name.is_some() {
                    if let Some(box_name) = &group_name {
                        // Load the AudioBox data from disk
                        if let Some(project_path) = self.state.file_path.as_ref() {
                            if let Some(project_dir) = project_path.parent() {
                                let box_path = project_dir.join(box_name);
                                let render_path = box_path.join("render.wav");
                                
                                if render_path.exists() {
                                    // Try to load audio file to get waveform data
                                    if let Ok((audio_samples, rate)) = crate::audio::load_audio(&render_path) {
                                        // Generate waveform data
                                        let duration = audio_samples.len() as f32 / rate as f32;
                                        
                                        // Generate waveform for display
                                        let waveform_data = generate_waveform(&audio_samples, 1000);
                                        
                                        // Create a sample info entry representing the audio box
                                        let sample_info = (
                                            0usize, // sample id
                                            box_name.clone(),
                                            0.0f32, // position
                                            duration * (bpm / 60.0), // length in beats
                                            waveform_data, 
                                            rate,
                                            duration,
                                            0.0f32, // audio_start_time
                                            duration, // audio_end_time
                                            TrackItemType::Group,
                                        );
                                        
                                        // Add the box to the track's samples
                                        samples.push(sample_info);
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Add the track to our list
                temp_tracks.push((
                    track_id,
                    track_name,
                    false, // muted
                    false, // soloed
                    false, // recording
                    samples, // samples list (may be empty)
                ));
            }
            
            track_info = temp_tracks;
        } else {
            // Use the main project tracks
            track_info = self
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
                                sample.item_type.clone(),
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
        }

        // Prepare controls info based on active tab
        let track_controls_info: Vec<_>;
        
        if is_group_tab {
            // For audio box tabs, create matching controls for our temporary tracks
            let mut temp_controls = Vec::new();
            
            // Create 4 default tracks (same as default project)
            for i in 0..4 {
                let track_id = i;
                let track_name = format!("Track {}", i + 1);
                let mut samples = Vec::new();
                
                // Only put the audio box in the first track
                if i == 0 && group_name.is_some() {
                    if let Some(box_name) = &group_name {
                        // Get audio data for duration
                        let mut duration = 0.0;
                        
                        if let Some(project_path) = self.state.file_path.as_ref() {
                            if let Some(project_dir) = project_path.parent() {
                                let box_path = project_dir.join(box_name);
                                let render_path = box_path.join("render.wav");
                                
                                if render_path.exists() {
                                    if let Ok((audio_samples, rate)) = crate::audio::load_audio(&render_path) {
                                        duration = audio_samples.len() as f32 / rate as f32;
                                    }
                                }
                            }
                        }
                        
                        // Create a sample entry for controls
                        let sample_info = (
                            0, // sample id
                            box_name.clone(),
                            0.0, // position
                            duration * (bpm / 60.0), // length in beats
                            0.0, // current position
                            duration, // duration
                            0.0, // trim_start
                            0.0, // trim_end
                        );
                        
                        // Add the sample info
                        samples.push(sample_info);
                    }
                }
                
                // Add the track to controls
                temp_controls.push((
                    track_id,
                    track_name,
                    false, // muted
                    false, // soloed
                    false, // recording
                    samples, // samples list (may be empty)
                ));
            }
            
            track_controls_info = temp_controls;
        } else {
            // Use the main project's track controls
            track_controls_info = self
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
        }

        // Define UI actions without capturing self
        #[derive(Clone)]
        enum UiAction {
            Rewind,
            TogglePlayback,
            Forward,
            SetBpm(f32),
            SetGridDivision(f32),
            SaveProject,
            SaveProjectAs,
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
            SetLoopRangeFromSelection,
            SetZoomLevel(f32),
            CreateGroup(String),
            RenameGroup(String, String),
            DeleteGroup(String),
            AddGroupToTrack(usize, String),
            RenderGroupFromSelection(String),
            OpenGroupInNewTab(String),
            SwitchToTab(usize),
            CloseTab(usize),
            SaveGroup(String),
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

        // Create a file browser panel structure with the project directory
        let mut file_browser = if let Some(stored_browser) = ctx.memory(|mem| mem.data.get_temp::<FileBrowserPanel>(egui::Id::new("file_browser"))) {
            stored_browser
        } else {
            // Get the current project folder from the state.file_path
            let project_folder = self.state.file_path.as_ref()
                .and_then(|path| path.parent())
                .map(|path| path.to_path_buf());
            
            FileBrowserPanel::new(project_folder.as_deref())
        };
        
        // Get current side panel width from memory or use default
        let side_panel_width = ctx.memory(|mem| 
            mem.data.get_temp::<f32>(egui::Id::new("side_panel_width"))
                .unwrap_or(200.0)
        );
        
        // Draw file browser panel with resizable width - BEFORE central panel
        let mut panel_shown = file_browser.get_show_panel();
        let mut new_panel_width = side_panel_width;
        egui::SidePanel::left("file_browser_panel")
            .default_width(side_panel_width)
            .resizable(true)
            .show_animated(ctx, panel_shown, |ui| {
                // Store the current width for synchronization
                new_panel_width = ui.available_width() + 2.0; // Add some padding
                
                ui.horizontal(|ui| {
                    ui.heading("Project Files");
                    ui.add_space(ui.available_width() - 100.0);
                    if ui.button("⟳").clicked() {
                        // If the project path has changed, update the file browser
                        let project_folder = self.state.file_path.as_ref()
                            .and_then(|path| path.parent())
                            .map(|path| path.to_path_buf());
                        
                        if let Some(project_path) = project_folder.as_deref() {
                            if !file_browser.is_current_folder(project_path) {
                                file_browser = FileBrowserPanel::new(Some(project_path));
                                
                                // Also update the AudioBox panel
                                let project_path_ref = project_path.clone();
                                ctx.memory_mut(|mem| {
                                    // Update the AudioBox panel with the new project path
                                    let new_box_panel = GroupPanel::new(Some(&project_path_ref));
                                    mem.data.insert_temp(egui::Id::new("group_panel"), new_box_panel);
                                });
                            } else {
                                file_browser.refresh();
                            }
                        } else {
                            file_browser.refresh();
                        }
                    }
                    if ui.button("⬆").clicked() {
                        file_browser.navigate_up();
                    }
                });
                file_browser.draw(ui, ctx);
            });
            
        // Store the panel width for next frame if it changed
        if panel_shown && (new_panel_width - side_panel_width).abs() > 1.0 {
            ctx.memory_mut(|mem| {
                mem.data.insert_temp(egui::Id::new("side_panel_width"), new_panel_width);
            });
        }
        
        // Store the updated file browser
        ctx.memory_mut(|mem| {
            mem.data.insert_temp(egui::Id::new("file_browser"), file_browser);
        });
        
        // Create a group panel structure with the project directory
        let mut group_panel = if let Some(stored_panel) = ctx.memory(|mem| mem.data.get_temp::<GroupPanel>(egui::Id::new("group_panel"))) {
            stored_panel
        } else {
            // Get the current project folder from the state.file_path
            let project_folder = self.state.file_path.as_ref()
                .and_then(|path| path.parent())
                .map(|path| path.to_path_buf());
            
            GroupPanel::new(project_folder.as_deref())
        };
        
        // Draw AudioBox panel with the same width as file browser panel
        let mut box_panel_shown = panel_shown; // Use the same visibility as file browser
        
        if panel_shown {
            let actions_clone = actions.clone();
            egui::SidePanel::left("box_panel")
                .exact_width(new_panel_width)
                .frame(egui::Frame::none())
                .resizable(false)
                .show_separator_line(false)
                .show(ctx, |ui| {
                    if let Some(group_to_open) = group_panel.draw(ui, ctx) {
                        // When a group is opened, open it in a new tab with empty project and tracks
                        actions_clone.borrow_mut().push(UiAction::OpenGroupInNewTab(group_to_open.name));
                    }
                    
                    // Handle group dragging - check if a group is being dragged
                    if let Some(dragged_group) = ctx.memory(|mem| mem.data.get_temp::<Group>(egui::Id::new("dragged_group"))) {
                        // If we detect a drop on the grid area
                        for (track_idx, track) in self.state.tracks.iter().enumerate() {
                            let track_rect = egui::Rect::from_min_size(
                                egui::pos2(0.0, track_idx as f32 * (TRACK_HEIGHT + TRACK_SPACING)),
                                egui::vec2(ui.available_width(), TRACK_HEIGHT),
                            );
                            
                            if ui.rect_contains_pointer(track_rect) && ctx.input(|i| i.pointer.any_released()) {
                                actions_clone.borrow_mut().push(UiAction::AddGroupToTrack(track.id, dragged_group.name));
                                break;
                            }
                        }
                    }
                });
        }
        
        // Store the updated group panel
        ctx.memory_mut(|mem| {
            mem.data.insert_temp(egui::Id::new("group_panel"), group_panel);
        });

        // Complete the UI with the central grid and track panels AFTER side panel
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                // Add zoom level display and control first
                let actions_clone = actions.clone();
                ui.horizontal(|ui| {
                    ui.label(format!("Zoom: {:.2}x", self.state.zoom_level));

                    // Add a button to reset zoom
                    if ui.button("Reset Zoom").clicked() {
                        actions_clone.borrow_mut().push(UiAction::SetZoomLevel(1.0));
                    }

                    ui.label("(Use Alt/Option + scroll or Alt + ↑/↓ keys to zoom)");
                });

                ui.add_space(4.0);

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Tabs bar
                let actions_clone = actions.clone();
                let tabs_bar = TabsBar {
                    tabs: self.state.tabs.iter().map(|tab| {
                        (
                            tab.id, 
                            tab.name.clone(), 
                            tab.id == self.state.active_tab_id,
                            tab.is_group
                        )
                    }).collect(),
                    on_tab_select: &mut |id| {
                        actions_clone.borrow_mut().push(UiAction::SwitchToTab(id));
                    },
                    on_tab_close: &mut |id| {
                        actions_clone.borrow_mut().push(UiAction::CloseTab(id));
                    },
                };
                tabs_bar.draw(ui);

                // Grid in the middle
                let actions_clone = actions.clone();
                let mut grid = Grid {
                    timeline_position: self.state.timeline_position,
                    bpm: self.state.bpm,
                    grid_division: self.state.grid_division,
                    tracks: track_info,
                    on_track_drag: &mut |track_id, sample_id, position| {
                        actions_clone.borrow_mut().push(UiAction::TrackDrag {
                            track_id,
                            sample_id,
                            position,
                        });
                    },
                    on_cross_track_move: &mut |source_track_id, sample_id, target_track_id, position| {
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
                    on_selection_change: &mut |selection| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::SetSelection(selection));
                    },
                    on_playhead_position_change: &mut |position| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::SetTimelinePosition(position));
                    },
                    loop_enabled: self.state.loop_enabled,
                    loop_range: self.state.loop_range,
                    on_group_double_click: &mut |track_id, group_id, group_name| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::OpenGroupInNewTab(group_name.to_string()));
                    },
                    snap_to_grid_enabled: true,
                    seconds_per_pixel: 0.01, // Will be calculated in grid.draw()
                    zoom_level: self.state.zoom_level,
                    on_zoom_change: &mut |zoom_level| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::SetZoomLevel(zoom_level));
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

                // Add a button to save the audio box when it's open in a tab
                if is_group_tab {
                    if let Some(box_name) = &group_name {
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            
                            ui.label(RichText::new("Audio Box:").size(16.0).strong());
                            ui.label(RichText::new(box_name).size(16.0));
                            
                            ui.add_space(ui.available_width() - 120.0);
                            
                            if ui.button(RichText::new("💾 Save Group").size(14.0))
                                .on_hover_text("Save Group and update render.wav")
                                .clicked() 
                            {
                                actions_clone.borrow_mut().push(UiAction::SaveGroup(box_name.clone()));
                            }
                        });
                        
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                    }
                }
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
                UiAction::SaveProjectAs => {
                    self.save_project_as();
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
                        // Get the path where to copy the sample if we're in an AudioBox
                        let active_tab = self.state.tabs.iter().find(|t| t.id == self.state.active_tab_id);
                        let is_group_tab = active_tab.map_or(false, |tab| tab.is_group);
                        
                        if is_group_tab {
                            // If we're in an audio box, we first need to copy the file to the samples directory
                            if let Some(box_name) = active_tab.and_then(|tab| tab.group_name.clone()) {
                                if let Some(project_path) = self.state.file_path.as_ref() {
                                    if let Some(project_dir) = project_path.parent() {
                                        let box_path = project_dir.join(&box_name);
                                        let samples_dir = box_path.join("samples");
                                        
                                        // Create samples directory if it doesn't exist
                                        if !samples_dir.exists() {
                                            if let Err(e) = std::fs::create_dir_all(&samples_dir) {
                                                eprintln!("Failed to create samples directory: {:?}", e);
                                                return;
                                            }
                                        }
                                        
                                        // Copy the file to the samples directory
                                        let file_name = path.file_name().unwrap_or_default();
                                        let target_path = samples_dir.join(file_name);
                                        
                                        if let Err(e) = std::fs::copy(&path, &target_path) {
                                            eprintln!("Failed to copy file to AudioBox: {:?}", e);
                                            return;
                                        }
                                        
                                        // Use the copied file path for the sample
                                        self.dispatch(DawAction::AddSampleToTrack(*track_id, target_path));
                                        
                                eprintln!("Added sample to AudioBox '{}' at track {}", box_name, track_id);
                                    }
                                }
                            }
                        } else {
                            // In main project tab, use the normal AddSampleToTrack action with original path
                            self.dispatch(DawAction::AddSampleToTrack(*track_id, path));
                        }
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
                        // First set the loop range from the selection
                        self.dispatch(DawAction::SetLoopRangeFromSelection);
                        // Then toggle the loop state
                        self.dispatch(DawAction::ToggleLoopSelection);

                        // If we're enabling looping and playback is active, set the timeline position to the start of the selection
                        if self.state.loop_enabled && self.state.is_playing {
                            let selection = self.state.selection.as_ref().unwrap();
                            let loop_start = selection.start_beat * (60.0 / self.state.bpm);
                            self.dispatch(DawAction::SetTimelinePosition(loop_start));
                        }
                    }
                }
                UiAction::SetLoopRangeFromSelection => {
                    self.dispatch(DawAction::SetLoopRangeFromSelection);
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
                UiAction::CreateGroup(name) => {
                    self.dispatch(DawAction::CreateGroup(name.clone()));
                }
                UiAction::RenameGroup(old_name, new_name) => {
                    self.dispatch(DawAction::RenameGroup(old_name.clone(), new_name.clone()));
                }
                UiAction::DeleteGroup(name) => {
                    self.dispatch(DawAction::DeleteGroup(name.clone()));
                }
                UiAction::AddGroupToTrack(track_id, name) => {
                    self.dispatch(DawAction::AddGroupToTrack(*track_id, name.clone()));
                }
                UiAction::RenderGroupFromSelection(name) => {
                    self.dispatch(DawAction::RenderGroupFromSelection(name.clone()));
                }
                UiAction::OpenGroupInNewTab(name) => {
                    self.dispatch(DawAction::OpenGroupInNewTab(name.clone()));
                }
                UiAction::SwitchToTab(tab_id) => {
                    self.dispatch(DawAction::SwitchToTab(*tab_id));
                }
                UiAction::CloseTab(tab_id) => {
                    self.dispatch(DawAction::CloseTab(*tab_id));
                }
                UiAction::SaveGroup(box_name) => {
                    self.dispatch(DawAction::SaveGroup(box_name.clone()));
                }
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Call the DawApp's on_exit method to ensure the project is saved
        DawApp::on_exit(self);
    }
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
