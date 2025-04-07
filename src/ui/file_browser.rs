use eframe::egui;
use std::path::{Path, PathBuf};

/// A panel that displays files in the current project folder.
/// Allows users to browse directories and drag audio files to the grid.
#[derive(Clone)]
pub struct FileBrowserPanel {
    files: Vec<(String, PathBuf, bool)>, // (filename, path, is_audio)
    current_folder: PathBuf,
    show_panel: bool,
}

impl FileBrowserPanel {
    pub fn new(project_path: Option<&Path>) -> Self {
        // Use the provided project path, or fall back to the current directory
        let current_folder = match project_path {
            Some(path) if path.exists() && path.is_dir() => path.to_path_buf(),
            _ => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };
        
        let files = Self::scan_directory(&current_folder);
        
        Self {
            files,
            current_folder,
            show_panel: true,
        }
    }
    
    fn scan_directory(path: &Path) -> Vec<(String, PathBuf, bool)> {
        let mut files = Vec::new();
        
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let file_path = entry.path();
                let file_name = file_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
                
                // Check if it's an audio file
                let is_audio = if file_path.is_file() {
                    match file_path.extension() {
                        Some(ext) => {
                            let ext = ext.to_string_lossy().to_lowercase();
                            ext == "wav" || ext == "mp3" || ext == "ogg" || ext == "flac"
                        }
                        None => false,
                    }
                } else {
                    false
                };
                
                files.push((file_name, file_path, is_audio));
            }
        }
        
        // Sort: directories first, then sort alphabetically
        files.sort_by(|a, b| {
            let a_is_dir = a.1.is_dir();
            let b_is_dir = b.1.is_dir();
            
            if a_is_dir && !b_is_dir {
                std::cmp::Ordering::Less
            } else if !a_is_dir && b_is_dir {
                std::cmp::Ordering::Greater
            } else {
                a.0.to_lowercase().cmp(&b.0.to_lowercase())
            }
        });
        
        files
    }
    
    pub fn refresh(&mut self) {
        self.files = Self::scan_directory(&self.current_folder);
    }
    
    pub fn navigate_to(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.current_folder = path;
            self.refresh();
        }
    }
    
    pub fn navigate_up(&mut self) {
        if let Some(parent) = self.current_folder.parent() {
            self.current_folder = parent.to_path_buf();
            self.refresh();
        }
    }
    
    pub fn draw(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
        let mut file_dragged = false;
        let mut navigate_to = None;
        
        // Show current path
        ui.label(format!("📂 {}", self.current_folder.display()));
        ui.separator();
        
        // File list with scrolling
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Check if we're in the default directory instead of a project folder
            let is_default_dir = self.current_folder == std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            
            if is_default_dir {
                ui.add_space(10.0);
                ui.colored_label(egui::Color32::from_rgb(255, 180, 0), 
                    "No project folder found.");
                ui.add_space(5.0);
                ui.label("Save your project first to create a project folder where you can organize your audio files.");
                ui.add_space(10.0);
                ui.separator();
            }
            
            for (i, (name, path, is_audio)) in self.files.iter().enumerate() {
                let is_dir = path.is_dir();
                let icon = if is_dir { "📁" } else if *is_audio { "🔊" } else { "📄" };
                
                let response = ui.horizontal(|ui| {
                    ui.label(format!("{} {}", icon, name));
                }).response;
                
                // Handle clicks on entries
                if response.clicked() {
                    if is_dir {
                        // Store the path to navigate to instead of doing it immediately
                        navigate_to = Some(path.clone());
                    }
                }
                
                // Make audio files draggable
                if *is_audio {
                    // Handle drag and drop manually
                    if response.dragged() {
                        file_dragged = true;
                        
                        eprintln!("Dragging audio file: {}", name);
                        
                        // Store the dragged path
                        ctx.memory_mut(|mem| {
                            mem.data.insert_temp(egui::Id::new("dragged_file"), path.clone());
                            eprintln!("Stored dragged file path in memory: {:?}", path);
                        });
                        
                        // Show drag visual
                        egui::show_tooltip_at_pointer(ctx, 
                            ui.layer_id(),
                            egui::Id::new("drag_file"),
                            |ui| {
                            ui.label(format!("{} {}", icon, name));
                        });
                    }
                    
                    // Also check if the response was clicked and the pointer is still down
                    // This helps with touch/mobile drag detection
                    if response.clicked() && ctx.input(|i| i.pointer.any_down()) {
                        eprintln!("Audio file clicked, preparing for potential drag: {}", name);
                    }
                }
            }
        });
        
        // Handle directory navigation after the loop to avoid borrowing conflicts
        if let Some(path) = navigate_to {
            self.navigate_to(path);
        }
        
        file_dragged
    }

    
    pub fn get_show_panel(&self) -> bool {
        self.show_panel
    }
    
    /// Check if the given path matches the current folder
    pub fn is_current_folder(&self, path: &Path) -> bool {
        self.current_folder == path
    }
} 