use crate::daw::{SelectionRect, TrackItemType};
use crate::ui::main::{SAMPLE_BORDER_COLOR, TRACK_HEIGHT, TRACK_TEXT_COLOR, WAVEFORM_COLOR};
use egui::{Color32, Stroke};

pub fn draw_sample(
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
    sample_rate: u32,
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
) -> bool {
    if length <= 0.0 {
        return false;
    }

    // Calculate sample position in beats
    let beats_position = position;
    // Convert to seconds
    let seconds_position = beats_position / beats_per_second;

    // Skip samples that are not visible due to horizontal scrolling
    if seconds_position + (length / beats_per_second) < h_scroll_offset
        || seconds_position > h_scroll_offset + (grid_rect.width() * seconds_per_pixel)
    {
        return false;
    }

    // Calculate visible region
    let region_left = grid_rect.left() + (seconds_position - h_scroll_offset) / seconds_per_pixel;
    let region_width = (length / beats_per_second) / seconds_per_pixel;

    // Clip to visible area
    let visible_left = region_left.max(grid_rect.left());
    let visible_right = (region_left + region_width).min(grid_rect.right());
    let visible_width = visible_right - visible_left;

    if visible_width <= 0.0 {
        return false;
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
    painter.rect_stroke(region_rect, 4.0, Stroke::new(1.0, SAMPLE_BORDER_COLOR), egui::StrokeKind::Inside);

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

    draw_waveform(
        painter,
        &region_rect,
        waveform,
        duration,
        audio_start_time,
        audio_end_time,
        visible_left,
        visible_width,
        region_left,
        region_width,
    );

    handle_sample_interaction(
        ui,
        grid_rect,
        region_rect,
        track_idx,
        track_id,
        sample_id,
        position,
        length,
        h_scroll_offset,
        seconds_per_pixel,
        beats_per_second,
        clicked_on_sample_in_track,
        sample_dragged_this_frame,
        snap_to_grid,
        on_selection_change,
        on_track_drag,
    )
}

fn draw_waveform(
    painter: &egui::Painter,
    region_rect: &egui::Rect,
    waveform: &Vec<f32>,
    duration: f32,
    audio_start_time: f32,
    audio_end_time: f32,
    visible_left: f32,
    visible_width: f32,
    region_left: f32,
    region_width: f32,
) {
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
}

fn handle_sample_interaction(
    ui: &mut egui::Ui,
    grid_rect: &egui::Rect,
    region_rect: egui::Rect,
    track_idx: usize,
    track_id: usize,
    sample_id: usize,
    position: f32,
    length: f32,
    h_scroll_offset: f32,
    seconds_per_pixel: f32,
    beats_per_second: f32,
    clicked_on_sample_in_track: &mut bool,
    sample_dragged_this_frame: &mut bool,
    snap_to_grid: &dyn Fn(f32) -> f32,
    on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    on_track_drag: &mut dyn FnMut(usize, usize, f32),
) -> bool {
    let id = ui
        .id()
        .with(format!("track_{}_sample_{}", track_id, sample_id));
    let region_response = ui.interact(region_rect, id, egui::Sense::click_and_drag());

    let mut interaction_occurred = false;

    if region_response.clicked() {
        let selection = SelectionRect {
            start_track_idx: track_idx,
            start_beat: snap_to_grid(position),
            end_track_idx: track_idx,
            end_beat: snap_to_grid(position + length),
        };
        on_selection_change(Some(selection));
        *clicked_on_sample_in_track = true;
        interaction_occurred = true;
    }

    // Check for drag start
    if region_response.drag_started() {
        // Calculate click offset from the start of the sample in beats
        let click_offset_beats = if let Some(pointer_pos) = region_response.interact_pointer_pos() {
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
        interaction_occurred = true;
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
        interaction_occurred = true;
    }

    interaction_occurred
}

/// Trait for handling sample dragging operations
pub trait SampleDragging {
    fn handle_sample_dragging(
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
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32, TrackItemType)>,
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
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32, TrackItemType)>,
        )>,
        drag_track_id: usize,
        drag_sample_id: usize,
        selection: Option<&SelectionRect>,
        on_selection_change: &mut dyn FnMut(Option<SelectionRect>),
    );
}

pub struct GridSampleHelper;

impl SampleDragging for GridSampleHelper {
    fn handle_sample_dragging(
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
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32, TrackItemType)>,
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
                    if let Some((_, _, _, length, _, _, _, _, _, _)) = tracks[source_track_idx]
                        .5
                        .iter()
                        .find(|(id, _, _, _, _, _, _, _, _, _)| *id == drag_sample_id)
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
            Vec<(usize, String, f32, f32, Vec<f32>, u32, f32, f32, f32, TrackItemType)>,
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
