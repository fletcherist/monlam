// Export UI components
pub mod main;
pub mod drag_drop;
pub mod file_browser;
pub mod grid;
pub mod sample;
pub mod audio_box_panel;
pub mod audio_box_item;

// Only export the modules, don't re-export main
// as it would bring in all of main's items
// This ensures we're properly using qualified names

// We can add specific exports here later if needed
