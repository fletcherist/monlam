use crate::daw::SelectionRect;
use crate::ui::main::{
    BAR_LINE_COLOR, BASE_PIXELS_PER_BEAT, BEAT_LINE_COLOR, GRID_BACKGROUND, PLAYHEAD_COLOR,
    SCROLLBAR_SIZE, SELECTION_COLOR, TRACK_BORDER_COLOR, TRACK_HEIGHT,
    TRACK_SPACING, TRACK_TEXT_COLOR, SCROLL_SENSITIVITY, ZOOM_SENSITIVITY_FACTOR,
};
use crate::ui::grid_item::{GridItem, GridItemDragging, GridItemHelper};
use crate::daw::TrackItemType;
use egui::{Color32, Stroke};

pub struct Grid<'a> {
    pub timeline_position: f32,
    pub clicked_position: f32,   // New field to store the position where user clicked
    pub bpm: f32,
    pub grid_division: f32,
    pub tracks: Vec<(
        usize,
        String,
        bool,
        bool,
        bool,
        Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32, TrackItemType)>,
    )>, // Track ID, Name, muted, soloed, recording, samples: (Sample ID, name, position, length, waveform, sample_rate, duration, audio_start_time, audio_end_time, item_type)
    pub on_track_drag: &'a mut dyn FnMut(usize, usize, f32), // track_id, sample_id, new_position
    pub on_cross_track_move: &'a mut dyn FnMut(usize, usize, usize, f32), // source_track_id, sample_id, target_track_id, new_position
    pub on_track_mute: &'a mut dyn FnMut(usize),                          // track_id
    pub on_track_solo: &'a mut dyn FnMut(usize),                          // track_id
    pub on_track_record: &'a mut dyn FnMut(usize),                        // track_id
    pub on_delete_sample: &'a mut dyn FnMut(usize, usize), // track_id, sample_id - Callback when a sample is deleted using backspace/delete key
    pub h_scroll_offset: f32,                              // Horizontal scroll offset in seconds
    pub v_scroll_offset: f32,                              // Vertical scroll offset in pixels
    pub selection: Option<SelectionRect>,
    pub on_selection_change: &'a mut dyn FnMut(Option<SelectionRect>),
    pub on_playhead_position_change: &'a mut dyn FnMut(f32), // Callback when playhead position changes
    pub on_clicked_position_change: &'a mut dyn FnMut(f32),  // Callback when the blue marker position changes
    pub loop_enabled: bool,
    pub loop_start: f32,         // Loop start time in beats
    pub loop_end: f32,           // Loop end time in beats
    pub on_loop_change: &'a mut dyn FnMut(bool, f32, f32),  // Callback when loop range changes (enabled, start, end)
    pub on_group_double_click: &'a mut dyn FnMut(usize, usize, &str), // Callback when a group is double-clicked
    pub snap_to_grid_enabled: bool,
    pub seconds_per_pixel: f32,
    pub zoom_level: f32,                     // Zoom level for the grid view (1.0 = 100%)
    pub on_zoom_change: &'a mut dyn FnMut(f32),
    pub is_playing: bool,                    // Whether the DAW is currently playing
    pub clicked_track_idx: Option<usize>,    // The index of the track that was clicked (for track-only playhead)
}

/// Trait for handling grid selection operations
trait GridSelection {
    fn handle_grid_selection(
        ui: &mut egui::Ui,
        grid_response: &egui::Response,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        selection_drag_start: &mut Option<(usize, f32)>,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    );

    fn start_selection_drag(
        ui: &mut egui::Ui,
        grid_response: &egui::Response,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        selection_drag_start: &mut Option<(usize, f32)>,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    );

    fn update_selection_during_drag(
        grid_response: &egui::Response,
        selection_start: Option<(usize, f32)>,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    );

    fn end_selection_drag(ui: &mut egui::Ui, selection_drag_start: &mut Option<(usize, f32)>);

    fn handle_selection_click(
        ui: &mut egui::Ui,
        selection_drag_start: &mut Option<(usize, f32)>,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    );
}

/// Trait for handling grid zooming operations
trait GridZooming {
    fn handle_grid_zooming(
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        grid_response: &egui::Response,
        zoom_level: &mut f32,
        h_scroll_offset: &mut f32,
        beats_per_second: f32,
        seconds_per_pixel: f32,
        actual_width: f32,
        total_duration: f32,
        on_zoom_change: &mut dyn FnMut(f32),
    );

    fn process_mouse_wheel_zoom(
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        zoom_level: &mut f32,
        h_scroll_offset: &mut f32,
        beats_per_second: f32,
        seconds_per_pixel: f32,
        actual_width: f32,
        total_duration: f32,
        on_zoom_change: &mut dyn FnMut(f32),
    );
}

impl<'a> Grid<'a> {
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        // Declare selection_drag_start using ui.memory_mut, storing Option<(usize, f32)> directly
        let mut selection_drag_start = ui.memory_mut(|mem| {
            mem.data
                .get_persisted_mut_or_insert_with(ui.id().with("selection_drag_start"), || {
                    None::<(usize, f32)> // Store Option directly, not RefCell<Option<...>>
                })
                .clone() // Clone the Option<(usize, f32)>
        });

        // Store the currently dragged sample, if any
        let dragged_sample = ui.memory_mut(|mem| {
            mem.data
                .get_persisted_mut_or_insert_with(ui.id().with("dragged_sample"), || {
                    None::<(usize, usize, f32)> // (track_id, sample_id, initial_click_offset_in_beats)
                })
                .clone()
        });

        // Track if the current frame processed a sample drag
        let mut sample_dragged_this_frame = false;

        // Store the currently dragged group, if any
        let dragged_group = ui.memory_mut(|mem| {
            mem.data
                .get_persisted_mut_or_insert_with(ui.id().with("dragged_group"), || {
                    None::<(usize, usize, f32)> // (track_id, group_id, initial_click_offset_in_beats)
                })
                .clone()
        });

        // Create local copies of scroll offsets to avoid borrowing issues
        let mut h_scroll_offset = self.h_scroll_offset;
        let mut v_scroll_offset = self.v_scroll_offset;

        // Track if we've clicked on a sample to prevent duplicate selection
        let mut clicked_on_sample_in_track = false;

        let available_width = ui.available_width();
        let available_height = ui.available_height(); // Remove the height limitation

        // Calculate minimum grid height based on number of tracks
        let min_grid_height = TRACK_HEIGHT * self.tracks.len() as f32
            + TRACK_SPACING * (self.tracks.len() as f32 - 1.0);
            
        // Always use full available height, but ensure it's at least as big as needed for tracks
        let total_grid_height = min_grid_height.max(available_height);

        // Capture tracks.len() for use in closures
        let tracks_len = self.tracks.len();

        // Determine if scrollbars are needed - vertical scroll only if min_grid_height > available_height
        let need_v_scroll = min_grid_height > available_height;
        let actual_width = if need_v_scroll {
            available_width - SCROLLBAR_SIZE
        } else {
            available_width
        };

        // Calculate grid dimensions
        let total_width = ui.available_width();
        let track_area_width = total_width * 0.8; // Main grid area width (80% of total)
        let control_area_width = total_width - track_area_width; // Side controls width (20%)

        // Calculate time scale (in seconds per pixel) with zoom factor
        let pixels_per_beat = BASE_PIXELS_PER_BEAT * self.zoom_level;
        let beats_per_second = self.bpm / 60.0;
        let seconds_per_pixel = 1.0 / (pixels_per_beat * beats_per_second);

        // Calculate number of visible seconds in the grid area
        let num_visible_seconds = track_area_width * seconds_per_pixel;
        let num_visible_beats = num_visible_seconds * beats_per_second;

        // Estimate total timeline width (arbitrarily use 5 minutes or calculate based on samples)
        let total_duration = 5.0 * 60.0; // 5 minutes default

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

        // Draw loop range control at the top of the grid
        let loop_height = 16.0; // Small height for the loop range control
        let loop_enabled = self.loop_enabled;
        let loop_start = self.loop_start;
        let loop_end = self.loop_end;

        // Allocate space for the loop control
        let (loop_rect, loop_response) = ui.allocate_exact_size(
            egui::Vec2::new(actual_width, loop_height),
            egui::Sense::click_and_drag(),
        );

        if ui.is_rect_visible(loop_rect) {
            let painter = ui.painter();

            // Draw background
            painter.rect_filled(loop_rect, 0.0, Color32::from_rgb(40, 40, 40));

            // Convert beat positions to screen positions
            let beat_to_screen_x = |beat: f32| -> f32 {
                let seconds = beat / beats_per_second;
                let visible_seconds = seconds - h_scroll_offset;
                loop_rect.left() + (visible_seconds / seconds_per_pixel)
            };

            // Draw time markers (bars)
            let first_visible_beat = (h_scroll_offset * beats_per_second).floor() as i32;
            let last_visible_beat = (first_visible_beat as f32 + num_visible_beats).ceil() as i32;

            for beat in first_visible_beat..last_visible_beat {
                let is_bar = beat % 4 == 0;
                if is_bar {
                    let x_pos = beat_to_screen_x(beat as f32);
                    if x_pos >= loop_rect.left() && x_pos <= loop_rect.right() {
                        painter.line_segment(
                            [
                                egui::pos2(x_pos, loop_rect.top()),
                                egui::pos2(x_pos, loop_rect.bottom()),
                            ],
                            Stroke::new(1.0, BAR_LINE_COLOR),
                        );

                        // Draw bar number
                        let bar_number = beat / 4 + 1; // 1-indexed bars
                        painter.text(
                            egui::pos2(x_pos + 2.0, loop_rect.top()),
                            egui::Align2::LEFT_TOP,
                            format!("{}", bar_number),
                            egui::FontId::proportional(9.0),
                            Color32::from_rgb(120, 120, 120),
                        );
                    }
                }
            }

            // Draw loop range - always draw it, not depending on selection
            let start_x = beat_to_screen_x(loop_start);
            let end_x = beat_to_screen_x(loop_end);

            // Only draw if at least partially visible
            if end_x >= loop_rect.left() && start_x <= loop_rect.right() {
                let visible_start_x = start_x.max(loop_rect.left());
                let visible_end_x = end_x.min(loop_rect.right());

                let loop_range_rect = egui::Rect::from_min_max(
                    egui::pos2(visible_start_x, loop_rect.top()),
                    egui::pos2(visible_end_x, loop_rect.bottom()),
                );

                // Draw loop range background
                painter.rect_filled(
                    loop_range_rect,
                    0.0,
                    if loop_enabled {
                        Color32::from_rgba_premultiplied(255, 50, 50, 80)
                    } else {
                        Color32::from_rgba_premultiplied(100, 100, 100, 80)
                    },
                );

                // Draw loop range borders
                painter.rect_stroke(
                    loop_range_rect,
                    0.0,
                    Stroke::new(
                        1.0,
                        if loop_enabled {
                            Color32::from_rgb(255, 50, 50)
                        } else {
                            Color32::from_rgb(150, 150, 150)
                        },
                    ),
                    egui::StrokeKind::Inside,
                );

                // Draw start and end handles if visible
                if start_x >= loop_rect.left() && start_x <= loop_rect.right() {
                    let handle_width = 4.0;
                    let start_handle = egui::Rect::from_min_max(
                        egui::pos2(start_x - handle_width / 2.0, loop_rect.top()),
                        egui::pos2(start_x + handle_width / 2.0, loop_rect.bottom()),
                    );
                    painter.rect_filled(start_handle, 0.0, Color32::from_rgb(200, 200, 200));
                }

                if end_x >= loop_rect.left() && end_x <= loop_rect.right() {
                    let handle_width = 4.0;
                    let end_handle = egui::Rect::from_min_max(
                        egui::pos2(end_x - handle_width / 2.0, loop_rect.top()),
                        egui::pos2(end_x + handle_width / 2.0, loop_rect.bottom()),
                    );
                    painter.rect_filled(end_handle, 0.0, Color32::from_rgb(200, 200, 200));
                }
            }
        }

        // Handle loop range interaction
        if loop_response.dragged() && loop_response.interact_pointer_pos().is_some() {
            let mouse_pos = loop_response.interact_pointer_pos().unwrap();

            // Convert screen position to beat
            let screen_to_beat = |screen_x: f32| -> f32 {
                let x_relative_to_rect = screen_x - loop_rect.left();
                let visible_seconds = x_relative_to_rect * seconds_per_pixel;
                let total_seconds = h_scroll_offset + visible_seconds;
                total_seconds * beats_per_second
            };

            let beat_pos = screen_to_beat(mouse_pos.x);

            // Use the snap_to_grid defined above
            let division = self.grid_division;
            let lower_grid_line = (beat_pos / division).floor() * division;
            let upper_grid_line = (beat_pos / division).ceil() * division;

            // Find which grid line is closer
            let snapped_beat_pos = if (beat_pos - lower_grid_line) < (upper_grid_line - beat_pos) {
                lower_grid_line
            } else {
                upper_grid_line
            };

            // Calculate the start_x and end_x using the same formula as the beat_to_screen_x closure
            let start_seconds = self.loop_start / beats_per_second;
            let end_seconds = self.loop_end / beats_per_second;
            let start_x =
                loop_rect.left() + ((start_seconds - h_scroll_offset) / seconds_per_pixel);
            let end_x = loop_rect.left() + ((end_seconds - h_scroll_offset) / seconds_per_pixel);

            // Check if we're near the start handle
            if (mouse_pos.x - start_x).abs() < 10.0
                || (loop_response.drag_started() && mouse_pos.x < (start_x + end_x) / 2.0 - 10.0)
            {
                // Drag start handle
                let new_loop_start = snapped_beat_pos.max(0.0).min(self.loop_end - 0.5);
                self.loop_start = new_loop_start;
                (self.on_loop_change)(self.loop_enabled, new_loop_start, self.loop_end);
            }
            // Check if we're near the end handle
            else if (mouse_pos.x - end_x).abs() < 10.0
                || (loop_response.drag_started() && mouse_pos.x > (start_x + end_x) / 2.0 + 10.0)
            {
                // Drag end handle
                let new_loop_end = snapped_beat_pos.max(self.loop_start + 0.5);
                self.loop_end = new_loop_end;
                (self.on_loop_change)(self.loop_enabled, self.loop_start, new_loop_end);
            }
            // If not dragging handles, move the entire range
            else {
                // Calculate the range width
                let range_width = self.loop_end - self.loop_start;

                // Get the drag delta in beats since last frame
                let drag_delta = loop_response.drag_delta().x / (pixels_per_beat);

                // Apply drag - move both start and end points together
                let new_start = (self.loop_start + drag_delta).max(0.0);

                // Snap the start position to grid
                let division = self.grid_division;
                let lower_grid_line = (new_start / division).floor() * division;
                let upper_grid_line = (new_start / division).ceil() * division;

                // Find which grid line is closer
                let snapped_start = if (new_start - lower_grid_line) < (upper_grid_line - new_start)
                {
                    lower_grid_line
                } else {
                    upper_grid_line
                };

                // Ensure we maintain the same width
                let new_loop_start = snapped_start;
                let new_loop_end = snapped_start + range_width;
                
                self.loop_start = new_loop_start;
                self.loop_end = new_loop_end;
                (self.on_loop_change)(self.loop_enabled, new_loop_start, new_loop_end);
            }
        }
        
        // Handle double-click to toggle loop enabled state
        if loop_response.double_clicked() {
            // Toggle the loop enabled state
            self.loop_enabled = !self.loop_enabled;
            // Call the callback to notify about the change
            (self.on_loop_change)(self.loop_enabled, self.loop_start, self.loop_end);
        }

        // Allocate the grid area
        let (grid_rect, grid_response) = ui.allocate_exact_size(
            egui::Vec2::new(actual_width, visible_height),
            egui::Sense::click_and_drag(),
        );

        // Store the grid rect in memory for drag and drop functionality
        ui.ctx().memory_mut(|mem| {
            mem.data.insert_temp(egui::Id::new("grid_rect"), grid_rect);
            
            // Debug output to verify the grid rect is being stored
            // eprintln!("Grid rect stored: {:?}", grid_rect);
        });

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
                eprintln!("Clicked above the grid: y_relative={}", y_relative_to_grid);
                return None; // Clicked above the grid
            }
            let scrolled_y = v_scroll_offset + y_relative_to_grid;
            let track_index_f = scrolled_y / (TRACK_HEIGHT + TRACK_SPACING);
            let track_index = track_index_f.floor() as usize;

            eprintln!("Track index calculation: y_relative={}, scrolled_y={}, track_index_f={}, track_index={}, tracks_len={}", 
                      y_relative_to_grid, scrolled_y, track_index_f, track_index, tracks_len);

            if track_index < tracks_len {
                Some(track_index)
            } else {
                eprintln!("Clicked below the last track: track_index={}, tracks_len={}", track_index, tracks_len);
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
                .min((min_grid_height - visible_height).max(0.0));
        }

        let painter = ui.painter_at(grid_rect);

        // Draw grid background
        painter.rect_filled(grid_rect, 0.0, GRID_BACKGROUND);

        // Draw grid lines accounting for horizontal scroll
        let first_visible_beat = (h_scroll_offset * beats_per_second).floor() as i32;
        let last_visible_beat = (first_visible_beat as f32 + num_visible_beats).ceil() as i32;

        // Draw main grid lines
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

            // Draw subdivision grid lines with 0.5 opacity for better snapping visualization
            let subdivisions = (1.0 / self.grid_division).floor() as i32;
            if subdivisions > 1 {
                for i in 1..subdivisions {
                    let subdivision_pos = beat as f32 + (i as f32 / subdivisions as f32);
                    let subdivision_pos_seconds = subdivision_pos / beats_per_second;
                    let subdivision_x = grid_rect.left()
                        + (subdivision_pos_seconds - h_scroll_offset) / seconds_per_pixel;

                    // Create a color with 0.5 opacity
                    let mut subdivision_color = if beat % 4 == 0 {
                        BAR_LINE_COLOR
                    } else {
                        BEAT_LINE_COLOR
                    };
                    subdivision_color = subdivision_color.linear_multiply(0.5); // Reduce opacity

                    painter.line_segment(
                        [
                            egui::Pos2::new(subdivision_x, grid_rect.top()),
                            egui::Pos2::new(subdivision_x, grid_rect.bottom()),
                        ],
                        Stroke::new(0.5, subdivision_color), // Thinner line for subdivisions
                    );
                }
            }
        }

        // Draw tracks and samples
        let visible_track_start =
            (v_scroll_offset / (TRACK_HEIGHT + TRACK_SPACING)).floor() as usize;
        let visible_track_end =
            ((v_scroll_offset + visible_height) / (TRACK_HEIGHT + TRACK_SPACING)).ceil() as usize;
        let visible_track_end = visible_track_end.min(self.tracks.len());

        // Only draw visible tracks
        for (track_idx, (track_id, track_name, muted, soloed, recording, samples)) in self
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
            
            // Draw control panel background - an opaque rectangle for the entire track height
            let control_panel_rect = egui::Rect::from_min_max(
                egui::Pos2::new(control_left, track_top),
                egui::Pos2::new(grid_rect.right(), track_bottom),
            );
            painter.rect_filled(control_panel_rect, 0.0, Color32::from_rgb(45, 45, 50));
            
            // Add a left border to visually separate the control panel from the grid
            painter.line_segment(
                [
                    egui::Pos2::new(control_left, track_top),
                    egui::Pos2::new(control_left, track_bottom),
                ],
                Stroke::new(1.0, Color32::from_rgb(70, 70, 75)),
            );

            // Display track name on the right side, above the buttons
            painter.text(
                egui::Pos2::new(control_left + 10.0, track_top + 10.0),
                egui::Align2::LEFT_TOP,
                track_name,
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
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                solo_rect,
                4.0,
                Stroke::new(1.0, Color32::from_rgb(80, 80, 80)),
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                record_rect,
                4.0,
                Stroke::new(1.0, Color32::from_rgb(80, 80, 80)),
                egui::StrokeKind::Inside,
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
                    item_type,
                ),
            ) in samples.iter().enumerate()
            {
                // Create a GridItem for this sample/group
                let grid_item = GridItem {
                    track_idx,
                    track_id: *track_id,
                    track_top,
                    item_index: sample_index,
                    item_id: *sample_id,
                    item_name: sample_name,
                    position: *position,
                    length: *length,
                    waveform,
                    sample_rate: *sample_rate,
                    duration: *duration,
                    audio_start_time: *audio_start_time,
                    audio_end_time: *audio_end_time,
                    item_type: *item_type,
                };
                
                // Draw the item using our unified interface
                grid_item.draw(
                    ui,
                    &grid_rect,
                    &painter,
                    h_scroll_offset,
                    seconds_per_pixel,
                    beats_per_second,
                    &mut clicked_on_sample_in_track,
                    &mut sample_dragged_this_frame,
                    &snap_to_grid,
                    &mut self.on_selection_change,
                    &mut self.on_track_drag,
                    // Only provide the on_group_double_click for groups
                    if *item_type == TrackItemType::Group {
                        Some(&mut self.on_group_double_click)
                    } else {
                        None
                    },
                );
            }
        }

        // Check if we have a dragged sample from previous frames, and the mouse button is still down
        if let Some((drag_track_id, drag_sample_id, click_offset_beats)) = dragged_sample {
            if ui.input(|i| i.pointer.primary_down()) {
                // Only process active drag if mouse button is still down
                GridItemHelper::process_active_drag(
                    ui,
                    &grid_rect,
                    &self.tracks,
                    drag_track_id,
                    drag_sample_id,
                    TrackItemType::Sample,
                    click_offset_beats,
                    &screen_x_to_beat,
                    &screen_y_to_track_index,
                    &snap_to_grid,
                    &mut sample_dragged_this_frame,
                    &mut self.on_cross_track_move,
                    &mut self.on_track_drag,
                    &mut self.on_selection_change,
                );
            }
        }
        
        // Check if we have a dragged group from previous frames, and the mouse button is still down
        if let Some((drag_track_id, drag_group_id, click_offset_beats)) = dragged_group {
            if ui.input(|i| i.pointer.primary_down()) {
                // Only process active drag if mouse button is still down
                GridItemHelper::process_active_drag(
                    ui,
                    &grid_rect,
                    &self.tracks,
                    drag_track_id,
                    drag_group_id,
                    TrackItemType::Group,
                    click_offset_beats,
                    &screen_x_to_beat,
                    &screen_y_to_track_index,
                    &snap_to_grid,
                    &mut sample_dragged_this_frame,
                    &mut self.on_cross_track_move,
                    &mut self.on_track_drag,
                    &mut self.on_selection_change,
                );
            }
        }

        // Check for end of dragging for both samples and groups
        if !ui.input(|i| i.pointer.primary_down()) {
            // End of drag for sample
            if let Some((drag_track_id, drag_sample_id, _)) = dragged_sample {
                // Clear the dragged sample reference
                GridItemHelper::end_item_drag(
                    ui,
                    TrackItemType::Sample,
                    &self.tracks,
                    drag_track_id,
                    drag_sample_id,
                    self.selection.as_ref(),
                    &mut self.on_selection_change,
                );
            }
            
            // End of drag for group
            if let Some((drag_track_id, drag_group_id, _)) = dragged_group {
                // Clear the dragged group reference
                GridItemHelper::end_item_drag(
                    ui,
                    TrackItemType::Group,
                    &self.tracks,
                    drag_track_id,
                    drag_group_id,
                    self.selection.as_ref(),
                    &mut self.on_selection_change,
                );
            }
        }

        // --- Handle Grid Background Interaction for Selection ---
        if !clicked_on_sample_in_track {
            <Self as GridSelection>::handle_grid_selection(
                ui,
                &grid_response,
                &screen_x_to_beat,
                &screen_y_to_track_index,
                &snap_to_grid,
                &mut selection_drag_start,
                &mut self.on_selection_change,
            );

            // Move playhead on single click (not part of drag)
            if grid_response.clicked_by(egui::PointerButton::Primary) {
                if let Some(pointer_pos) = grid_response.interact_pointer_pos() {
                    // Get the beat position of the click and snap it to the grid
                    let click_beat_position = screen_x_to_beat(pointer_pos.x);
                    let snapped_beat_position = snap_to_grid(click_beat_position);

                    // Convert the snapped beat position back to seconds
                    let snapped_seconds_position = snapped_beat_position / beats_per_second;

                    // Store the clicked track if any
                    let clicked_track = screen_y_to_track_index(pointer_pos.y);
                    
                    // Only update the clicked position and track when manually clicking, not during playback callbacks
                    self.clicked_track_idx = clicked_track;
                    self.clicked_position = snapped_seconds_position;
                    
                    // Debug the clicked track
                    eprintln!("Clicked at y={}, track_idx={:?}, position={}s (clicked_pos={}s)", 
                              pointer_pos.y, clicked_track, snapped_seconds_position, self.clicked_position);

                    // Update both the clicked position and timeline position, but use SEPARATE callbacks
                    self.timeline_position = snapped_seconds_position;
                    (self.on_playhead_position_change)(snapped_seconds_position);
                    (self.on_clicked_position_change)(snapped_seconds_position);
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

            // Choose colors based on whether looping is enabled
            let fill_color = if self.loop_enabled {
                // Brighter colors for loop-enabled state
                Color32::from_rgba_premultiplied(255, 100, 100, 80) // Red-tinted, more visible
            } else {
                // Default colors for normal selection - use the SELECTION_COLOR
                SELECTION_COLOR.linear_multiply(0.3)  // Blue with transparency
            };

            // Draw semi-transparent fill
            painter.rect_filled(selection_rect, 0.0, fill_color);
        }

        // Draw playhead adjusted for horizontal scroll
        let visible_playhead_x =
            grid_rect.left() + (self.timeline_position - h_scroll_offset) / seconds_per_pixel;

        // Only draw the playhead if it's in the visible area AND
        // either the audio is playing OR there's no blue marker visible
        if visible_playhead_x >= grid_rect.left() && visible_playhead_x <= grid_rect.right() &&
           (self.is_playing || self.clicked_track_idx.is_none()) {
            painter.line_segment(
                [
                    egui::Pos2::new(visible_playhead_x, grid_rect.top()),
                    egui::Pos2::new(visible_playhead_x, grid_rect.bottom()),
                ],
                Stroke::new(0.75, PLAYHEAD_COLOR), // Using the white playhead color
            );
        }

        // Draw BLUE marker for selected track
        // This stays in place at all times, regardless of playback state
        if let Some(track_idx) = self.clicked_track_idx {
            // Calculate the x position of the blue marker based on the stored clicked position
            let clicked_x_pos = grid_rect.left() + (self.clicked_position - h_scroll_offset) / seconds_per_pixel;
            
            // Only draw if it's in the visible area
            if clicked_x_pos >= grid_rect.left() && clicked_x_pos <= grid_rect.right() {
                let track_top = grid_rect.top() + track_idx as f32 * (TRACK_HEIGHT + TRACK_SPACING)
                    - v_scroll_offset;
                let track_bottom = track_top + TRACK_HEIGHT;
                
                // Only draw if the track is visible
                if track_bottom >= grid_rect.top() && track_top <= grid_rect.bottom() {
                    let visible_top = track_top.max(grid_rect.top());
                    let visible_bottom = track_bottom.min(grid_rect.bottom());
                    
                    // Offset by 1 pixel to separate
                    let track_marker_x = clicked_x_pos + 1.0;
                    
                    painter.line_segment(
                        [
                            egui::Pos2::new(track_marker_x, visible_top - 3.0), 
                            egui::Pos2::new(track_marker_x, visible_bottom + 3.0),
                        ],
                        Stroke::new(1.0, SELECTION_COLOR), // Using blue selection color
                    );
                }
            }
        }

        // Handle keyboard events for deleting selected samples
        if grid_response.has_focus()
            || grid_response.clicked()
            || grid_rect.contains(ui.input(|i| i.pointer.interact_pos().unwrap_or_default()))
        {
            if ui.input(|i| i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete))
                && self.selection.is_some()
            {
                // Check if we have a single-track selection
                if let Some(selection) = &self.selection {
                    if selection.start_track_idx == selection.end_track_idx {
                        let track_idx = selection.start_track_idx;

                        // Get the track ID
                        let track_id = self.tracks[track_idx].0;

                        // Find samples that are within the selection
                        let track = &self.tracks[track_idx];
                        let samples = &track.5;

                        // Look for samples that overlap with the selection bounds
                        for (sample_id, _, position, length, _, _, _, _, _, _) in samples {
                            let sample_end = *position + *length;

                            // Check if the sample overlaps with the selection
                            if (*position >= selection.start_beat && *position < selection.end_beat)
                                || (sample_end > selection.start_beat
                                    && sample_end <= selection.end_beat)
                                || (*position <= selection.start_beat
                                    && sample_end >= selection.end_beat)
                            {
                                // Delete the sample
                                (self.on_delete_sample)(track_id, *sample_id);

                                // Clear the selection after deleting
                                (self.on_selection_change)(None);

                                // Only delete one sample per backspace press
                                break;
                            }
                        }
                    }
                }
            }
            
            // Handle arrow keys to move the blue marker position
            if self.clicked_track_idx.is_some() {
                // Get current position in beats
                let current_beat_pos = self.clicked_position * (self.bpm / 60.0);
                
                // Check for left/right arrow keys
                if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                    // Move backward by one grid division
                    let new_beat_pos = current_beat_pos - self.grid_division;
                    // Ensure we don't go below zero
                    let new_beat_pos = new_beat_pos.max(0.0);
                    // Snap to grid
                    let snapped_beat_pos = snap_to_grid(new_beat_pos);
                    // Convert back to seconds
                    let new_position = snapped_beat_pos / (self.bpm / 60.0);
                    
                    // Update the clicked position
                    self.clicked_position = new_position;
                    // Update the callback
                    (self.on_clicked_position_change)(new_position);
                    
                    eprintln!("Moved marker left to {}s ({})", new_position, snapped_beat_pos);
                }
                
                if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                    // Move forward by one grid division
                    let new_beat_pos = current_beat_pos + self.grid_division;
                    // Snap to grid
                    let snapped_beat_pos = snap_to_grid(new_beat_pos);
                    // Convert back to seconds
                    let new_position = snapped_beat_pos / (self.bpm / 60.0);
                    
                    // Update the clicked position
                    self.clicked_position = new_position;
                    // Update the callback
                    (self.on_clicked_position_change)(new_position);
                    
                    eprintln!("Moved marker right to {}s ({})", new_position, snapped_beat_pos);
                }
            }
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
            let v_visible_ratio = visible_height / min_grid_height;
            let v_thumb_height = v_visible_ratio * v_scrollbar_rect.height();
            let v_scroll_ratio = v_scroll_offset / (min_grid_height - visible_height).max(0.001);
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
                v_scroll_offset = click_pos_ratio * (min_grid_height - visible_height);
                v_scroll_offset = v_scroll_offset
                    .max(0.0)
                    .min((min_grid_height - visible_height).max(0.0));
            }
        }

        // Handle scrolling with mouse wheel
        if grid_response.hovered() {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
            // Vertical scrolling with mouse wheel
            if scroll_delta.y != 0.0 {
                v_scroll_offset += scroll_delta.y * -SCROLL_SENSITIVITY; // Use constant
                v_scroll_offset = v_scroll_offset
                    .max(0.0)
                    .min((min_grid_height - visible_height).max(0.0));
            }

            // Horizontal scrolling with shift+wheel or horizontal wheel
            if scroll_delta.x != 0.0 || (ui.input(|i| i.modifiers.shift) && scroll_delta.y != 0.0) {
                let h_delta = if scroll_delta.x != 0.0 {
                    scroll_delta.x
                } else {
                    scroll_delta.y
                };
                let time_delta = h_delta * -SCROLL_SENSITIVITY * 0.2 * seconds_per_pixel; // Use constant with adjustment factor
                h_scroll_offset += time_delta;
                h_scroll_offset = h_scroll_offset
                    .max(0.0)
                    .min(total_duration - num_visible_seconds);
            }

            // Handle grid zooming
            <Self as GridZooming>::handle_grid_zooming(
                ui,
                &grid_rect,
                &grid_response,
                &mut self.zoom_level,
                &mut h_scroll_offset,
                beats_per_second,
                seconds_per_pixel,
                actual_width,
                total_duration,
                &mut self.on_zoom_change,
            );
        }

        // Update the actual scroll offsets at the end of the function
        self.h_scroll_offset = h_scroll_offset;
        self.v_scroll_offset = v_scroll_offset;
    }
}

// Implement the GridSelection trait for Grid
impl<'a> GridSelection for Grid<'a> {
    fn handle_grid_selection(
        ui: &mut egui::Ui,
        grid_response: &egui::Response,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        selection_drag_start: &mut Option<(usize, f32)>,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    ) {
        // Handle selection drag start
        if grid_response.drag_started_by(egui::PointerButton::Primary) {
            Self::start_selection_drag(
                ui,
                grid_response,
                screen_x_to_beat,
                screen_y_to_track_index,
                snap_to_grid,
                selection_drag_start,
                on_selection_change,
            );
        }

        // Handle ongoing selection drag
        if grid_response.dragged_by(egui::PointerButton::Primary) {
            Self::update_selection_during_drag(
                grid_response,
                *selection_drag_start, // Pass the current value
                screen_x_to_beat,
                screen_y_to_track_index,
                snap_to_grid,
                on_selection_change,
            );
        }

        // Handle drag release
        if grid_response.drag_released_by(egui::PointerButton::Primary) {
            Self::end_selection_drag(ui, selection_drag_start);
        }

        // Handle single click (not part of drag)
        if grid_response.clicked_by(egui::PointerButton::Primary) {
            Self::handle_selection_click(ui, selection_drag_start, on_selection_change);
        }
    }

    fn start_selection_drag(
        ui: &mut egui::Ui,
        grid_response: &egui::Response,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        selection_drag_start: &mut Option<(usize, f32)>,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    ) {
        if let Some(pointer_pos) = grid_response.interact_pointer_pos() {
            if let Some(start_track_idx) = screen_y_to_track_index(pointer_pos.y) {
                // Get the beat position and snap to grid
                let raw_start_beat = screen_x_to_beat(pointer_pos.x);
                let start_beat = snap_to_grid(raw_start_beat);

                // Store the selection start both in UI memory and local variable
                ui.memory_mut(|mem| {
                    *mem.data
                        .get_persisted_mut_or_default::<Option<(usize, f32)>>(
                            ui.id().with("selection_drag_start"),
                        ) = Some((start_track_idx, start_beat));
                });
                *selection_drag_start = Some((start_track_idx, start_beat));

                // Clear any existing selection when starting a new drag
                on_selection_change(None);
            }
        }
    }

    fn update_selection_during_drag(
        grid_response: &egui::Response,
        selection_start: Option<(usize, f32)>,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    ) {
        if let (Some((start_idx, start_beat)), Some(current_pos)) =
            (selection_start, grid_response.interact_pointer_pos())
        {
            if let Some(current_idx) = screen_y_to_track_index(current_pos.y) {
                // Get current position and snap to grid
                let raw_current_beat = screen_x_to_beat(current_pos.x);
                let current_beat = snap_to_grid(raw_current_beat);

                // Calculate selection rectangle dimensions
                // (ensure start is always before end)
                let final_start_idx = start_idx.min(current_idx);
                let final_end_idx = start_idx.max(current_idx);
                let final_start_beat = start_beat.min(current_beat);
                let final_end_beat = start_beat.max(current_beat);

                // Create and update selection if it has a meaningful size
                if (final_end_beat - final_start_beat).abs() > 0.01 {
                    let selection = SelectionRect {
                        start_track_idx: final_start_idx,
                        start_beat: final_start_beat,
                        end_track_idx: final_end_idx,
                        end_beat: final_end_beat,
                    };
                    on_selection_change(Some(selection));
                } else {
                    // Selection too small, clear it
                    on_selection_change(None);
                }
            } else {
                // Dragged outside of track area, clear selection
                on_selection_change(None);
            }
        }
    }

    fn end_selection_drag(ui: &mut egui::Ui, selection_drag_start: &mut Option<(usize, f32)>) {
        // Clear stored selection start
        ui.memory_mut(|mem| {
            *mem.data
                .get_persisted_mut_or_default::<Option<(usize, f32)>>(
                    ui.id().with("selection_drag_start"),
                ) = None;
        });
        *selection_drag_start = None;
    }

    fn handle_selection_click(
        ui: &mut egui::Ui,
        selection_drag_start: &mut Option<(usize, f32)>,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    ) {
        // If there's no drag in progress, clear selection on click
        if selection_drag_start.is_none() {
            on_selection_change(None);
        } else {
            // Should never happen - reset for safety
            ui.memory_mut(|mem| {
                *mem.data
                    .get_persisted_mut_or_default::<Option<(usize, f32)>>(
                        ui.id().with("selection_drag_start"),
                    ) = None;
            });
            *selection_drag_start = None;
        }
    }
}

// Implement the GridZooming trait for Grid
impl<'a> GridZooming for Grid<'a> {
    fn handle_grid_zooming(
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        grid_response: &egui::Response,
        zoom_level: &mut f32,
        h_scroll_offset: &mut f32,
        beats_per_second: f32,
        seconds_per_pixel: f32,
        actual_width: f32,
        total_duration: f32,
        on_zoom_change: &mut dyn FnMut(f32),
    ) {
        // Check if Alt/Option key is pressed
        let alt_pressed = ui.input(|i| i.modifiers.alt);

        if alt_pressed && grid_response.hovered() {
            Self::process_mouse_wheel_zoom(
                ui,
                grid_rect,
                zoom_level,
                h_scroll_offset,
                beats_per_second,
                seconds_per_pixel,
                actual_width,
                total_duration,
                on_zoom_change,
            );
        }
    }

    fn process_mouse_wheel_zoom(
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        zoom_level: &mut f32,
        h_scroll_offset: &mut f32,
        beats_per_second: f32,
        seconds_per_pixel: f32,
        actual_width: f32,
        total_duration: f32,
        on_zoom_change: &mut dyn FnMut(f32),
    ) {
        // Get the scroll input
        let scroll_input = ui.input(|i| i.smooth_scroll_delta);

        if scroll_input.y != 0.0 {
            // Calculate zoom factor based on actual scroll delta with a sensitivity factor
            // Use shared SCROLL_SENSITIVITY with zoom factor adjustment
            let zoom_factor = 1.0 + (scroll_input.y * -ZOOM_SENSITIVITY_FACTOR * SCROLL_SENSITIVITY); // Invert so scrolling down zooms out
            let old_zoom = *zoom_level;
            let new_zoom = (old_zoom * zoom_factor).clamp(0.1, 10.0);

            // Get mouse position for zooming to cursor position
            if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                if grid_rect.contains(mouse_pos) {
                    // Calculate time at mouse position before zoom
                    let mouse_x_relative = mouse_pos.x - grid_rect.left();
                    let time_at_mouse_before =
                        *h_scroll_offset + mouse_x_relative * seconds_per_pixel;

                    // Update zoom level
                    *zoom_level = new_zoom;

                    // Calculate new seconds_per_pixel after zoom change
                    let new_pixels_per_beat = BASE_PIXELS_PER_BEAT * new_zoom;
                    let new_seconds_per_pixel = 1.0 / (new_pixels_per_beat * beats_per_second);

                    // Calculate new scroll position to keep mouse over same time position
                    *h_scroll_offset =
                        time_at_mouse_before - mouse_x_relative * new_seconds_per_pixel;

                    // Constrain horizontal scroll
                    *h_scroll_offset = h_scroll_offset
                        .max(0.0)
                        .min(total_duration - (actual_width * new_seconds_per_pixel));

                    // Report zoom change through callback
                    (on_zoom_change)(new_zoom);
                } else {
                    // Mouse is outside grid, just update zoom level
                    *zoom_level = new_zoom;
                    (on_zoom_change)(new_zoom);
                }
            } else {
                // No mouse position, just zoom centered
                *zoom_level = new_zoom;
                (on_zoom_change)(new_zoom);
            }
        }
    }
}

