use crate::daw::SelectionRect;
use crate::ui::main::{
    BAR_LINE_COLOR, BASE_PIXELS_PER_BEAT, BEAT_LINE_COLOR, GRID_BACKGROUND, PLAYHEAD_COLOR,
    SAMPLE_BORDER_COLOR, SCROLLBAR_SIZE, TRACK_BORDER_COLOR, TRACK_HEIGHT, TRACK_SPACING,
    TRACK_TEXT_COLOR, WAVEFORM_COLOR,
};
use egui::{Color32, Stroke};

pub struct Grid<'a> {
    pub timeline_position: f32,
    pub bpm: f32,
    pub grid_division: f32,
    pub tracks: Vec<(
        usize,
        String,
        bool,
        bool,
        bool,
        Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32)>,
    )>, // Track ID, Name, muted, soloed, recording, samples: (Sample ID, name, position, length, waveform, sample_rate, duration, audio_start_time, audio_end_time)
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
    pub loop_enabled: bool,
    pub zoom_level: f32, // Zoom level for the grid view (1.0 = 100%)
    pub on_zoom_change: &'a mut dyn FnMut(f32), // Callback when zoom changes
}

// Add this after the main struct definitions, before struct implementations

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

/// Trait for handling sample dragging operations
trait SampleDragging {
    fn handle_sample_dragging(
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        tracks: &Vec<(
            usize,
            String,
            bool,
            bool,
            bool,
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32)>,
        )>,
        drag_track_id: usize,
        drag_sample_id: usize,
        click_offset_beats: f32,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        sample_dragged_this_frame: &mut bool,
        on_cross_track_move: &mut dyn FnMut(usize, usize, usize, f32),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
    );

    fn process_active_drag(
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        tracks: &Vec<(
            usize,
            String,
            bool,
            bool,
            bool,
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32)>,
        )>,
        drag_track_id: usize,
        drag_sample_id: usize,
        click_offset_beats: f32,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        sample_dragged_this_frame: &mut bool,
        on_cross_track_move: &mut dyn FnMut(usize, usize, usize, f32),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    );

    fn move_sample(
        ui: &mut egui::Ui,
        source_track_id: usize,
        sample_id: usize,
        target_track_id: usize,
        new_position: f32,
        click_offset_beats: f32,
        on_cross_track_move: &mut dyn FnMut(usize, usize, usize, f32),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
    );

    fn end_sample_drag(
        ui: &mut egui::Ui,
        tracks: &Vec<(
            usize,
            String,
            bool,
            bool,
            bool,
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32)>,
        )>,
        drag_track_id: usize,
        drag_sample_id: usize,
        selection: Option<&SelectionRect>,
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
        let pixels_per_beat = BASE_PIXELS_PER_BEAT * self.zoom_level;
        let beats_per_second = self.bpm / 60.0;
        let seconds_per_pixel = 1.0 / (pixels_per_beat * beats_per_second);

        // Calculate total timeline width in pixels
        let num_visible_seconds = actual_width * seconds_per_pixel;
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
                Self::draw_sample(
                    ui,
                    &grid_rect,
                    &painter,
                    track_idx,
                    *track_id,
                    track_top,
                    sample_index,
                    *sample_id,
                    sample_name,
                    *position,
                    *length,
                    waveform,
                    *sample_rate,
                    *duration,
                    *audio_start_time,
                    *audio_end_time,
                    h_scroll_offset,
                    seconds_per_pixel,
                    beats_per_second,
                    &mut clicked_on_sample_in_track,
                    &mut sample_dragged_this_frame,
                    &snap_to_grid,
                    &mut self.on_selection_change,
                    &mut self.on_track_drag,
                );
            }
        }

        // Check if we have a dragged sample from previous frames, and the mouse button is still down
        if let Some((drag_track_id, drag_sample_id, click_offset_beats)) = dragged_sample {
            if ui.input(|i| i.pointer.primary_down()) {
                // Only process active drag if mouse button is still down
                <Self as SampleDragging>::process_active_drag(
                    ui,
                    &grid_rect,
                    &self.tracks,
                    drag_track_id,
                    drag_sample_id,
                    click_offset_beats,
                    &screen_x_to_beat,
                    &screen_y_to_track_index,
                    &snap_to_grid,
                    &mut sample_dragged_this_frame,
                    &mut self.on_cross_track_move,
                    &mut self.on_track_drag,
                    &mut self.on_selection_change,
                );
            } else {
                // Mouse button released, update selection if needed
                <Self as SampleDragging>::end_sample_drag(
                    ui,
                    &self.tracks,
                    drag_track_id,
                    drag_sample_id,
                    self.selection.as_ref(),
                    &mut self.on_selection_change,
                );

                // Find the sample's new position and track index to update selection
                // Find the track index for drag_track_id
                if let Some(track_idx) = self
                    .tracks
                    .iter()
                    .position(|(id, _, _, _, _, _)| *id == drag_track_id)
                {
                    // Find the sample
                    if let Some((_, _, position, length, _, _, _, _, _)) = self.tracks[track_idx]
                        .5
                        .iter()
                        .find(|(id, _, _, _, _, _, _, _, _)| *id == drag_sample_id)
                    {
                        // Create new selection for the dragged sample
                        let new_selection = SelectionRect {
                            start_track_idx: track_idx,
                            start_beat: *position,
                            end_track_idx: track_idx,
                            end_beat: *position + *length,
                        };

                        // Update the selection
                        (self.on_selection_change)(Some(new_selection));
                    }
                }
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

                    // Update timeline position and notify through callback
                    self.timeline_position = snapped_seconds_position;
                    (self.on_playhead_position_change)(snapped_seconds_position);
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
            let (fill_color, stroke_color) = if self.loop_enabled {
                // Brighter colors for loop-enabled state
                (
                    Color32::from_rgba_premultiplied(255, 100, 100, 80), // Red-tinted, more visible
                    Color32::from_rgb(255, 100, 100),                    // Red border
                )
            } else {
                // Default colors for normal selection
                (
                    Color32::from_rgba_premultiplied(100, 150, 255, 64), // Light blue, semi-transparent
                    Color32::from_rgb(100, 150, 255),                    // Light blue border
                )
            };

            // Draw semi-transparent fill
            painter.rect_filled(selection_rect, 0.0, fill_color);

            // Draw border
            painter.rect_stroke(selection_rect, 0.0, Stroke::new(2.0, stroke_color));
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
                        for (sample_id, _, position, length, _, _, _, _, _) in samples {
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

    // Keep draw_sample in the main impl because it's fundamental to the Grid functionality
    fn draw_sample(
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        painter: &egui::Painter,
        track_idx: usize,
        track_id: usize,
        track_top: f32,
        sample_index: usize,
        sample_id: usize,
        sample_name: &str,
        position: f32,
        length: f32,
        waveform: &Vec<f32>,
        _sample_rate: u32,
        duration: f32,
        audio_start_time: f32,
        audio_end_time: f32,
        h_scroll_offset: f32,
        seconds_per_pixel: f32,
        beats_per_second: f32,
        clicked_on_sample_in_track: &mut bool,
        sample_dragged_this_frame: &mut bool,
        snap_to_grid: &dyn Fn(f32) -> f32,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
    ) {
        if length > 0.0 {
            // Calculate sample position in beats
            let beats_position = position;
            // Convert to seconds
            let seconds_position = beats_position / beats_per_second;

            // Skip samples that are not visible due to horizontal scrolling
            if seconds_position + (length / beats_per_second) < h_scroll_offset
                || seconds_position > h_scroll_offset + (grid_rect.width() * seconds_per_pixel)
            {
                return;
            }

            // Calculate visible region
            let region_left =
                grid_rect.left() + (seconds_position - h_scroll_offset) / seconds_per_pixel;
            let region_width = (length / beats_per_second) / seconds_per_pixel;

            // Clip to visible area
            let visible_left = region_left.max(grid_rect.left());
            let visible_right = (region_left + region_width).min(grid_rect.right());
            let visible_width = visible_right - visible_left;

            if visible_width <= 0.0 {
                return;
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
            if !waveform.is_empty() && duration > 0.0 {
                let waveform_length = waveform.len();

                // Calculate what portion of the original waveform we're showing
                let trim_start_ratio = audio_start_time / duration;
                let trim_end_ratio = audio_end_time / duration;

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
                    let full_waveform_pos =
                        trim_start_ratio + position_in_trim * (trim_end_ratio - trim_start_ratio);

                    // Get index in the waveform data
                    let sample_index = (full_waveform_pos * waveform_length as f32) as usize;

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
            let region_response = ui.interact(region_rect, id, egui::Sense::click_and_drag());

            if region_response.clicked() {
                let selection = SelectionRect {
                    start_track_idx: track_idx,
                    start_beat: snap_to_grid(position),
                    end_track_idx: track_idx,
                    end_beat: snap_to_grid(position + length),
                };
                on_selection_change(Some(selection));
                *clicked_on_sample_in_track = true;
            }

            // Check for drag start
            if region_response.drag_started() {
                // Calculate click offset from the start of the sample in beats
                let click_offset_beats =
                    if let Some(pointer_pos) = region_response.interact_pointer_pos() {
                        let click_beat = ((pointer_pos.x - grid_rect.left()) * seconds_per_pixel
                            + h_scroll_offset)
                            * beats_per_second;
                        click_beat - position // offset from start of sample
                    } else {
                        0.0 // Fallback if we can't get the pointer position
                    };

                // Store the dragged sample with the offset
                ui.memory_mut(|mem| {
                    *mem.data
                        .get_persisted_mut_or_default::<Option<(usize, usize, f32)>>(
                            ui.id().with("dragged_sample"),
                        ) = Some((track_id, sample_id, click_offset_beats));
                });

                // Select the sample when drag starts
                let selection = SelectionRect {
                    start_track_idx: track_idx,
                    start_beat: snap_to_grid(position),
                    end_track_idx: track_idx,
                    end_beat: snap_to_grid(position + length),
                };
                on_selection_change(Some(selection));
                *clicked_on_sample_in_track = true;
            }

            // Check for drag during this frame
            if region_response.dragged() && !*sample_dragged_this_frame {
                let delta = region_response.drag_delta().x;
                let time_delta = delta * seconds_per_pixel;
                let beat_delta = time_delta * beats_per_second;
                let new_position = position + beat_delta;
                let snapped_position = snap_to_grid(new_position); // Snap to grid just like area selection

                // We'll only use this for within-track drags, as between-track drags are handled above
                on_track_drag(track_id, sample_id, snapped_position);
                *sample_dragged_this_frame = true;

                // Update the selection to follow the dragged sample
                let selection = SelectionRect {
                    start_track_idx: track_idx,
                    start_beat: snapped_position,
                    end_track_idx: track_idx,
                    end_beat: snapped_position + length,
                };
                on_selection_change(Some(selection));
            }
        }
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

// Implement the SampleDragging trait for Grid
impl<'a> SampleDragging for Grid<'a> {
    fn handle_sample_dragging(
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        tracks: &Vec<(
            usize,
            String,
            bool,
            bool,
            bool,
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32)>,
        )>,
        drag_track_id: usize,
        drag_sample_id: usize,
        click_offset_beats: f32,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        sample_dragged_this_frame: &mut bool,
        on_cross_track_move: &mut dyn FnMut(usize, usize, usize, f32),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
    ) {
        if ui.input(|i| i.pointer.primary_down()) {
            // Only proceed if mouse button is still down
            Self::process_active_drag(
                ui,
                grid_rect,
                tracks,
                drag_track_id,
                drag_sample_id,
                click_offset_beats,
                screen_x_to_beat,
                screen_y_to_track_index,
                snap_to_grid,
                sample_dragged_this_frame,
                on_cross_track_move,
                on_track_drag,
                &mut |_| {}, // Empty selection change handler since we don't need it here
            );
        }
        // We don't handle the end of dragging here anymore, it's now handled in the draw function
    }

    fn process_active_drag(
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        tracks: &Vec<(
            usize,
            String,
            bool,
            bool,
            bool,
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32)>,
        )>,
        drag_track_id: usize,
        drag_sample_id: usize,
        click_offset_beats: f32,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        sample_dragged_this_frame: &mut bool,
        on_cross_track_move: &mut dyn FnMut(usize, usize, usize, f32),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    ) {
        // Get current mouse position
        if let Some(pointer_pos) = ui.input(|i| i.pointer.interact_pos()) {
            // Check if the mouse is over a valid track
            if let Some(target_track_idx) = screen_y_to_track_index(pointer_pos.y) {
                // Calculate new position in beats
                let pointer_beat_position = screen_x_to_beat(pointer_pos.x);
                let new_beat_position = pointer_beat_position - click_offset_beats;
                let snapped_position = snap_to_grid(new_beat_position);

                // Get target track id
                let target_track_id = tracks[target_track_idx].0;

                // Find the source track index
                if let Some(source_track_idx) = tracks
                    .iter()
                    .position(|(id, _, _, _, _, _)| *id == drag_track_id)
                {
                    // Find the sample to get its length
                    if let Some((_, _, _, length, _, _, _, _, _)) = tracks[source_track_idx]
                        .5
                        .iter()
                        .find(|(id, _, _, _, _, _, _, _, _)| *id == drag_sample_id)
                    {
                        Self::move_sample(
                            ui,
                            drag_track_id,
                            drag_sample_id,
                            target_track_id,
                            snapped_position,
                            click_offset_beats,
                            on_cross_track_move,
                            on_track_drag,
                        );
                        *sample_dragged_this_frame = true;

                        // Update the selection to follow the dragged sample
                        let selection = SelectionRect {
                            start_track_idx: target_track_idx,
                            start_beat: snapped_position,
                            end_track_idx: target_track_idx,
                            end_beat: snapped_position + *length,
                        };
                        on_selection_change(Some(selection));
                    }
                }
            }
        }
    }

    fn move_sample(
        ui: &mut egui::Ui,
        source_track_id: usize,
        sample_id: usize,
        target_track_id: usize,
        new_position: f32,
        click_offset_beats: f32,
        on_cross_track_move: &mut dyn FnMut(usize, usize, usize, f32),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
    ) {
        // If target track is different from source track, move between tracks
        if target_track_id != source_track_id {
            // Cross-track movement
            on_cross_track_move(source_track_id, sample_id, target_track_id, new_position);

            // Update the dragged sample reference to the new track
            ui.memory_mut(|mem| {
                *mem.data
                    .get_persisted_mut_or_default::<Option<(usize, usize, f32)>>(
                        ui.id().with("dragged_sample"),
                    ) = Some((target_track_id, sample_id, click_offset_beats));
            });
        } else {
            // Move within the same track
            on_track_drag(source_track_id, sample_id, new_position);
        }
    }

    fn end_sample_drag(
        ui: &mut egui::Ui,
        tracks: &Vec<(
            usize,
            String,
            bool,
            bool,
            bool,
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32)>,
        )>,
        drag_track_id: usize,
        drag_sample_id: usize,
        selection: Option<&SelectionRect>,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    ) {
        // Clear the dragged sample reference
        ui.memory_mut(|mem| {
            *mem.data
                .get_persisted_mut_or_default::<Option<(usize, usize, f32)>>(
                    ui.id().with("dragged_sample"),
                ) = None;
        });
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
        let scroll_input = ui.input(|i| i.scroll_delta);

        if scroll_input.y != 0.0 {
            // Calculate zoom factor based on scroll direction - reduced sensitivity
            let zoom_delta = if scroll_input.y > 0.0 { 1.05 } else { 0.95 };
            let old_zoom = *zoom_level;
            let new_zoom = (old_zoom * zoom_delta).clamp(0.1, 10.0);

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

                    eprintln!(
                        "Alt+Scroll zooming from {:.2} to {:.2} at time position {:.2}",
                        old_zoom, new_zoom, time_at_mouse_before
                    );

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
                eprintln!(
                    "Alt+Scroll zoom (centered): {:.2} to {:.2}",
                    old_zoom, new_zoom
                );
            }
        }
    }
}
