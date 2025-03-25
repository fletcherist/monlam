use crate::audio::Audio;
use crate::daw::{DawAction, DawApp};
use std::path::PathBuf;
use std::time::Duration;

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Context;

    fn dummy_context() -> Context {
        Context::default()
    }

    #[test]
    fn test_timeline_navigation() {
        let ctx = dummy_context();

        // Create a new DAW app instance
        let mut app = DawApp::new(&eframe::CreationContext::headless(
            &ctx,
            eframe::integrations::Renderer::default(),
        ));

        // Initial state
        assert_eq!(app.state.timeline_position, 0.0);
        assert_eq!(app.state.is_playing, false);

        // Test setting timeline position
        app.dispatch(DawAction::SetTimelinePosition(2.5));
        assert_eq!(app.state.timeline_position, 2.5);

        // Test rewinding
        app.dispatch(DawAction::RewindTimeline);
        assert_eq!(app.state.timeline_position, 0.0);

        // Test forwarding
        app.dispatch(DawAction::ForwardTimeline(4.0));
        assert_eq!(app.state.timeline_position, 4.0);
    }

    #[test]
    fn test_track_positioning() {
        let ctx = dummy_context();

        // Create a new DAW app instance
        let mut app = DawApp::new(&eframe::CreationContext::headless(
            &ctx,
            eframe::integrations::Renderer::default(),
        ));

        // Set BPM and grid division
        app.dispatch(DawAction::SetBpm(120.0));
        app.dispatch(DawAction::SetGridDivision(0.25));

        // Position a track
        app.dispatch(DawAction::SetTrackPosition(0, 2.0));
        assert_eq!(app.state.tracks[0].grid_position, 2.0);

        // Verify grid_start_time was updated properly
        assert_eq!(app.state.tracks[0].grid_start_time, 1.0); // 2 beats at 120 BPM = 1.0 second

        // Change BPM and verify grid_start_time updates
        app.dispatch(DawAction::SetBpm(60.0));
        assert_eq!(app.state.tracks[0].grid_start_time, 2.0); // 2 beats at 60 BPM = 2.0 seconds
    }

    #[test]
    fn test_grid_snapping() {
        let ctx = dummy_context();

        // Create a new DAW app instance
        let mut app = DawApp::new(&eframe::CreationContext::headless(
            &ctx,
            eframe::integrations::Renderer::default(),
        ));

        // Set grid division to quarter notes (0.25)
        app.dispatch(DawAction::SetGridDivision(0.25));

        // Test snapping to grid
        assert_eq!(app.snap_to_grid(1.13), 1.0);
        assert_eq!(app.snap_to_grid(1.2), 1.25);
        assert_eq!(app.snap_to_grid(2.35), 2.25);
        assert_eq!(app.snap_to_grid(3.49), 3.5);

        // Change grid division to eighth notes (0.125)
        app.dispatch(DawAction::SetGridDivision(0.125));

        // Test snapping with new grid division
        assert_eq!(app.snap_to_grid(1.13), 1.125);
        assert_eq!(app.snap_to_grid(1.2), 1.25);
        assert_eq!(app.snap_to_grid(2.35), 2.375);
    }

    #[test]
    fn test_track_playback_status() {
        let ctx = dummy_context();

        // Create a new DAW app instance
        let mut app = DawApp::new(&eframe::CreationContext::headless(
            &ctx,
            eframe::integrations::Renderer::default(),
        ));

        // Set up a track
        app.dispatch(DawAction::SetTrackPosition(0, 2.0));
        app.dispatch(DawAction::SetTrackLength(0, 4.0));

        // Update grid times
        app.state.tracks[0].update_grid_times(app.state.bpm);

        // Track should start at 1.0 seconds and end at 3.0 seconds
        assert_eq!(app.state.tracks[0].grid_start_time, 1.0);
        assert_eq!(app.state.tracks[0].grid_end_time, 3.0);

        // Test track playback status at different timeline positions
        app.dispatch(DawAction::SetTimelinePosition(0.5));
        assert_eq!(app.should_track_play(0), false);

        app.dispatch(DawAction::SetTimelinePosition(1.0));
        assert_eq!(app.should_track_play(0), true);

        app.dispatch(DawAction::SetTimelinePosition(2.0));
        assert_eq!(app.should_track_play(0), true);

        app.dispatch(DawAction::SetTimelinePosition(3.0));
        assert_eq!(app.should_track_play(0), false);
    }

    #[test]
    fn test_track_mute_solo_record() {
        let ctx = dummy_context();

        // Create a new DAW app instance
        let mut app = DawApp::new(&eframe::CreationContext::headless(
            &ctx,
            eframe::integrations::Renderer::default(),
        ));

        // Test mute toggle
        assert_eq!(app.state.tracks[0].muted, false);
        app.dispatch(DawAction::ToggleTrackMute(0));
        assert_eq!(app.state.tracks[0].muted, true);
        app.dispatch(DawAction::ToggleTrackMute(0));
        assert_eq!(app.state.tracks[0].muted, false);

        // Test solo toggle
        assert_eq!(app.state.tracks[0].soloed, false);
        app.dispatch(DawAction::ToggleTrackSolo(0));
        assert_eq!(app.state.tracks[0].soloed, true);
        app.dispatch(DawAction::ToggleTrackSolo(0));
        assert_eq!(app.state.tracks[0].soloed, false);

        // Test record toggle
        assert_eq!(app.state.tracks[0].recording, false);
        app.dispatch(DawAction::ToggleTrackRecord(0));
        assert_eq!(app.state.tracks[0].recording, true);
        app.dispatch(DawAction::ToggleTrackRecord(0));
        assert_eq!(app.state.tracks[0].recording, false);
    }
}
