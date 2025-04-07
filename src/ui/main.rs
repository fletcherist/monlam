use crate::daw::{DawAction, DawApp, SelectionRect, TrackItemType};
use crate::group::Group;
use crate::ui::grid::Grid;
use crate::ui::file_browser::FileBrowserPanel;
use crate::ui::group_panel::GroupPanel;
use crate::ui::drag_drop;
use eframe::egui;
use egui::{Color32, Key, RichText};
use rfd::FileDialog;

// UI Constants
pub const TRACK_HEIGHT: f32 = 100.0;
pub const TRACK_SPACING: f32 = 8.0;
pub const GRID_BACKGROUND: Color32 = Color32::from_rgb(30, 30, 30);
pub const BAR_LINE_COLOR: Color32 = Color32::from_rgb(60, 60, 60);
pub const BEAT_LINE_COLOR: Color32 = Color32::from_rgb(50, 50, 50);
pub const PLAYHEAD_COLOR: Color32 = Color32::from_rgb(255, 255, 255); // White color for playhead
pub const TRACK_BORDER_COLOR: Color32 = Color32::from_rgb(60, 60, 60);
pub const TRACK_TEXT_COLOR: Color32 = Color32::from_rgb(200, 200, 200);
pub const WAVEFORM_COLOR: Color32 = Color32::from_rgb(100, 100, 100);
pub const SAMPLE_BORDER_COLOR: Color32 = Color32::from_rgb(60, 60, 60);
pub const GROUP_COLOR: Color32 = Color32::from_rgb(100, 120, 180); // Distinct blue color for Group samples
pub const SELECTION_COLOR: Color32 = Color32::from_rgb(50, 100, 255); // Blue color for selected bars/elements
pub const SCROLLBAR_SIZE: f32 = 14.0;
pub const BASE_PIXELS_PER_BEAT: f32 = 50.0; // Base pixels per beat at zoom level 1.0
// Define scroll sensitivity constant
pub const SCROLL_SENSITIVITY: f32 = 2.0; // Higher value = more sensitive scrolling/zooming
pub const ZOOM_SENSITIVITY_FACTOR: f32 = 0.002; // Multiplier to adjust zoom relative to scroll

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
                .on_hover_text("Click to enable loop mode")
            };

            if loop_button.clicked() {
                (self.on_toggle_loop)();
            }

            ui.add_space(16.0);

            // BPM control
            ui.label(RichText::new("BPM:").size(14.0));
            let mut bpm = self.bpm;
            ui.add(egui::DragValue::new(&mut bpm)
                .range(30.0..=240.0)
                .speed(1.0)
                .fixed_decimals(2));
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
            // If there's a selection, set loop range from it
            if self.state.selection.is_some() {
                // Set the loop range from the selection
                self.dispatch(DawAction::SetLoopRangeFromSelection);
            }
            
            // Toggle the loop state regardless of selection
            self.dispatch(DawAction::ToggleLoopSelection);

            // If we're enabling looping and playback is active, set the timeline position to the start of the loop
            if self.state.loop_enabled && self.state.is_playing {
                if let Some(loop_range) = self.state.loop_range {
                    self.dispatch(DawAction::SetTimelinePosition(loop_range.0));
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
        
        // Check for dragged items and show overlay if needed
        let drag_active = drag_drop::is_drag_active(ctx);
        drag_drop::draw_drop_overlay(ctx, drag_active);
        
        // Add a custom drag visualization for group dragging
        if ctx.memory(|mem| mem.data.get_temp::<Group>(egui::Id::new("dragged_group")).is_some()) {
            if let Some(pointer_pos) = ctx.pointer_hover_pos() {
                // Large, very visible cursor that follows the mouse
                egui::Area::new(egui::Id::new("global_group_drag_indicator"))
                    .fixed_pos(pointer_pos)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        ui.add(egui::Label::new(
                            egui::RichText::new("◉") // Large visible cursor
                                .size(40.0)
                                .color(egui::Color32::from_rgba_premultiplied(0, 120, 255, 150))
                        ));
                    });
            }
        }
        
        // Handle external file drops (from OS)
        drag_drop::handle_external_file_drop(self, ctx);
        
        // Handle internal file drops (from file browser)
        drag_drop::handle_internal_file_drop(self, ctx);
        
        // Get any pending actions back from the UI
        let actions = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

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
                                        
                                        // Create a sample entry for the group
                                        let sample_info = (
                                            0usize, // sample id
                                            box_name.clone(),
                                            0.0f32, // grid position
                                            4.0f32, // grid length (default to 4 beats)
                                            waveform_data, // waveform data
                                            rate as u32, // sample rate
                                            duration, // full duration
                                            0.0f32, // audio start time
                                            0.0f32, // audio end time
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
                    track_id as usize,
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
        #[allow(dead_code)]
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
            SetLastClickedPosition(f32),
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
            CreateTrack,
            CreateGroup(String),
            RenameGroup(String, String),
            DeleteGroup(String),
            AddGroupToTrack(usize, String),
            RenderGroupFromSelection(String),
            OpenGroupInNewTab(String),
            SwitchToTab(usize),
            CloseTab(usize),
            SaveGroup(String),
            UpdateLoopRange(bool, f32, f32),
        }

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
                        eprintln!("DEBUG: Group drag detected in main loop: {} (size: {})", dragged_group.name, dragged_group.waveform.len());
                        
                        // Now we need to show a visual indicator that we have a dragged group
                        if let Some(pointer_pos) = ctx.pointer_hover_pos() {
                            // Show a simple indicator in the main loop too - this ensures visibility
                            egui::Area::new(egui::Id::new("main_drag_indicator"))
                                .fixed_pos(pointer_pos)
                                .order(egui::Order::Foreground) // Make sure it's on top
                                .show(ctx, |ui| {
                                    ui.label(format!("Dragging: {}", dragged_group.name));
                                });
                        }
                        
                        // If we detect a drop on the grid area
                        if let Some(grid_rect) = ctx.memory(|mem| mem.data.get_temp::<egui::Rect>(egui::Id::new("grid_rect"))) {
                            if let Some(mouse_pos) = ctx.pointer_hover_pos() {
                                eprintln!("DEBUG: Mouse position during group drag: {:?}", mouse_pos);
                                eprintln!("DEBUG: Grid rect: {:?}", grid_rect);
                                eprintln!("DEBUG: Mouse is inside grid: {}", grid_rect.contains(mouse_pos));
                                eprintln!("DEBUG: Pointer released: {}", ctx.input(|i| i.pointer.any_released()));
                                
                                // For dragging from group panel, just check if mouse released inside grid
                                let is_released_on_grid = grid_rect.contains(mouse_pos) && ctx.input(|i| i.pointer.any_released());
                                
                                if is_released_on_grid {
                                    eprintln!("DEBUG: Group drop detected on grid (mouse released on grid)");
                                    
                                    // Calculate drop target using the grid's drop target system
                                    if let Some(target) = drag_drop::calculate_drop_target(self, ctx, mouse_pos) {
                                        eprintln!("DEBUG: Adding group '{}' to track {} at beat position {}", 
                                            dragged_group.name, target.track_id, target.beat_position);
                                            
                                        // Check if we need to create a new track
                                        if self.state.tracks.is_empty() {
                                            eprintln!("DEBUG: No tracks available, creating a new track for the group");
                                            actions_clone.borrow_mut().push(UiAction::CreateTrack);
                                        }
                                            
                                        let group_name = dragged_group.name.clone();
                                        actions_clone.borrow_mut().push(UiAction::AddGroupToTrack(target.track_id, group_name));
                                        
                                        // After adding the group, move it to the correct position
                                        // We need to find the newly added group in the target track
                                        if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == target.track_id) {
                                            if let Some(sample) = track.samples.iter_mut().find(|s| 
                                                s.item_type == TrackItemType::Group && s.name == dragged_group.name) {
                                                // Move the sample to the drop position
                                                actions_clone.borrow_mut().push(UiAction::SetSamplePosition {
                                                    track_id: target.track_id,
                                                    sample_id: sample.id,
                                                    position: target.beat_position
                                                });
                                                eprintln!("DEBUG: Moving group '{}' to position {}", dragged_group.name, target.beat_position);
                                            } else {
                                                eprintln!("DEBUG: Could not find newly added group sample '{}'", dragged_group.name);
                                            }
                                        } else {
                                            eprintln!("DEBUG: Could not find track with ID {}", target.track_id);
                                        }
                                    } else {
                                        eprintln!("DEBUG: Failed to calculate drop target for group");
                                    }
                                    
                                    // Clear the group drag flag
                                    ctx.memory_mut(|mem| {
                                        mem.data.insert_temp(egui::Id::new("group_drag_active"), false);
                                        eprintln!("DEBUG: Set group_drag_active=false after drop");
                                    });
                                }
                            } else {
                                eprintln!("DEBUG: No mouse position available during group drag");
                            }
                        } else {
                            eprintln!("DEBUG: Grid rect not found during group drag");
                        }
                        
                        // Clear dragged group state if mouse is released (whether over the grid or not)
                        if ctx.input(|i| i.pointer.any_released()) {
                            eprintln!("DEBUG: Mouse released while dragging group '{}'", dragged_group.name);
                            ctx.memory_mut(|mem| {
                                mem.data.remove::<Group>(egui::Id::new("dragged_group"));
                                mem.data.insert_temp(egui::Id::new("group_drag_active"), false);
                                eprintln!("DEBUG: Cleared dragged_group and set group_drag_active=false on release");
                            });
                            eprintln!("DEBUG: Drag ended: cleared dragged group state");
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
                
                // Get the last clicked track index and position from the previous frame if it exists
                let previous_clicked_track = ctx.memory(|mem| 
                    mem.data.get_temp::<Option<usize>>(egui::Id::new("grid_clicked_track"))
                        .unwrap_or(None)
                );
                
                let previous_clicked_position = ctx.memory(|mem| 
                    mem.data.get_temp::<Option<f32>>(egui::Id::new("grid_clicked_position"))
                        .unwrap_or(None)
                ).unwrap_or(self.state.timeline_position);
                
                let mut grid = Grid {
                    timeline_position: self.state.timeline_position,
                    clicked_position: self.state.last_clicked_position, // Use the dedicated field from state
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
                    on_clicked_position_change: &mut |position| {
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::SetLastClickedPosition(position));
                    },
                    loop_enabled: self.state.loop_enabled,
                    loop_start: self.state.loop_range.map_or(0.0, |range| range.0 * 4.0), // Convert seconds to beats with fixed ratio
                    loop_end: self.state.loop_range.map_or(16.0, |range| range.1 * 4.0), // Convert seconds to beats with fixed ratio
                    on_loop_change: &mut |enabled, start, end| {
                        // Convert from beats back to seconds with fixed ratio
                        let start_seconds = start / 4.0;
                        let end_seconds = end / 4.0;
                        actions_clone
                            .borrow_mut()
                            .push(UiAction::UpdateLoopRange(enabled, start_seconds, end_seconds));
                    },
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
                    is_playing: self.state.is_playing,
                    clicked_track_idx: previous_clicked_track,
                };
                grid.draw(ui);

                // Store clicked track and position for the next frame
                ctx.memory_mut(|mem| {
                    mem.data.insert_temp(egui::Id::new("grid_clicked_track"), grid.clicked_track_idx);
                    mem.data.insert_temp(egui::Id::new("grid_clicked_position"), grid.clicked_position);
                });

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
                    // Toggle loop directly without requiring a selection
                    self.dispatch(DawAction::ToggleLoopSelection);
                    
                    // If we're enabling looping and playback is active, set the timeline position to the start of the loop
                    if self.state.loop_enabled && self.state.is_playing {
                        if let Some(loop_range) = self.state.loop_range {
                            self.dispatch(DawAction::SetTimelinePosition(loop_range.0));
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
                UiAction::CreateTrack => {
                    self.dispatch(DawAction::CreateTrack);
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
                UiAction::SetLastClickedPosition(position) => {
                    // Use the new DawAction that only updates the clicked position
                    self.dispatch(DawAction::SetClickedPosition(*position));
                }
                UiAction::UpdateLoopRange(enabled, start, end) => {
                    self.dispatch(DawAction::UpdateLoopRange(*enabled, *start, *end));
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
