use crate::group::Group;
use crate::ui::main::{SAMPLE_BORDER_COLOR, TRACK_TEXT_COLOR, WAVEFORM_COLOR};
use eframe::egui;
use std::path::{Path, PathBuf};

/// A panel that displays and manages Groups (audio containers).
#[derive(Clone)]
pub struct GroupPanel {
    groups: Vec<Group>,
    current_folder: PathBuf,
    selected_group_idx: Option<usize>,
    renaming_group_idx: Option<usize>,
    new_name_buffer: String,
    new_group_name: String,
    show_create_dialog: bool,
}

impl GroupPanel {
    pub fn new(project_path: Option<&Path>) -> Self {
        // Use the provided project path, or fall back to the current directory
        let current_folder = match project_path {
            Some(path) if path.exists() && path.is_dir() => path.to_path_buf(),
            _ => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };
        
        let groups = Self::scan_groups(&current_folder);
        
        Self {
            groups,
            current_folder,
            selected_group_idx: None,
            renaming_group_idx: None,
            new_name_buffer: String::new(),
            new_group_name: String::new(),
            show_create_dialog: false,
        }
    }
    
    /// Scan a directory for Groups and load them
    fn scan_groups(path: &Path) -> Vec<Group> {
        let mut groups = Vec::new();
        
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let dir_path = entry.path();
                
                // Check if it's a directory that might be a Group
                if dir_path.is_dir() {
                    let render_path = dir_path.join("render.wav");
                    
                    // If the directory has a render.wav file, it's a Group
                    if render_path.exists() || true { // For now, consider all directories as potential Groups
                        if let Ok(group) = Group::load(&dir_path) {
                            groups.push(group);
                        }
                    }
                }
            }
        }
        
        // Sort alphabetically
        groups.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        
        groups
    }
    
    /// Refresh the Group list
    pub fn refresh(&mut self) {
        self.groups = Self::scan_groups(&self.current_folder);
    }
    
    /// Draw the Group panel UI
    pub fn draw(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> Option<Group> {
        let mut group_to_open = None;
        
        ui.heading("Groups");
        ui.separator();
        
        // Create Group button
        if ui.button("Create Group").clicked() {
            self.show_create_dialog = true;
            self.new_group_name.clear();
        }
        
        // Create Group dialog
        if self.show_create_dialog {
            egui::Window::new("Create Group")
                .fixed_size([300.0, 100.0])
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Group Name:");
                    ui.text_edit_singleline(&mut self.new_group_name);
                    
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.show_create_dialog = false;
                        }
                        
                        if ui.button("Create").clicked() {
                            if !self.new_group_name.trim().is_empty() {
                                // Validate name (no slashes)
                                if !self.new_group_name.contains('/') {
                                    match Group::new(&self.new_group_name, &self.current_folder) {
                                        Ok(group) => {
                                            // Add the new Group to the list
                                            self.groups.push(group);
                                            self.show_create_dialog = false;
                                            self.new_group_name.clear();
                                        }
                                        Err(e) => {
                                            // Show error
                                            eprintln!("Error creating Group: {}", e);
                                        }
                                    }
                                } else {
                                    eprintln!("Group name cannot contain '/'");
                                }
                            }
                        }
                    });
                });
        }
        
        ui.separator();
        
        // Group list with scrolling
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Check if we're in the default directory instead of a project folder
            let is_default_dir = self.current_folder == std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            
            if is_default_dir {
                ui.add_space(10.0);
                ui.colored_label(egui::Color32::from_rgb(255, 180, 0), 
                    "No project folder found.");
                ui.add_space(5.0);
                ui.label("Save your project first to create a project folder where you can create Groups.");
                ui.add_space(10.0);
                ui.separator();
            }
            
            // Track operations to perform after the iteration
            let mut group_to_rename: Option<(usize, String)> = None;
            let mut group_to_delete: Option<usize> = None;
            
            for (i, group) in self.groups.iter().enumerate() {
                let is_selected = self.selected_group_idx == Some(i);
                let is_renaming = self.renaming_group_idx == Some(i);
                
                if is_renaming {
                    ui.horizontal(|ui| {
                        if ui.text_edit_singleline(&mut self.new_name_buffer).lost_focus() {
                            if !self.new_name_buffer.trim().is_empty() && !self.new_name_buffer.contains('/') {
                                // Store the rename operation for later
                                group_to_rename = Some((i, self.new_name_buffer.clone()));
                            }
                            self.renaming_group_idx = None;
                        }
                        
                        if ui.button("✓").clicked() {
                            if !self.new_name_buffer.trim().is_empty() && !self.new_name_buffer.contains('/') {
                                // Store the rename operation for later
                                group_to_rename = Some((i, self.new_name_buffer.clone()));
                            }
                            self.renaming_group_idx = None;
                        }
                    });
                } else {
                    let response = ui.selectable_label(is_selected, format!("📦 {}", group.name));
                    
                    if response.clicked() {
                        self.selected_group_idx = Some(i);
                    }
                    
                    // Double-click to open
                    if response.double_clicked() {
                        group_to_open = Some(group.clone());
                    }
                    
                    // Context menu
                    response.context_menu(|ui| {
                        if ui.button("Open").clicked() {
                            group_to_open = Some(group.clone());
                            ui.close_menu();
                        }
                        
                        if ui.button("Rename").clicked() {
                            self.renaming_group_idx = Some(i);
                            self.new_name_buffer = group.name.clone();
                            ui.close_menu();
                        }
                        
                        if ui.button("Delete").clicked() {
                            // Mark for deletion after the loop
                            group_to_delete = Some(i);
                            ui.close_menu();
                        }
                    });
                }
            }
            
            // Process rename operation after the loop
            if let Some((i, new_name)) = group_to_rename {
                if i < self.groups.len() {
                    let mut group_clone = self.groups[i].clone();
                    if let Err(e) = group_clone.rename(&new_name, &self.current_folder) {
                        eprintln!("Error renaming Group: {}", e);
                    } else {
                        // Replace the group with the renamed one
                        self.groups[i] = group_clone;
                    }
                }
            }
            
            // Process delete operation after the loop
            if let Some(i) = group_to_delete {
                if i < self.groups.len() {
                    if let Ok(_) = std::fs::remove_dir_all(&self.groups[i].path) {
                        // Remove from the list
                        self.groups.remove(i);
                        if let Some(selected) = self.selected_group_idx {
                            if selected == i || selected >= self.groups.len() {
                                self.selected_group_idx = None;
                            }
                        }
                    } else {
                        eprintln!("Failed to delete Group: {}", self.groups[i].name);
                    }
                }
            }
        });
        
        group_to_open
    }
    
    /// Make the Group draggable with a waveform preview
    pub fn make_group_draggable(&self, ui: &mut egui::Ui, group: &Group, ctx: &egui::Context) -> bool {
        let mut dragged = false;
        
        // Create a preview of the Group
        let preview_rect = ui.available_rect_before_wrap().shrink(4.0);
        let preview_response = ui.allocate_rect(preview_rect, egui::Sense::click_and_drag());
        
        if preview_response.dragged() {
            dragged = true;
            
            // Store the dragged Group
            ctx.memory_mut(|mem| {
                mem.data.insert_temp(egui::Id::new("dragged_group"), group.clone());
            });
            
            // Show drag visual with waveform
            egui::show_tooltip_at_pointer(ctx, ui.layer_id(), egui::Id::new("group_preview"), |ui| {
                let preview_width = 200.0;
                let preview_height = 60.0;
                
                let (rect, response) = ui.allocate_exact_size(
                    egui::Vec2::new(preview_width, preview_height),
                    egui::Sense::hover(),
                );
                
                let painter = ui.painter();
                
                // Draw Group background
                painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(60, 60, 70));
                painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, SAMPLE_BORDER_COLOR), egui::StrokeKind::Inside);
                
                // Draw Group name
                painter.text(
                    egui::Pos2::new(rect.left() + 4.0, rect.top() + 12.0),
                    egui::Align2::LEFT_TOP,
                    &group.name,
                    egui::FontId::proportional(10.0),
                    TRACK_TEXT_COLOR,
                );
                
                // Draw waveform if available
                if !group.waveform.is_empty() {
                    let waveform_height = rect.height() * 0.6;
                    let center_y = rect.center().y;
                    
                    let samples_per_pixel = (group.waveform.len() as f32 / rect.width()).max(1.0);
                    
                    for x in 0..rect.width() as usize {
                        let sample_idx = (x as f32 * samples_per_pixel) as usize;
                        if sample_idx < group.waveform.len() {
                            let amplitude = group.waveform[sample_idx];
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