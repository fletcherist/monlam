use crate::daw::{DawAction, DawApp};
use crate::group::Group;
use crate::ui::main::{BASE_PIXELS_PER_BEAT, TRACK_HEIGHT, TRACK_SPACING};
use eframe::egui;
use std::path::{Path, PathBuf};

/// Check if a file has a supported audio extension
pub fn is_supported_audio_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        return ext == "wav" || ext == "mp3" || ext == "ogg" || ext == "flac";
    }
    false
}

/// Filter dropped files to only include audio files
pub fn filter_dragged_audio_files(ctx: &egui::Context) {
    ctx.input_mut(|i| {
        i.raw.hovered_files.retain(|file| {
            if let Some(path) = &file.path {
                is_supported_audio_file(path)
            } else {
                false
            }
        });
    });
}

/// Draw a visual overlay when dragging files over the grid
pub fn draw_drop_overlay(ctx: &egui::Context, drag_active: bool) {
    if !drag_active {
        return;
    }
    
    // Get the grid rect
    if let Some(grid_rect) = ctx.memory(|mem| mem.data.get_temp::<egui::Rect>(egui::Id::new("grid_rect"))) {
        eprintln!("Drawing drag overlay over grid rect: {:?}", grid_rect);
        
        // Determine if this is a group drag
        let is_group_drag = ctx.memory(|mem| 
            mem.data.get_temp::<bool>(egui::Id::new("group_drag_active")).unwrap_or(false)
        );
        
        // Create a transparent overlay over the grid
        egui::Area::new(egui::Id::new("drag_overlay"))
            .fixed_pos(egui::pos2(grid_rect.left(), grid_rect.top()))
            .order(egui::Order::Middle) // Place behind foreground elements but visible
            .show(ctx, |ui| {
                let overlay_rect = egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(grid_rect.width(), grid_rect.height()),
                );
                
                // Draw semi-transparent overlay with different color for groups vs files
                let color = if is_group_drag {
                    egui::Color32::from_rgba_premultiplied(0, 80, 255, 60) // Blue for groups
                } else {
                    egui::Color32::from_rgba_premultiplied(0, 120, 255, 40) // Default
                };
                
                ui.painter().rect_filled(
                    overlay_rect,
                    0.0,
                    color,
                );
                
                // Add dashed border with different style for groups vs files
                let stroke_color = if is_group_drag {
                    egui::Color32::from_rgba_premultiplied(0, 80, 255, 200) // More visible
                } else {
                    egui::Color32::from_rgba_premultiplied(0, 120, 255, 160)
                };
                
                ui.painter().rect_stroke(
                    overlay_rect,
                    4.0,
                    egui::Stroke::new(2.0, stroke_color),
                    egui::StrokeKind::Inside,
                );
                
                // Add drop text
                let text = if is_group_drag {
                    "Drop group here"
                } else {
                    "Drop audio files here"
                };
                
                ui.painter().text(
                    overlay_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(24.0),
                    egui::Color32::from_rgba_premultiplied(255, 255, 255, 200),
                );
            });
    } else {
        eprintln!("Cannot draw drag overlay - grid rect not found");
    }
}

/// Calculates the target position for a dropped file
pub struct DropTarget {
    pub track_id: usize,
    pub beat_position: f32,
}

/// Check if any drag operation is active
pub fn is_drag_active(ctx: &egui::Context) -> bool {
    let internal_file_drag = ctx.memory(|mem| 
        mem.data.get_temp::<PathBuf>(egui::Id::new("dragged_file")).is_some()
    );
    
    // Check if a group is being dragged
    let group_drag = ctx.memory(|mem| 
        mem.data.get_temp::<Group>(egui::Id::new("dragged_group")).is_some()
    );
    
    let group_drag_active = ctx.memory(|mem| 
        mem.data.get_temp::<bool>(egui::Id::new("group_drag_active")).unwrap_or(false)
    );
    
    let external_drag = !ctx.input(|i| i.raw.hovered_files.is_empty());
    
    if internal_file_drag {
        eprintln!("DEBUG: Internal file is being dragged");
    }
    
    if group_drag {
        eprintln!("DEBUG: dragged_group is present in memory");
    }
    
    if group_drag_active {
        eprintln!("DEBUG: group_drag_active flag is true");
    }

    if external_drag {
        eprintln!("DEBUG: External files are being dragged: {}", ctx.input(|i| i.raw.hovered_files.len()));
    }
    
    let is_active = internal_file_drag || external_drag || group_drag_active || group_drag;
    if is_active {
        eprintln!("DEBUG: Drag is active (internal={}, external={}, group_flag={}, group_obj={})",
            internal_file_drag, external_drag, group_drag_active, group_drag);
    }
    
    is_active
}

/// Calculate drop target information from mouse position
pub fn calculate_drop_target(
    app: &DawApp,
    ctx: &egui::Context,
    mouse_pos: egui::Pos2,
) -> Option<DropTarget> {
    eprintln!("DEBUG: Calculating drop target for mouse position: {:?}", mouse_pos);
    
    // Access the grid rect directly from memory
    if let Some(grid_rect) = ctx.memory(|mem| mem.data.get_temp::<egui::Rect>(egui::Id::new("grid_rect"))) {
        // Only process if inside the grid
        if !grid_rect.contains(mouse_pos) {
            eprintln!("DEBUG: Mouse position {:?} is outside grid rect {:?}", mouse_pos, grid_rect);
            return None;
        }
        
        eprintln!("DEBUG: Mouse position {:?} is inside grid rect {:?}", mouse_pos, grid_rect);
        
        // Calculate the beat position based on mouse position
        let h_scroll_offset = app.state.h_scroll_offset;
        let beats_per_second = app.state.bpm / 60.0;
        let pixels_per_beat = BASE_PIXELS_PER_BEAT * app.state.zoom_level;
        let seconds_per_pixel = 1.0 / (pixels_per_beat * beats_per_second);
        
        let pos_x = mouse_pos.x - grid_rect.left();
        let seconds_position = pos_x * seconds_per_pixel + h_scroll_offset;
        let beat_position = seconds_position * beats_per_second;
        
        // Calculate the track index based on mouse position
        let v_scroll_offset = app.state.v_scroll_offset;
        let pos_y = mouse_pos.y - grid_rect.top() + v_scroll_offset;
        let track_idx = (pos_y / (TRACK_HEIGHT + TRACK_SPACING)).floor() as usize;
        
        eprintln!("DEBUG: Track index calculated: {} (pos_y={}, v_scroll_offset={})", 
            track_idx, pos_y, v_scroll_offset);
        eprintln!("DEBUG: Beat position calculated: {} (pos_x={}, h_scroll_offset={})",
            beat_position, pos_x, h_scroll_offset);
        
        // Determine which track to use
        if app.state.tracks.is_empty() {
            eprintln!("DEBUG: No tracks available in the project");
            return None;
        }
        
        let track_id = if track_idx < app.state.tracks.len() {
            // Use the existing track if the index is valid
            app.state.tracks[track_idx].id
        } else {
            // If we're beyond the last track, use the last available track
            eprintln!("DEBUG: Track index {} is beyond available tracks (count={}), using last track", 
                track_idx, app.state.tracks.len());
            app.state.tracks.last().unwrap().id
        };
        
        eprintln!("DEBUG: Using track ID: {} for drop target", track_id);
        
        // Snap the beat position to the grid
        let snapped_beat = app.snap_to_grid(beat_position);
        eprintln!("DEBUG: Snapped beat position: {} (original: {})", snapped_beat, beat_position);
        
        return Some(DropTarget {
            track_id,
            beat_position: snapped_beat,
        });
    } else {
        eprintln!("DEBUG: Grid rect not found in memory");
    }
    
    None
}

/// Handle dropping external files
pub fn handle_external_file_drop(app: &mut DawApp, ctx: &egui::Context) {
    let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
    if dropped_files.is_empty() {
        return;
    }

    eprintln!("Files dropped: {:?}", dropped_files.len());
    
    // Get the current mouse position to determine the target track and position
    if let Some(mouse_pos) = ctx.input(|i| i.pointer.hover_pos()) {
        if let Some(target) = calculate_drop_target(app, ctx, mouse_pos) {
            // Process each dropped file
            for file in dropped_files {
                if let Some(path) = file.path {
                    eprintln!("Processing dropped file: {:?}", path);
                    
                    // Check if the file is an audio file
                    if is_supported_audio_file(&path) {
                        eprintln!("Adding sample at beat position: {}", target.beat_position);
                        
                        // Add the sample to the track
                        app.dispatch(DawAction::AddSampleToTrack(target.track_id, path.clone()));
                        
                        // If we just added the sample, it's the last one in the track
                        if let Some(track) = app.state.tracks.iter_mut().find(|t| t.id == target.track_id) {
                            if !track.samples.is_empty() {
                                let sample_id = track.samples.last().unwrap().id;
                                eprintln!("Moving sample ID {} to position {}", sample_id, target.beat_position);
                                
                                // Move the sample to the drop position without handling overlaps
                                if let Some(sample) = track.get_sample_mut(sample_id) {
                                    sample.grid_position = target.beat_position;
                                    sample.update_grid_times(app.state.bpm);
                                }
                            } else {
                                eprintln!("No samples found in track after adding");
                            }
                        } else {
                            eprintln!("Could not find track with ID {}", target.track_id);
                        }
                    }
                }
            }
        }
    }
}

/// Handle dropping internal files (from file browser)
pub fn handle_internal_file_drop(app: &mut DawApp, ctx: &egui::Context) {
    // Only process drops when a pointer is released
    if !ctx.input(|i| i.pointer.any_released()) {
        return;
    }
    
    // Check if a real drag occurred (mouse moved significantly)
    let drag_occurred = ctx.input(|i| i.pointer.delta().length() > 5.0);
    if !drag_occurred {
        // If no actual drag occurred, just clear any dragged file reference and return
        ctx.memory_mut(|mem| {
            mem.data.remove::<PathBuf>(egui::Id::new("dragged_file"));
        });
        return;
    }
    
    if let Some(dragged_path) = ctx.memory(|mem| mem.data.get_temp::<PathBuf>(egui::Id::new("dragged_file"))) {
        eprintln!("Internal file drag detected: {:?}", dragged_path);
        
        // Check if it's an audio file
        if !is_supported_audio_file(&dragged_path) {
            // Clear the dragged file and return
            ctx.memory_mut(|mem| {
                mem.data.remove::<PathBuf>(egui::Id::new("dragged_file"));
            });
            return;
        }
        
        // Get mouse position
        if let Some(mouse_pos) = ctx.input(|i| i.pointer.interact_pos()) {
            eprintln!("Mouse position on release: {:?}", mouse_pos);
            
            if let Some(target) = calculate_drop_target(app, ctx, mouse_pos) {
                eprintln!("Adding sample at beat position: {}", target.beat_position);
                
                // Add the sample to the track
                app.dispatch(DawAction::AddSampleToTrack(target.track_id, dragged_path));
                
                // If we just added the sample, it's the last one in the track
                if let Some(track) = app.state.tracks.iter_mut().find(|t| t.id == target.track_id) {
                    if !track.samples.is_empty() {
                        let sample_id = track.samples.last().unwrap().id;
                        eprintln!("Moving sample ID {} to position {}", sample_id, target.beat_position);
                        
                        // Move the sample to the drop position
                        app.dispatch(DawAction::MoveSample(target.track_id, sample_id, target.beat_position));
                    } else {
                        eprintln!("No samples found in track after adding");
                    }
                } else {
                    eprintln!("Could not find track with ID {}", target.track_id);
                }
            }
        }
        
        // Clear the dragged file
        ctx.memory_mut(|mem| {
            mem.data.remove::<PathBuf>(egui::Id::new("dragged_file"));
        });
    }
}

