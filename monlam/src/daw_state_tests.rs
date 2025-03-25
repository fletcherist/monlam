#[cfg(test)]
mod timeline_tests {
    use crate::daw_state::{DawAction, DawStateManager};
    use std::path::PathBuf;

    #[test]
    fn test_moving_track_on_timeline() {
        let mut manager = DawStateManager::new();

        // Set up track with audio
        let audio_path = PathBuf::from("test_audio.wav");
        manager.dispatch(DawAction::LoadTrackAudio(0, audio_path, 5.0));

        // Initial position is 0.0
        assert_eq!(manager.state().tracks[0].grid_position, 0.0);

        // Move track to position 4.0
        manager.dispatch(DawAction::SetTrackPosition(0, 4.0));
        assert_eq!(manager.state().tracks[0].grid_position, 4.0);

        // Verify track timing calculations
        if let Some((start, end)) = manager.calculate_track_times(0) {
            assert_eq!(start, 2.0); // 4.0 beats at 120bpm = 2.0 seconds
            assert_eq!(end, 2.0 + 5.0 * (120.0 / 60.0) * (60.0 / 120.0));
        }
    }

    #[test]
    fn test_snapping_to_grid() {
        let mut manager = DawStateManager::new();

        // Set grid division to quarter notes (0.25)
        manager.dispatch(DawAction::SetGridDivision(0.25));

        // Load a track
        let audio_path = PathBuf::from("test_audio.wav");
        manager.dispatch(DawAction::LoadTrackAudio(0, audio_path, 5.0));

        // Test snapping various positions
        assert_eq!(manager.snap_to_grid(1.13), 1.0);
        assert_eq!(manager.snap_to_grid(1.2), 1.25);
        assert_eq!(manager.snap_to_grid(2.35), 2.25);
        assert_eq!(manager.snap_to_grid(3.49), 3.5);

        // Change grid division to eighth notes (0.125)
        manager.dispatch(DawAction::SetGridDivision(0.125));

        // Test snapping with new grid division
        assert_eq!(manager.snap_to_grid(1.13), 1.125);
        assert_eq!(manager.snap_to_grid(1.2), 1.25);
        assert_eq!(manager.snap_to_grid(2.35), 2.375);
    }

    #[test]
    fn test_overlapping_tracks() {
        let mut manager = DawStateManager::new();

        // Set up two tracks
        let audio_path1 = PathBuf::from("track1.wav");
        let audio_path2 = PathBuf::from("track2.wav");

        manager.dispatch(DawAction::LoadTrackAudio(0, audio_path1, 2.0));
        manager.dispatch(DawAction::LoadTrackAudio(1, audio_path2, 3.0));

        // Position track1 at beat 2 and track2 at beat 4
        manager.dispatch(DawAction::SetTrackPosition(0, 2.0));
        manager.dispatch(DawAction::SetTrackPosition(1, 4.0));

        // Check track start/end times
        let (track1_start, track1_end) = manager.calculate_track_times(0).unwrap();
        let (track2_start, track2_end) = manager.calculate_track_times(1).unwrap();

        assert_eq!(track1_start, 1.0); // 2.0 beats at 120bpm = 1.0 second
        assert_eq!(track1_end, 1.0 + 2.0 * (120.0 / 60.0) * (60.0 / 120.0));

        assert_eq!(track2_start, 2.0); // 4.0 beats at 120bpm = 2.0 seconds
        assert_eq!(track2_end, 2.0 + 3.0 * (120.0 / 60.0) * (60.0 / 120.0));

        // Test playback at different timeline positions
        manager.dispatch(DawAction::SetTimelinePosition(0.5));
        assert_eq!(manager.should_track_play(0), false);
        assert_eq!(manager.should_track_play(1), false);

        manager.dispatch(DawAction::SetTimelinePosition(1.5));
        assert_eq!(manager.should_track_play(0), true);
        assert_eq!(manager.should_track_play(1), false);

        manager.dispatch(DawAction::SetTimelinePosition(2.5));
        assert_eq!(manager.should_track_play(0), false);
        assert_eq!(manager.should_track_play(1), true);

        manager.dispatch(DawAction::SetTimelinePosition(3.5));
        assert_eq!(manager.should_track_play(0), false);
        assert_eq!(manager.should_track_play(1), true);

        manager.dispatch(DawAction::SetTimelinePosition(5.0));
        assert_eq!(manager.should_track_play(0), false);
        assert_eq!(manager.should_track_play(1), false);
    }

    #[test]
    fn test_relative_position_calculation() {
        let mut manager = DawStateManager::new();

        // Set up track with audio
        let audio_path = PathBuf::from("test_audio.wav");
        manager.dispatch(DawAction::LoadTrackAudio(0, audio_path, 4.0));
        manager.dispatch(DawAction::SetTrackPosition(0, 2.0));

        // Calculate relative positions at different timeline locations

        // Before track start
        manager.dispatch(DawAction::SetTimelinePosition(0.5));
        assert_eq!(manager.track_relative_position(0), None);

        // At track start
        manager.dispatch(DawAction::SetTimelinePosition(1.0));
        assert_eq!(manager.track_relative_position(0), Some(0.0));

        // In the middle of the track
        manager.dispatch(DawAction::SetTimelinePosition(2.0));
        assert_eq!(manager.track_relative_position(0), Some(1.0));

        // Near the end of the track
        manager.dispatch(DawAction::SetTimelinePosition(2.9));
        assert_eq!(manager.track_relative_position(0), Some(1.9));

        // After track end
        manager.dispatch(DawAction::SetTimelinePosition(3.5));
        assert_eq!(manager.track_relative_position(0), None);
    }

    #[test]
    fn test_changing_bpm_affects_track_timing() {
        let mut manager = DawStateManager::new();

        // Set up track with audio
        let audio_path = PathBuf::from("test_audio.wav");
        manager.dispatch(DawAction::LoadTrackAudio(0, audio_path, 2.0));
        manager.dispatch(DawAction::SetTrackPosition(0, 4.0));

        // At 120 BPM
        let (start_120, end_120) = manager.calculate_track_times(0).unwrap();
        assert_eq!(start_120, 2.0); // 4.0 beats at 120bpm = 2.0 seconds

        // Change BPM to 60
        manager.dispatch(DawAction::SetBpm(60.0));

        // At 60 BPM
        let (start_60, end_60) = manager.calculate_track_times(0).unwrap();
        assert_eq!(start_60, 4.0); // 4.0 beats at 60bpm = 4.0 seconds

        // Grid length should remain the same in beats
        assert_eq!(manager.state().tracks[0].grid_length, 2.0 * (120.0 / 60.0));
    }
}
