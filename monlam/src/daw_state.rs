use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackState {
    pub id: usize,
    pub name: String,
    pub audio_file: Option<PathBuf>,
    pub waveform_file: Option<PathBuf>,
    pub muted: bool,
    pub soloed: bool,
    pub recording: bool,
    pub grid_position: f32, // Position in the grid (in beats)
    pub grid_length: f32,   // Length in the grid (in beats)
    pub duration: f32,      // Duration in seconds
}

impl Default for TrackState {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            audio_file: None,
            waveform_file: None,
            muted: false,
            soloed: false,
            recording: false,
            grid_position: 0.0,
            grid_length: 0.0,
            duration: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DawState {
    pub timeline_position: f32,
    pub is_playing: bool,
    pub bpm: f32,
    pub tracks: Vec<TrackState>,
    pub grid_division: f32,
    pub last_clicked_bar: f32,
}

impl Default for DawState {
    fn default() -> Self {
        Self {
            timeline_position: 0.0,
            is_playing: false,
            bpm: 120.0,
            tracks: (1..=4)
                .map(|i| TrackState {
                    id: i - 1,
                    name: format!("Track {}", i),
                    ..Default::default()
                })
                .collect(),
            grid_division: 0.25,
            last_clicked_bar: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DawAction {
    SetTimelinePosition(f32),
    SetLastClickedBar(f32),
    TogglePlayback,
    SetBpm(f32),
    SetGridDivision(f32),
    RewindTimeline,
    ForwardTimeline(f32),
    SetTrackPosition(usize, f32),
    SetTrackLength(usize, f32),
    ToggleTrackMute(usize),
    ToggleTrackSolo(usize),
    ToggleTrackRecord(usize),
    LoadTrackAudio(usize, PathBuf, f32),
}

pub struct DawStateManager {
    state: DawState,
}

impl DawStateManager {
    pub fn new() -> Self {
        Self {
            state: DawState::default(),
        }
    }

    pub fn with_state(state: DawState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> &DawState {
        &self.state
    }

    pub fn mut_state(&mut self) -> &mut DawState {
        &mut self.state
    }

    // Apply an action to the state and return the new state
    pub fn dispatch(&mut self, action: DawAction) -> &DawState {
        match action {
            DawAction::SetTimelinePosition(position) => {
                self.state.timeline_position = position;
            }
            DawAction::SetLastClickedBar(position) => {
                self.state.last_clicked_bar = position;
                self.state.timeline_position = position;
            }
            DawAction::TogglePlayback => {
                self.state.is_playing = !self.state.is_playing;
            }
            DawAction::SetBpm(bpm) => {
                self.state.bpm = bpm;
            }
            DawAction::SetGridDivision(division) => {
                self.state.grid_division = division;
            }
            DawAction::RewindTimeline => {
                self.state.timeline_position = 0.0;
            }
            DawAction::ForwardTimeline(bars) => {
                self.state.timeline_position += bars;
            }
            DawAction::SetTrackPosition(track_id, position) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    track.grid_position = position;
                }
            }
            DawAction::SetTrackLength(track_id, length) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    track.grid_length = length;
                }
            }
            DawAction::ToggleTrackMute(track_id) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    track.muted = !track.muted;
                }
            }
            DawAction::ToggleTrackSolo(track_id) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    track.soloed = !track.soloed;
                }
            }
            DawAction::ToggleTrackRecord(track_id) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    track.recording = !track.recording;
                }
            }
            DawAction::LoadTrackAudio(track_id, path, duration) => {
                if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                    track.audio_file = Some(path.clone());
                    track.name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    track.duration = duration;
                    track.grid_length = duration * (self.state.bpm / 60.0);
                }
            }
        }
        &self.state
    }

    // Calculate grid times for a track (when it should start/end in seconds)
    pub fn calculate_track_times(&self, track_id: usize) -> Option<(f32, f32)> {
        self.state
            .tracks
            .iter()
            .find(|t| t.id == track_id)
            .map(|track| {
                let start_time = track.grid_position * (60.0 / self.state.bpm);
                let end_time = start_time + (track.grid_length * (60.0 / self.state.bpm));
                (start_time, end_time)
            })
    }

    // Check if a track should be playing at the current timeline position
    pub fn should_track_play(&self, track_id: usize) -> bool {
        if let Some((start_time, end_time)) = self.calculate_track_times(track_id) {
            // Add a small epsilon to avoid floating-point precision issues
            const EPSILON: f32 = 0.0001;
            self.state.timeline_position + EPSILON >= start_time
                && self.state.timeline_position < end_time
        } else {
            false
        }
    }

    // Calculate the relative position within a track based on the timeline position
    pub fn track_relative_position(&self, track_id: usize) -> Option<f32> {
        if let Some((start_time, _)) = self.calculate_track_times(track_id) {
            if self.should_track_play(track_id) {
                // Round to 6 decimal places to avoid floating-point precision issues
                let relative_position = self.state.timeline_position - start_time;
                Some((relative_position * 1000000.0).round() / 1000000.0)
            } else {
                None
            }
        } else {
            None
        }
    }

    // Snap a position to the grid
    pub fn snap_to_grid(&self, position: f32) -> f32 {
        // Calculate the nearest grid line
        let grid_lines = position / self.state.grid_division;
        let lower_grid_line = grid_lines.floor();
        let upper_grid_line = grid_lines.ceil();

        // Determine whether to snap to the lower or upper grid line
        if position - (lower_grid_line * self.state.grid_division)
            < (upper_grid_line * self.state.grid_division) - position
        {
            lower_grid_line * self.state.grid_division
        } else {
            upper_grid_line * self.state.grid_division
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let manager = DawStateManager::new();
        let state = manager.state();

        assert_eq!(state.timeline_position, 0.0);
        assert_eq!(state.is_playing, false);
        assert_eq!(state.bpm, 120.0);
        assert_eq!(state.tracks.len(), 4);
        assert_eq!(state.grid_division, 0.25);
    }

    #[test]
    fn test_set_timeline_position() {
        let mut manager = DawStateManager::new();
        manager.dispatch(DawAction::SetTimelinePosition(2.5));

        assert_eq!(manager.state().timeline_position, 2.5);
    }

    #[test]
    fn test_toggle_playback() {
        let mut manager = DawStateManager::new();
        assert_eq!(manager.state().is_playing, false);

        manager.dispatch(DawAction::TogglePlayback);
        assert_eq!(manager.state().is_playing, true);

        manager.dispatch(DawAction::TogglePlayback);
        assert_eq!(manager.state().is_playing, false);
    }

    #[test]
    fn test_set_track_position() {
        let mut manager = DawStateManager::new();
        manager.dispatch(DawAction::SetTrackPosition(0, 4.0));

        assert_eq!(manager.state().tracks[0].grid_position, 4.0);
    }

    #[test]
    fn test_set_track_length() {
        let mut manager = DawStateManager::new();
        manager.dispatch(DawAction::SetTrackLength(1, 8.0));

        assert_eq!(manager.state().tracks[1].grid_length, 8.0);
    }

    #[test]
    fn test_toggle_track_mute() {
        let mut manager = DawStateManager::new();
        assert_eq!(manager.state().tracks[2].muted, false);

        manager.dispatch(DawAction::ToggleTrackMute(2));
        assert_eq!(manager.state().tracks[2].muted, true);

        manager.dispatch(DawAction::ToggleTrackMute(2));
        assert_eq!(manager.state().tracks[2].muted, false);
    }

    #[test]
    fn test_load_track_audio() {
        let mut manager = DawStateManager::new();
        let path = PathBuf::from("test_audio.wav");
        manager.dispatch(DawAction::LoadTrackAudio(3, path.clone(), 5.0));

        let track = &manager.state().tracks[3];
        assert_eq!(track.audio_file, Some(path));
        assert_eq!(track.name, "test_audio.wav");
        assert_eq!(track.duration, 5.0);
        assert_eq!(track.grid_length, 10.0); // 5.0 * (120/60)
    }

    #[test]
    fn test_track_timings() {
        let mut manager = DawStateManager::new();

        // Set up track at position 2.0 with length 4.0
        manager.dispatch(DawAction::SetTrackPosition(0, 2.0));
        manager.dispatch(DawAction::SetTrackLength(0, 4.0));

        // At 120 BPM, 1 beat = 0.5 seconds
        // Track should start at 1.0 seconds (2.0 beats) and end at 3.0 seconds (6.0 beats)
        if let Some((start, end)) = manager.calculate_track_times(0) {
            assert_eq!(start, 1.0);
            assert_eq!(end, 3.0);
        }

        // Timeline before track start
        manager.dispatch(DawAction::SetTimelinePosition(0.5));
        assert_eq!(manager.should_track_play(0), false);
        assert_eq!(manager.track_relative_position(0), None);

        // Timeline at track start
        manager.dispatch(DawAction::SetTimelinePosition(1.0));
        assert_eq!(manager.should_track_play(0), true);
        assert_eq!(manager.track_relative_position(0), Some(0.0));

        // Timeline in middle of track
        manager.dispatch(DawAction::SetTimelinePosition(2.0));
        assert_eq!(manager.should_track_play(0), true);
        assert_eq!(manager.track_relative_position(0), Some(1.0));

        // Timeline after track end
        manager.dispatch(DawAction::SetTimelinePosition(3.5));
        assert_eq!(manager.should_track_play(0), false);
        assert_eq!(manager.track_relative_position(0), None);
    }

    #[test]
    fn test_snap_to_grid() {
        let mut manager = DawStateManager::new();
        manager.dispatch(DawAction::SetGridDivision(0.5));

        assert_eq!(manager.snap_to_grid(1.1), 1.0);
        assert_eq!(manager.snap_to_grid(1.3), 1.5);
        assert_eq!(manager.snap_to_grid(2.7), 2.5);
    }

    #[test]
    fn test_forward_rewind_timeline() {
        let mut manager = DawStateManager::new();

        manager.dispatch(DawAction::SetTimelinePosition(2.0));
        assert_eq!(manager.state().timeline_position, 2.0);

        manager.dispatch(DawAction::ForwardTimeline(1.5));
        assert_eq!(manager.state().timeline_position, 3.5);

        manager.dispatch(DawAction::RewindTimeline);
        assert_eq!(manager.state().timeline_position, 0.0);
    }

    // Timeline-specific tests
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
