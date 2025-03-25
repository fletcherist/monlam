use std::path::PathBuf;

// Import from the parent crate
use monlam::daw_state::{DawAction, DawStateManager};

fn main() {
    println!("DAW State Manager CLI Test");

    // Create a new DAW state manager
    let mut manager = DawStateManager::new();

    // Example operations

    // 1. Load some audio tracks
    println!("\nLoading tracks...");
    manager.dispatch(DawAction::LoadTrackAudio(
        0,
        PathBuf::from("example1.wav"),
        3.0,
    ));
    manager.dispatch(DawAction::LoadTrackAudio(
        1,
        PathBuf::from("example2.wav"),
        2.5,
    ));

    // 2. Position tracks on the timeline
    println!("\nPositioning tracks...");
    manager.dispatch(DawAction::SetTrackPosition(0, 0.0)); // Track 0 at beat 0
    manager.dispatch(DawAction::SetTrackPosition(1, 4.0)); // Track 1 at beat 4

    // 3. Set BPM
    println!("\nSetting BPM to 120...");
    manager.dispatch(DawAction::SetBpm(120.0));

    // 4. Print track information
    println!("\nTrack Information:");
    for (i, track) in manager.state().tracks.iter().enumerate() {
        if track.audio_file.is_some() {
            let (start_time, end_time) = manager.calculate_track_times(i).unwrap();
            println!(
                "Track {}: '{}' starts at {}s, ends at {}s",
                i, track.name, start_time, end_time
            );
        }
    }

    // 5. Simulate timeline scrubbing
    println!("\nSimulating timeline scrubbing...");
    for position in [0.0, 1.0, 2.0, 3.0, 4.0, 5.0] {
        manager.dispatch(DawAction::SetTimelinePosition(position));
        println!("\nTimeline position: {}s", position);

        // Print which tracks should be playing
        for i in 0..manager.state().tracks.len() {
            let should_play = manager.should_track_play(i);
            if should_play {
                let relative_pos = manager.track_relative_position(i).unwrap();
                println!(
                    "  Track {} is playing at relative position: {}s",
                    i, relative_pos
                );
            } else {
                println!("  Track {} is not playing", i);
            }
        }
    }

    // 6. Test moving a track
    println!("\nMoving Track 1 to beat 8...");
    manager.dispatch(DawAction::SetTrackPosition(1, 8.0));

    let (new_start, new_end) = manager.calculate_track_times(1).unwrap();
    println!("Track 1 now starts at {}s, ends at {}s", new_start, new_end);

    println!("\nDone!");
}
