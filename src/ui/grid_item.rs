use crate::daw::{SelectionRect, TrackItemType};
use crate::ui::main::{
    GROUP_COLOR, SAMPLE_BORDER_COLOR, TRACK_HEIGHT, TRACK_TEXT_COLOR, WAVEFORM_COLOR,
};
use egui::{Color32, Stroke};

/// Unified interface for grid items (samples or groups)
pub struct GridItem<'a> {
    pub track_idx: usize,
    pub track_id: usize,
    pub track_top: f32,
    pub item_index: usize,
    pub item_id: usize,
    pub item_name: &'a str,
    pub position: f32,
    pub length: f32,
    pub waveform: &'a Vec<f32>,
    pub sample_rate: u32,
    pub duration: f32,
    pub audio_start_time: f32,
    pub audio_end_time: f32,
    pub item_type: TrackItemType,
}

impl<'a> GridItem<'a> {
    /// Draw a grid item (sample or group) on the grid
    pub fn draw(
        &self,
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        painter: &egui::Painter,
        h_scroll_offset: f32,
        seconds_per_pixel: f32,
        beats_per_second: f32,
        clicked_on_item_in_track: &mut bool,
        item_dragged_this_frame: &mut bool,
        snap_to_grid: &dyn Fn(f32) -> f32,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
        on_group_double_click: Option<&mut dyn FnMut(usize, usize, &str)>,
    ) -> bool {
        if self.length <= 0.0 {
            return false;
        }

        // Calculate item position in beats
        let beats_position = self.position;
        // Convert to seconds
        let seconds_position = beats_position / beats_per_second;

        // Skip items that are not visible due to horizontal scrolling
        if seconds_position + (self.length / beats_per_second) < h_scroll_offset
            || seconds_position > h_scroll_offset + (grid_rect.width() * seconds_per_pixel)
        {
            return false;
        }

        // Calculate visible region
        let region_left = grid_rect.left() + (seconds_position - h_scroll_offset) / seconds_per_pixel;
        let region_width = (self.length / beats_per_second) / seconds_per_pixel;

        // Clip to visible area
        let visible_left = region_left.max(grid_rect.left());
        let visible_right = (region_left + region_width).min(grid_rect.right());
        let visible_width = visible_right - visible_left;

        if visible_width <= 0.0 {
            return false;
        }

        let region_rect = egui::Rect::from_min_size(
            egui::Pos2::new(visible_left, self.track_top),
            egui::Vec2::new(visible_width, TRACK_HEIGHT),
        );

        // Draw item background based on type
        match self.item_type {
            TrackItemType::Sample => {
                // For samples, use alternating colors for better visibility
                let sample_color = if self.item_index % 2 == 0 {
                    Color32::from_rgb(60, 60, 70)
                } else {
                    Color32::from_rgb(70, 70, 80)
                };
                painter.rect_filled(region_rect, 4.0, sample_color);
                painter.rect_stroke(region_rect, 4.0, Stroke::new(1.0, SAMPLE_BORDER_COLOR), egui::StrokeKind::Inside);
            },
            TrackItemType::Group => {
                // For groups, use a distinctive blue color
                painter.rect_filled(region_rect, 4.0, GROUP_COLOR);
                painter.rect_stroke(
                    region_rect, 
                    4.0, 
                    Stroke::new(1.5, Color32::from_rgb(80, 100, 140)), 
                    egui::StrokeKind::Inside
                );
            },
        }

        // Show item name if there's enough space
        if visible_width > 20.0 {
            let display_name = match self.item_type {
                TrackItemType::Sample => self.item_name.to_string(),
                TrackItemType::Group => format!("📦 {}", self.item_name), // Add box icon for groups
            };
            
            painter.text(
                egui::Pos2::new(region_rect.left() + 4.0, region_rect.top() + 12.0),
                egui::Align2::LEFT_TOP,
                display_name,
                egui::FontId::proportional(10.0),
                TRACK_TEXT_COLOR,
            );
        }

        // Draw waveform
        self.draw_waveform(
            painter,
            &region_rect,
            visible_left,
            visible_width,
            region_left,
            region_width,
        );

        // Handle interaction
        self.handle_interaction(
            ui,
            grid_rect,
            region_rect,
            h_scroll_offset,
            seconds_per_pixel,
            beats_per_second,
            clicked_on_item_in_track,
            item_dragged_this_frame,
            snap_to_grid,
            on_selection_change,
            on_track_drag,
            on_group_double_click,
        )
    }

    /// Draw the waveform for an item
    fn draw_waveform(
        &self,
        painter: &egui::Painter,
        region_rect: &egui::Rect,
        visible_left: f32,
        visible_width: f32,
        region_left: f32,
        region_width: f32,
    ) {
        // Draw waveform if data is available
        if !self.waveform.is_empty() && self.duration > 0.0 {
            let waveform_length = self.waveform.len();

            // Calculate what portion of the original waveform we're showing
            let trim_start_ratio = self.audio_start_time / self.duration;
            let trim_end_ratio = self.audio_end_time / self.duration;

            // Choose waveform color based on item type
            let waveform_color = match self.item_type {
                TrackItemType::Sample => WAVEFORM_COLOR,
                TrackItemType::Group => Color32::from_rgb(160, 180, 200), // Lighter color for groups
            };

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
                    let amplitude = self.waveform[sample_index];

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
                        Stroke::new(1.0, waveform_color),
                    );
                }
            }
        }
    }

    /// Handle interaction with the grid item
    fn handle_interaction(
        &self,
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        region_rect: egui::Rect,
        h_scroll_offset: f32,
        seconds_per_pixel: f32,
        beats_per_second: f32,
        clicked_on_item_in_track: &mut bool,
        item_dragged_this_frame: &mut bool,
        snap_to_grid: &dyn Fn(f32) -> f32,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
        on_group_double_click: Option<&mut dyn FnMut(usize, usize, &str)>,
    ) -> bool {
        let id = ui
            .id()
            .with(format!("track_{}_item_{}", self.track_id, self.item_id));
        let region_response = ui.interact(region_rect, id, egui::Sense::click_and_drag());

        let mut interaction_occurred = false;

        // Handle single click (but skip if this is a double click on a group, which is handled separately)
        let is_group_double_click = region_response.double_clicked() && self.item_type == TrackItemType::Group;
        
        if region_response.clicked() && !is_group_double_click {
            let selection = SelectionRect {
                start_track_idx: self.track_idx,
                start_beat: snap_to_grid(self.position),
                end_track_idx: self.track_idx,
                end_beat: snap_to_grid(self.position + self.length),
            };
            on_selection_change(Some(selection));
            *clicked_on_item_in_track = true;
            interaction_occurred = true;
        }
        
        // Handle double click for groups
        if region_response.double_clicked() && self.item_type == TrackItemType::Group {
            if let Some(on_double_click) = on_group_double_click {
                // Just call the callback without modifying the selection
                on_double_click(self.track_id, self.item_id, self.item_name);
                // Return true to indicate interaction occurred, but don't modify any samples
                return true;
            }
        }

        // Check for drag start
        if region_response.drag_started() {
            // Calculate click offset from the start of the item in beats
            let click_offset_beats = if let Some(pointer_pos) = region_response.interact_pointer_pos() {
                let click_beat = ((pointer_pos.x - grid_rect.left()) * seconds_per_pixel
                    + h_scroll_offset)
                    * beats_per_second;
                click_beat - self.position // offset from start of item
            } else {
                0.0 // Fallback if we can't get the pointer position
            };

            // Store the dragged item with the offset, using different memory keys based on type
            match self.item_type {
                TrackItemType::Sample => {
                    ui.memory_mut(|mem| {
                        *mem.data
                            .get_persisted_mut_or_default::<Option<(usize, usize, f32)>>(
                                ui.id().with("dragged_sample"),
                            ) = Some((self.track_id, self.item_id, click_offset_beats));
                    });
                },
                TrackItemType::Group => {
                    ui.memory_mut(|mem| {
                        *mem.data
                            .get_persisted_mut_or_default::<Option<(usize, usize, f32)>>(
                                ui.id().with("dragged_group"),
                            ) = Some((self.track_id, self.item_id, click_offset_beats));
                    });
                },
            }

            // Select the item when drag starts
            let selection = SelectionRect {
                start_track_idx: self.track_idx,
                start_beat: snap_to_grid(self.position),
                end_track_idx: self.track_idx,
                end_beat: snap_to_grid(self.position + self.length),
            };
            on_selection_change(Some(selection));
            *clicked_on_item_in_track = true;
            interaction_occurred = true;
        }

        // Check for drag during this frame
        if region_response.dragged() && !*item_dragged_this_frame {
            let delta = region_response.drag_delta().x;
            let time_delta = delta * seconds_per_pixel;
            let beat_delta = time_delta * beats_per_second;
            let new_position = self.position + beat_delta;
            let snapped_position = snap_to_grid(new_position);

            // We'll only use this for within-track drags, as between-track drags are handled in the grid component
            on_track_drag(self.track_id, self.item_id, snapped_position);
            *item_dragged_this_frame = true;

            // Update the selection to follow the dragged item
            let selection = SelectionRect {
                start_track_idx: self.track_idx,
                start_beat: snapped_position,
                end_track_idx: self.track_idx,
                end_beat: snapped_position + self.length,
            };
            on_selection_change(Some(selection));
            interaction_occurred = true;
        }

        interaction_occurred
    }
}

/// Trait for handling item dragging operations
pub trait GridItemDragging {
    fn handle_item_dragging(
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        tracks: &Vec<(
            usize,
            String,
            bool,
            bool,
            bool,
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32, TrackItemType)>,
        )>,
        drag_track_id: usize,
        drag_item_id: usize,
        item_type: TrackItemType,
        click_offset_beats: f32,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        item_dragged_this_frame: &mut bool,
        on_cross_track_move: &mut dyn FnMut(usize, usize, usize, f32),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
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
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32, TrackItemType)>,
        )>,
        drag_track_id: usize,
        drag_item_id: usize,
        item_type: TrackItemType,
        click_offset_beats: f32,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        item_dragged_this_frame: &mut bool,
        on_cross_track_move: &mut dyn FnMut(usize, usize, usize, f32),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    );

    fn move_item(
        ui: &mut egui::Ui,
        source_track_id: usize,
        item_id: usize,
        item_type: TrackItemType,
        target_track_id: usize,
        new_position: f32,
        click_offset_beats: f32,
        on_cross_track_move: &mut dyn FnMut(usize, usize, usize, f32),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
    );

    fn end_item_drag(
        ui: &mut egui::Ui,
        item_type: TrackItemType,
        tracks: &Vec<(
            usize,
            String,
            bool,
            bool,
            bool,
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32, TrackItemType)>,
        )>,
        drag_track_id: usize,
        drag_item_id: usize,
        selection: Option<&SelectionRect>,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    );
}

pub struct GridItemHelper;

impl GridItemDragging for GridItemHelper {
    fn handle_item_dragging(
        ui: &mut egui::Ui,
        grid_rect: &egui::Rect,
        tracks: &Vec<(
            usize,
            String,
            bool,
            bool,
            bool,
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32, TrackItemType)>,
        )>,
        drag_track_id: usize,
        drag_item_id: usize,
        item_type: TrackItemType,
        click_offset_beats: f32,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        item_dragged_this_frame: &mut bool,
        on_cross_track_move: &mut dyn FnMut(usize, usize, usize, f32),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    ) {
        if ui.input(|i| i.pointer.primary_down()) {
            // Only proceed if mouse button is still down
            Self::process_active_drag(
                ui,
                grid_rect,
                tracks,
                drag_track_id,
                drag_item_id,
                item_type,
                click_offset_beats,
                screen_x_to_beat,
                screen_y_to_track_index,
                snap_to_grid,
                item_dragged_this_frame,
                on_cross_track_move,
                on_track_drag,
                on_selection_change,
            );
        }
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
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32, TrackItemType)>,
        )>,
        drag_track_id: usize,
        drag_item_id: usize,
        item_type: TrackItemType,
        click_offset_beats: f32,
        screen_x_to_beat: &dyn Fn(f32) -> f32,
        screen_y_to_track_index: &dyn Fn(f32) -> Option<usize>,
        snap_to_grid: &dyn Fn(f32) -> f32,
        item_dragged_this_frame: &mut bool,
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
                    // Find the item to get its length
                    if let Some((_, _, _, length, _, _, _, _, _, _)) = tracks[source_track_idx]
                        .5
                        .iter()
                        .find(|(id, _, _, _, _, _, _, _, _, item_type_)| 
                            *id == drag_item_id && *item_type_ == item_type)
                    {
                        Self::move_item(
                            ui,
                            drag_track_id,
                            drag_item_id,
                            item_type,
                            target_track_id,
                            snapped_position,
                            click_offset_beats,
                            on_cross_track_move,
                            on_track_drag,
                        );
                        *item_dragged_this_frame = true;

                        // Update the selection to follow the dragged item
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

    fn move_item(
        ui: &mut egui::Ui,
        source_track_id: usize,
        item_id: usize,
        item_type: TrackItemType,
        target_track_id: usize,
        new_position: f32,
        click_offset_beats: f32,
        on_cross_track_move: &mut dyn FnMut(usize, usize, usize, f32),
        on_track_drag: &mut dyn FnMut(usize, usize, f32),
    ) {
        // If target track is different from source track, move between tracks
        if target_track_id != source_track_id {
            // Cross-track movement
            on_cross_track_move(source_track_id, item_id, target_track_id, new_position);

            // Update the dragged item reference to the new track
            match item_type {
                TrackItemType::Sample => {
                    ui.memory_mut(|mem| {
                        *mem.data
                            .get_persisted_mut_or_default::<Option<(usize, usize, f32)>>(
                                ui.id().with("dragged_sample"),
                            ) = Some((target_track_id, item_id, click_offset_beats));
                    });
                },
                TrackItemType::Group => {
                    ui.memory_mut(|mem| {
                        *mem.data
                            .get_persisted_mut_or_default::<Option<(usize, usize, f32)>>(
                                ui.id().with("dragged_group"),
                            ) = Some((target_track_id, item_id, click_offset_beats));
                    });
                },
            }
        } else {
            // Move within the same track
            on_track_drag(source_track_id, item_id, new_position);
        }
    }

    fn end_item_drag(
        ui: &mut egui::Ui,
        item_type: TrackItemType,
        tracks: &Vec<(
            usize,
            String,
            bool,
            bool,
            bool,
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32, TrackItemType)>,
        )>,
        drag_track_id: usize,
        drag_item_id: usize,
        selection: Option<&SelectionRect>,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    ) {
        // Clear the dragged item reference based on item type
        match item_type {
            TrackItemType::Sample => {
                ui.memory_mut(|mem| {
                    *mem.data
                        .get_persisted_mut_or_default::<Option<(usize, usize, f32)>>(
                            ui.id().with("dragged_sample"),
                        ) = None;
                });
            },
            TrackItemType::Group => {
                ui.memory_mut(|mem| {
                    *mem.data
                        .get_persisted_mut_or_default::<Option<(usize, usize, f32)>>(
                            ui.id().with("dragged_group"),
                        ) = None;
                });
            },
        }
    }
} 