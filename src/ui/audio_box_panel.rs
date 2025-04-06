use crate::audio_box::AudioBox;
use crate::ui::main::{SAMPLE_BORDER_COLOR, TRACK_HEIGHT, TRACK_TEXT_COLOR, WAVEFORM_COLOR};
use eframe::egui;
use std::path::{Path, PathBuf};

/// A panel that displays and manages Boxes (audio containers).
#[derive(Clone)]
pub struct AudioBoxPanel {
    boxes: Vec<AudioBox>,
    current_folder: PathBuf,
    show_panel: bool,
    selected_box_idx: Option<usize>,
    renaming_box_idx: Option<usize>,
    new_name_buffer: String,
    new_box_name: String,
    show_create_dialog: bool,
}

impl AudioBoxPanel {
    pub fn new(project_path: Option<&Path>) -> Self {
        // Use the provided project path, or fall back to the current directory
        let current_folder = match project_path {
            Some(path) if path.exists() && path.is_dir() => path.to_path_buf(),
            _ => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };
        
        let boxes = Self::scan_boxes(&current_folder);
        
        Self {
            boxes,
            current_folder,
            show_panel: true,
            selected_box_idx: None,
            renaming_box_idx: None,
            new_name_buffer: String::new(),
            new_box_name: String::new(),
            show_create_dialog: false,
        }
    }
    
    /// Scan a directory for Boxes and load them
    fn scan_boxes(path: &Path) -> Vec<AudioBox> {
        let mut boxes = Vec::new();
        
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let dir_path = entry.path();
                
                // Check if it's a directory that might be a Box
                if dir_path.is_dir() {
                    let render_path = dir_path.join("render.wav");
                    
                    // If the directory has a render.wav file, it's a Box
                    if render_path.exists() || true { // For now, consider all directories as potential Boxes
                        if let Ok(audio_box) = AudioBox::load(&dir_path) {
                            boxes.push(audio_box);
                        }
                    }
                }
            }
        }
        
        // Sort alphabetically
        boxes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        
        boxes
    }
    
    /// Refresh the Box list
    pub fn refresh(&mut self) {
        self.boxes = Self::scan_boxes(&self.current_folder);
    }
    
    /// Draw the Box panel UI
    pub fn draw(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> Option<AudioBox> {
        let mut box_to_open = None;
        
        ui.heading("Boxes");
        ui.separator();
        
        // Create Box button
        if ui.button("Create Box").clicked() {
            self.show_create_dialog = true;
            self.new_box_name.clear();
        }
        
        // Create Box dialog
        if self.show_create_dialog {
            egui::Window::new("Create Box")
                .fixed_size([300.0, 100.0])
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Box Name:");
                    ui.text_edit_singleline(&mut self.new_box_name);
                    
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.show_create_dialog = false;
                        }
                        
                        if ui.button("Create").clicked() {
                            if !self.new_box_name.trim().is_empty() {
                                // Validate name (no slashes)
                                if !self.new_box_name.contains('/') {
                                    match AudioBox::new(&self.new_box_name, &self.current_folder) {
                                        Ok(audio_box) => {
                                            // Add the new Box to the list
                                            self.boxes.push(audio_box);
                                            self.show_create_dialog = false;
                                            self.new_box_name.clear();
                                        }
                                        Err(e) => {
                                            // Show error
                                            eprintln!("Error creating Box: {}", e);
                                        }
                                    }
                                } else {
                                    eprintln!("Box name cannot contain '/'");
                                }
                            }
                        }
                    });
                });
        }
        
        ui.separator();
        
        // Box list with scrolling
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Check if we're in the default directory instead of a project folder
            let is_default_dir = self.current_folder == std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            
            if is_default_dir {
                ui.add_space(10.0);
                ui.colored_label(egui::Color32::from_rgb(255, 180, 0), 
                    "No project folder found.");
                ui.add_space(5.0);
                ui.label("Save your project first to create a project folder where you can create Boxes.");
                ui.add_space(10.0);
                ui.separator();
            }
            
            // Track operations to perform after the iteration
            let mut box_to_rename: Option<(usize, String)> = None;
            let mut box_to_delete: Option<usize> = None;
            
            for (i, audio_box) in self.boxes.iter().enumerate() {
                let is_selected = self.selected_box_idx == Some(i);
                let is_renaming = self.renaming_box_idx == Some(i);
                
                if is_renaming {
                    ui.horizontal(|ui| {
                        if ui.text_edit_singleline(&mut self.new_name_buffer).lost_focus() {
                            if !self.new_name_buffer.trim().is_empty() && !self.new_name_buffer.contains('/') {
                                // Store the rename operation for later
                                box_to_rename = Some((i, self.new_name_buffer.clone()));
                            }
                            self.renaming_box_idx = None;
                        }
                        
                        if ui.button("✓").clicked() {
                            if !self.new_name_buffer.trim().is_empty() && !self.new_name_buffer.contains('/') {
                                // Store the rename operation for later
                                box_to_rename = Some((i, self.new_name_buffer.clone()));
                            }
                            self.renaming_box_idx = None;
                        }
                    });
                } else {
                    let response = ui.selectable_label(is_selected, format!("📦 {}", audio_box.name));
                    
                    if response.clicked() {
                        self.selected_box_idx = Some(i);
                    }
                    
                    // Double-click to open
                    if response.double_clicked() {
                        box_to_open = Some(audio_box.clone());
                    }
                    
                    // Context menu
                    response.context_menu(|ui| {
                        if ui.button("Open").clicked() {
                            box_to_open = Some(audio_box.clone());
                            ui.close_menu();
                        }
                        
                        if ui.button("Rename").clicked() {
                            self.renaming_box_idx = Some(i);
                            self.new_name_buffer = audio_box.name.clone();
                            ui.close_menu();
                        }
                        
                        if ui.button("Delete").clicked() {
                            // Mark for deletion after the loop
                            box_to_delete = Some(i);
                            ui.close_menu();
                        }
                    });
                }
            }
            
            // Process rename operation after the loop
            if let Some((i, new_name)) = box_to_rename {
                if i < self.boxes.len() {
                    let mut box_clone = self.boxes[i].clone();
                    if let Err(e) = box_clone.rename(&new_name, &self.current_folder) {
                        eprintln!("Error renaming Box: {}", e);
                    } else {
                        // Replace the box with the renamed one
                        self.boxes[i] = box_clone;
                    }
                }
            }
            
            // Process delete operation after the loop
            if let Some(i) = box_to_delete {
                if i < self.boxes.len() {
                    if let Ok(_) = std::fs::remove_dir_all(&self.boxes[i].path) {
                        // Remove from the list
                        self.boxes.remove(i);
                        if let Some(selected) = self.selected_box_idx {
                            if selected == i || selected >= self.boxes.len() {
                                self.selected_box_idx = None;
                            }
                        }
                    } else {
                        eprintln!("Failed to delete Box: {}", self.boxes[i].name);
                    }
                }
            }
        });
        
        box_to_open
    }
    
    /// Make the Box draggable with a waveform preview
    pub fn make_box_draggable(&self, ui: &mut egui::Ui, audio_box: &AudioBox, ctx: &egui::Context) -> bool {
        let mut dragged = false;
        
        // Create a preview of the Box
        let preview_rect = ui.available_rect_before_wrap().shrink(4.0);
        let preview_response = ui.allocate_rect(preview_rect, egui::Sense::click_and_drag());
        
        if preview_response.dragged() {
            dragged = true;
            
            // Store the dragged Box
            ctx.memory_mut(|mem| {
                mem.data.insert_temp(egui::Id::new("dragged_box"), audio_box.clone());
            });
            
            // Show drag visual with waveform
            egui::show_tooltip_at_pointer(ctx, egui::Id::new("box_preview"), |ui| {
                let preview_width = 200.0;
                let preview_height = 60.0;
                
                let (rect, response) = ui.allocate_exact_size(
                    egui::Vec2::new(preview_width, preview_height),
                    egui::Sense::hover(),
                );
                
                let painter = ui.painter();
                
                // Draw Box background
                painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(60, 60, 70));
                painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, SAMPLE_BORDER_COLOR));
                
                // Draw Box name
                painter.text(
                    egui::Pos2::new(rect.left() + 4.0, rect.top() + 12.0),
                    egui::Align2::LEFT_TOP,
                    &audio_box.name,
                    egui::FontId::proportional(10.0),
                    TRACK_TEXT_COLOR,
                );
                
                // Draw waveform if available
                if !audio_box.waveform.is_empty() {
                    let waveform_height = rect.height() * 0.6;
                    let center_y = rect.center().y;
                    
                    let samples_per_pixel = (audio_box.waveform.len() as f32 / rect.width()).max(1.0);
                    
                    for x in 0..rect.width() as usize {
                        let sample_idx = (x as f32 * samples_per_pixel) as usize;
                        if sample_idx < audio_box.waveform.len() {
                            let amplitude = audio_box.waveform[sample_idx];
                            let y_offset = amplitude * (waveform_height / 2.0);
                            
                            // Draw waveform line
                            painter.line_segment(
                                [
                                    egui::Pos2::new(rect.left() + x as f32, center_y - y_offset),
                                    egui::Pos2::new(rect.left() + x as f32, center_y + y_offset),
                                ],
                                egui::Stroke::new(1.0, WAVEFORM_COLOR),
                            );
                        }
                    }
                }
            });
        }
        
        dragged
    }
    
    pub fn set_show_panel(&mut self, show: bool) {
        self.show_panel = show;
    }
    
    pub fn get_show_panel(&self) -> bool {
        self.show_panel
    }
    
    /// Check if the given path matches the current folder
    pub fn is_current_folder(&self, path: &Path) -> bool {
        self.current_folder == path
    }
    
    /// Update the current folder
    pub fn set_current_folder(&mut self, path: &Path) {
        if path.exists() && path.is_dir() {
            self.current_folder = path.to_path_buf();
            self.refresh();
        }
    }
} 