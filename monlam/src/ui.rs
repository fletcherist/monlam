use crate::daw::DawApp;
use eframe::egui;
use egui::{Color32, Key, RichText, Stroke};
use rfd::FileDialog;
use std::sync::mpsc::channel;

// UI Constants
const TIMELINE_HEIGHT: f32 = 60.0;
const TRACK_HEIGHT: f32 = 100.0;
const TRACK_SPACING: f32 = 8.0;
const GRID_BACKGROUND: Color32 = Color32::from_rgb(30, 30, 30);
const BAR_LINE_COLOR: Color32 = Color32::from_rgb(60, 60, 60);
const BEAT_LINE_COLOR: Color32 = Color32::from_rgb(50, 50, 50);
const PLAYHEAD_COLOR: Color32 = Color32::from_rgb(255, 50, 50);
const TRACK_BORDER_COLOR: Color32 = Color32::from_rgb(60, 60, 60);
const TRACK_TEXT_COLOR: Color32 = Color32::from_rgb(200, 200, 200);
const WAVEFORM_COLOR: Color32 = Color32::from_rgb(100, 100, 100);

// UI Components
struct TransportControls<'a> {
    is_playing: bool,
    bpm: f32,
    grid_division: f32,
    on_rewind: &'a mut dyn FnMut(),
    on_play_pause: &'a mut dyn FnMut(),
    on_forward: &'a mut dyn FnMut(),
    on_bpm_change: &'a mut dyn FnMut(f32),
    on_grid_change: &'a mut dyn FnMut(f32),
    on_save: &'a mut dyn FnMut(),
    on_load: &'a mut dyn FnMut(),
}

impl<'a> TransportControls<'a> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            // Transport controls
            ui.horizontal(|ui| {
                if ui.button(RichText::new("⏮").size(20.0)).clicked() {
                    (self.on_rewind)();
                }
                if ui
                    .button(RichText::new(if self.is_playing { "⏸" } else { "▶" }).size(20.0))
                    .clicked()
                {
                    (self.on_play_pause)();
                }
                if ui.button(RichText::new("⏭").size(20.0)).clicked() {
                    (self.on_forward)();
                }
            });
            ui.add_space(16.0);

            // BPM control
            ui.label(RichText::new("BPM:").size(14.0));
            let mut bpm = self.bpm;
            if ui
                .add(
                    egui::DragValue::new(&mut bpm)
                        .speed(1.0)
                        .clamp_range(20.0..=240.0),
                )
                .changed()
            {
                (self.on_bpm_change)(bpm);
            }
            ui.add_space(16.0);

            // Grid control
            ui.label(RichText::new("Grid:").size(14.0));
            let mut grid = self.grid_division;
            if ui
                .add(
                    egui::DragValue::new(&mut grid)
                        .speed(0.25)
                        .clamp_range(0.25..=4.0),
                )
                .changed()
            {
                (self.on_grid_change)(grid);
            }
            ui.add_space(16.0);

            // Save/load buttons
            if ui.button(RichText::new("💾 Save").size(14.0)).clicked() {
                (self.on_save)();
            }
            if ui.button(RichText::new("📂 Open").size(14.0)).clicked() {
                (self.on_load)();
            }
            ui.add_space(8.0);
        });
    }
}

struct Timeline<'a> {
    timeline_position: f32,
    bpm: f32,
    grid_division: f32,
    on_timeline_click: &'a mut dyn FnMut(f32),
    last_clicked_bar: f32,
}

impl<'a> Timeline<'a> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let (timeline_response, timeline_painter) = ui.allocate_painter(
            egui::vec2(ui.available_width(), TIMELINE_HEIGHT),
            egui::Sense::click_and_drag(),
        );

        // Draw timeline background
        timeline_painter.rect_filled(timeline_response.rect, 0.0, Color32::from_rgb(40, 40, 40));

        let timeline_rect = timeline_response.rect;
        let pixels_per_beat = timeline_rect.width() / (8.0 * 4.0);

        // Draw grid lines
        self.draw_grid_lines(&timeline_painter, timeline_rect, pixels_per_beat);

        // Draw playhead
        self.draw_playhead(&timeline_painter, timeline_rect, pixels_per_beat);

        // Draw time markers
        self.draw_time_markers(&timeline_painter, timeline_rect, pixels_per_beat);

        // Handle timeline interaction
        if timeline_response.dragged() || timeline_response.clicked() {
            if let Some(pos) = timeline_response.interact_pointer_pos() {
                let click_x = pos.x - timeline_rect.left();
                let click_beats = click_x / pixels_per_beat;
                // Snap to bar (4 beats per bar)
                let snapped_beats = (click_beats / 4.0).round() * 4.0;
                self.last_clicked_bar = snapped_beats;
                (self.on_timeline_click)(snapped_beats);
            }
        }
    }

    fn draw_grid_lines(&self, painter: &egui::Painter, rect: egui::Rect, pixels_per_beat: f32) {
        // Draw bar lines
        for bar in 0..=8 {
            let x = rect.left() + (bar as f32 * 4.0 * pixels_per_beat);
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(2.0, BAR_LINE_COLOR),
            );
        }

        // Draw beat lines
        for beat in 0..=(8.0 * 4.0 * self.grid_division) as i32 {
            let x = rect.left() + (beat as f32 * pixels_per_beat / self.grid_division);
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(1.0, BEAT_LINE_COLOR),
            );
        }
    }

    fn draw_playhead(&self, painter: &egui::Painter, rect: egui::Rect, pixels_per_beat: f32) {
        let playhead_x = rect.left() + (self.timeline_position * pixels_per_beat);
        painter.line_segment(
            [
                egui::pos2(playhead_x, rect.top()),
                egui::pos2(playhead_x, rect.bottom()),
            ],
            Stroke::new(2.0, PLAYHEAD_COLOR),
        );
    }

    fn draw_time_markers(&self, painter: &egui::Painter, rect: egui::Rect, pixels_per_beat: f32) {
        for bar in 0..=8 {
            let x = rect.left() + (bar as f32 * 4.0 * pixels_per_beat);
            let bar_number = bar + 1;
            painter.text(
                egui::pos2(x + 2.0, rect.top() + 15.0),
                egui::Align2::LEFT_TOP,
                bar_number.to_string(),
                egui::FontId::proportional(12.0),
                TRACK_TEXT_COLOR,
            );
        }
    }
}

struct Grid<'a> {
    timeline_position: f32,
    bpm: f32,
    grid_division: f32,
    tracks: Vec<(String, bool, bool, bool, f32, f32, Vec<f32>)>,
    on_track_drag: &'a mut dyn FnMut(usize, f32),
}

impl<'a> Grid<'a> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let available_height = ui.available_height() - 150.0 - TIMELINE_HEIGHT;
        let (grid_response, painter) = ui.allocate_painter(
            egui::vec2(ui.available_width(), available_height),
            egui::Sense::click_and_drag(),
        );

        if grid_response.rect.width() > 0.0 {
            let rect = grid_response.rect;
            let width = rect.width();

            // Draw grid background
            painter.rect_filled(rect, 0.0, GRID_BACKGROUND);

            // Draw grid lines
            let beats_per_bar = 4.0;
            let total_bars = 8.0;
            let total_beats = total_bars * beats_per_bar;
            let pixels_per_beat = width / total_beats;

            // Draw bar lines
            for bar in 0..=total_bars as i32 {
                let x = rect.left() + (bar as f32 * beats_per_bar * pixels_per_beat);
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    Stroke::new(2.0, BAR_LINE_COLOR),
                );
            }

            // Draw beat lines
            for beat in 0..=(total_beats * self.grid_division) as i32 {
                let x = rect.left() + (beat as f32 * pixels_per_beat / self.grid_division);
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    Stroke::new(1.0, BEAT_LINE_COLOR),
                );
            }

            // Draw tracks
            self.draw_tracks(&painter, rect, pixels_per_beat, &grid_response);

            // Draw playhead
            let playhead_x = rect.left() + (self.timeline_position * pixels_per_beat);
            painter.line_segment(
                [
                    egui::pos2(playhead_x, rect.top()),
                    egui::pos2(playhead_x, rect.bottom()),
                ],
                Stroke::new(2.0, PLAYHEAD_COLOR),
            );
        }
    }

    fn draw_tracks(
        &mut self,
        painter: &egui::Painter,
        rect: egui::Rect,
        pixels_per_beat: f32,
        grid_response: &egui::Response,
    ) {
        for (index, (name, muted, soloed, _, grid_position, grid_length, waveform_samples)) in
            self.tracks.iter().enumerate()
        {
            let track_y =
                rect.top() + TRACK_SPACING + (index as f32 * (TRACK_HEIGHT + TRACK_SPACING));
            let track_x = rect.left() + (grid_position * (60.0 / self.bpm) * pixels_per_beat);
            let track_width = grid_length * (60.0 / self.bpm) * pixels_per_beat;

            let track_rect = egui::Rect::from_min_size(
                egui::pos2(track_x, track_y),
                egui::vec2(track_width, TRACK_HEIGHT),
            );

            // Draw track background
            let bg_color = if *muted {
                Color32::from_rgb(40, 40, 40)
            } else if *soloed {
                Color32::from_rgb(40, 40, 60)
            } else {
                Color32::from_rgb(35, 35, 35)
            };
            painter.rect_filled(track_rect, 4.0, bg_color);
            painter.rect_stroke(track_rect, 4.0, Stroke::new(1.0, TRACK_BORDER_COLOR));

            // Draw track name
            painter.text(
                egui::pos2(track_x + 8.0, track_y + 20.0),
                egui::Align2::LEFT_TOP,
                name,
                egui::FontId::proportional(14.0),
                TRACK_TEXT_COLOR,
            );

            // Draw waveform
            if !waveform_samples.is_empty() {
                let waveform_height = TRACK_HEIGHT - 40.0;
                let waveform_y = track_y + 30.0;
                let waveform_width = track_width - 16.0;
                let waveform_x = track_x + 8.0;

                for (i, &sample) in waveform_samples.iter().enumerate() {
                    let x =
                        waveform_x + (i as f32 / waveform_samples.len() as f32) * waveform_width;
                    let amplitude = sample * waveform_height * 0.8;
                    painter.line_segment(
                        [
                            egui::pos2(x, waveform_y + waveform_height / 2.0 - amplitude),
                            egui::pos2(x, waveform_y + waveform_height / 2.0 + amplitude),
                        ],
                        Stroke::new(1.0, WAVEFORM_COLOR),
                    );
                }
            }

            // Handle track dragging
            if grid_response.dragged() {
                if let Some(pos) = grid_response.interact_pointer_pos() {
                    if track_rect.contains(pos) {
                        let click_x = pos.x - rect.left();
                        let grid_position = click_x / pixels_per_beat;
                        (self.on_track_drag)(index, grid_position);
                    }
                }
            }
        }
    }
}

struct TrackControls<'a> {
    tracks: Vec<(String, bool, bool, bool, f32, f32, f32, f32)>,
    on_track_start_change: &'a mut dyn FnMut(usize, f32),
    on_track_end_change: &'a mut dyn FnMut(usize, f32),
    on_track_file_select: &'a mut dyn FnMut(usize),
    on_track_mute: &'a mut dyn FnMut(usize),
    on_track_solo: &'a mut dyn FnMut(usize),
    on_track_record: &'a mut dyn FnMut(usize),
}

impl<'a> TrackControls<'a> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        for (
            index,
            (
                name,
                muted,
                soloed,
                recording,
                grid_position,
                grid_length,
                current_position,
                duration,
            ),
        ) in self.tracks.iter().enumerate()
        {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                // Track name with color based on whether it has an audio file
                ui.label(RichText::new(name).size(14.0).color(if *duration > 0.0 {
                    Color32::from_rgb(100, 255, 100)
                } else {
                    Color32::from_rgb(200, 200, 200)
                }));

                // Start and End time controls
                ui.label(RichText::new("Start:").size(14.0));
                let mut start_time = grid_position * (60.0 / 120.0);
                if ui
                    .add(egui::DragValue::new(&mut start_time).speed(0.1))
                    .changed()
                {
                    let new_grid_pos = start_time / (60.0 / 120.0);
                    (self.on_track_start_change)(index, new_grid_pos);
                }

                ui.label(RichText::new("End:").size(14.0));
                let mut end_time = (grid_position + grid_length) * (60.0 / 120.0);
                if ui
                    .add(egui::DragValue::new(&mut end_time).speed(0.1))
                    .changed()
                {
                    let new_grid_len = (end_time / (60.0 / 120.0)) - grid_position;
                    (self.on_track_end_change)(index, new_grid_len);
                }

                // File selector button
                if ui.button(RichText::new("📂").size(14.0)).clicked() {
                    (self.on_track_file_select)(index);
                }

                // Track position indicator
                if *duration > 0.0 {
                    ui.label(
                        RichText::new(format!("{:.1}s / {:.1}s", current_position, duration))
                            .size(14.0),
                    );
                }

                ui.add_space(ui.available_width() - 100.0);

                // Track controls
                if ui
                    .button(RichText::new(if *muted { "🔇" } else { "M" }).size(14.0))
                    .clicked()
                {
                    (self.on_track_mute)(index);
                }
                if ui
                    .button(RichText::new(if *soloed { "S!" } else { "S" }).size(14.0))
                    .clicked()
                {
                    (self.on_track_solo)(index);
                }
                if ui
                    .button(RichText::new(if *recording { "⏺" } else { "R" }).size(14.0))
                    .clicked()
                {
                    (self.on_track_record)(index);
                }
                ui.add_space(8.0);
            });
        }
    }
}

#[derive(Debug)]
enum DawCommand {
    Rewind,
    PlayPause,
    Forward,
    SetBpm(f32),
    SetGrid(f32),
    Save,
    Load,
    SetTimelinePosition(f32),
    SetLastClickedBar(f32),
    SetTrackPosition(usize, f32),
    SetTrackLength(usize, f32),
    LoadTrackFile(usize),
    ToggleTrackMute(usize),
    ToggleTrackSolo(usize),
    ToggleTrackRecord(usize),
}

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Set dark theme
        ctx.set_visuals(egui::Visuals::dark());

        // Force continuous repaints at 60 FPS
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(1.0 / 60.0));

        // Handle seek position if set
        if let Some(click_position) = self.seek_position.take() {
            self.state.timeline_position = click_position;
            for track in &mut self.state.tracks {
                track.update_grid_times(self.state.bpm);
                if click_position >= track.grid_start_time && click_position < track.grid_end_time {
                    let relative_position = click_position - track.grid_start_time;
                    track.seek_to(relative_position);
                } else {
                    track.seek_to(0.0);
                }
            }
        }

        // Handle spacebar input
        if ctx.input(|i| i.key_pressed(Key::Space)) {
            self.state.is_playing = !self.state.is_playing;
            self.last_update = std::time::Instant::now();
            self.update_playback();
        }

        // Update timeline position based on audio playback
        if self.state.is_playing {
            let now = std::time::Instant::now();
            let delta = now.duration_since(self.last_update).as_secs_f32();
            self.state.timeline_position += delta;

            // Update track positions and playback
            for track in &mut self.state.tracks {
                track.update_grid_times(self.state.bpm);

                if self.state.timeline_position >= track.grid_start_time
                    && self.state.timeline_position < track.grid_end_time
                {
                    let relative_position = self.state.timeline_position - track.grid_start_time;
                    if !track.is_playing {
                        track.seek_to(relative_position);
                        track.play();
                    }
                    track.current_position = relative_position;
                } else {
                    if track.is_playing {
                        track.pause();
                    }
                    track.current_position = 0.0;
                }
            }

            self.last_update = now;
        } else {
            for track in &mut self.state.tracks {
                track.pause();
                track.current_position = 0.0;
            }
        }

        let (tx, rx) = channel();

        egui::CentralPanel::default().show(ctx, |ui| {
            // Top toolbar
            ui.add_space(8.0);
            TransportControls {
                is_playing: self.state.is_playing,
                bpm: self.state.bpm,
                grid_division: self.state.grid_division,
                on_rewind: &mut || {
                    let _ = tx.send(DawCommand::Rewind);
                },
                on_play_pause: &mut || {
                    let _ = tx.send(DawCommand::PlayPause);
                },
                on_forward: &mut || {
                    let _ = tx.send(DawCommand::Forward);
                },
                on_bpm_change: &mut |bpm| {
                    let _ = tx.send(DawCommand::SetBpm(bpm));
                },
                on_grid_change: &mut |grid| {
                    let _ = tx.send(DawCommand::SetGrid(grid));
                },
                on_save: &mut || {
                    let _ = tx.send(DawCommand::Save);
                },
                on_load: &mut || {
                    let _ = tx.send(DawCommand::Load);
                },
            }
            .draw(ui);
            ui.add_space(8.0);
            ui.separator();

            // Timeline
            Timeline {
                timeline_position: self.state.timeline_position,
                bpm: self.state.bpm,
                grid_division: self.state.grid_division,
                on_timeline_click: &mut |position| {
                    let _ = tx.send(DawCommand::SetLastClickedBar(position));
                    let _ = tx.send(DawCommand::SetTimelinePosition(position));
                },
                last_clicked_bar: self.state.last_clicked_bar,
            }
            .draw(ui);

            // Grid
            Grid {
                timeline_position: self.state.timeline_position,
                bpm: self.state.bpm,
                grid_division: self.state.grid_division,
                tracks: self
                    .state
                    .tracks
                    .iter()
                    .map(|track| {
                        (
                            track.name.clone(),
                            track.muted,
                            track.soloed,
                            track.recording,
                            track.grid_position,
                            track.grid_length,
                            track.waveform_samples.clone(),
                        )
                    })
                    .collect(),
                on_track_drag: &mut |index, grid_position| {
                    if let Some(track) = self.state.tracks.get(index) {
                        // Calculate drag offset on first drag
                        if self.state.drag_offset.is_none() {
                            self.state.drag_offset = Some(grid_position - track.grid_position);
                        }

                        // Apply drag offset and snap to grid
                        if let Some(offset) = self.state.drag_offset {
                            let new_position = grid_position - offset;
                            let snapped_position = (new_position * self.state.grid_division)
                                .round()
                                / self.state.grid_division;
                            let _ = tx.send(DawCommand::SetTrackPosition(index, snapped_position));
                        }
                    }
                },
            }
            .draw(ui);

            // Track controls
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            TrackControls {
                tracks: self
                    .state
                    .tracks
                    .iter()
                    .map(|track| {
                        (
                            track.name.clone(),
                            track.muted,
                            track.soloed,
                            track.recording,
                            track.grid_position,
                            track.grid_length,
                            track.current_position(),
                            track.duration,
                        )
                    })
                    .collect(),
                on_track_start_change: &mut |index, new_position| {
                    let _ = tx.send(DawCommand::SetTrackPosition(index, new_position));
                },
                on_track_end_change: &mut |index, new_length| {
                    let _ = tx.send(DawCommand::SetTrackLength(index, new_length));
                },
                on_track_file_select: &mut |index| {
                    let _ = tx.send(DawCommand::LoadTrackFile(index));
                },
                on_track_mute: &mut |index| {
                    let _ = tx.send(DawCommand::ToggleTrackMute(index));
                },
                on_track_solo: &mut |index| {
                    let _ = tx.send(DawCommand::ToggleTrackSolo(index));
                },
                on_track_record: &mut |index| {
                    let _ = tx.send(DawCommand::ToggleTrackRecord(index));
                },
            }
            .draw(ui);
        });

        // Handle commands after UI rendering
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                DawCommand::Rewind => {
                    self.state.timeline_position = 0.0;
                    for track in &mut self.state.tracks {
                        track.is_playing = false;
                        track.current_position = 0.0;
                        track.seek_to(0.0);
                    }
                }
                DawCommand::PlayPause => {
                    self.state.is_playing = !self.state.is_playing;
                    self.last_update = std::time::Instant::now();
                    self.update_playback();
                }
                DawCommand::Forward => {
                    self.state.timeline_position += 4.0;
                    for track in &mut self.state.tracks {
                        track.is_playing = false;
                        track.current_position = self.state.timeline_position;
                        track.seek_to(self.state.timeline_position);
                    }
                }
                DawCommand::SetBpm(bpm) => {
                    self.state.bpm = bpm;
                }
                DawCommand::SetGrid(grid) => {
                    self.state.grid_division = grid;
                }
                DawCommand::Save => {
                    self.save_project();
                }
                DawCommand::Load => {
                    self.load_project();
                }
                DawCommand::SetTimelinePosition(position) => {
                    self.state.timeline_position = position;
                    for track in &mut self.state.tracks {
                        track.seek_to(position * (60.0 / self.state.bpm));
                    }
                }
                DawCommand::SetLastClickedBar(position) => {
                    self.state.last_clicked_bar = position;
                    // Also update timeline position immediately
                    self.state.timeline_position = position;
                    for track in &mut self.state.tracks {
                        track.update_grid_times(self.state.bpm);
                        if position >= track.grid_start_time && position < track.grid_end_time {
                            let relative_position = position - track.grid_start_time;
                            track.seek_to(relative_position);
                        }
                    }
                }
                DawCommand::SetTrackPosition(index, new_position) => {
                    if let Some(track) = self.state.tracks.get_mut(index) {
                        track.grid_position = new_position;
                    }
                }
                DawCommand::SetTrackLength(index, new_length) => {
                    if let Some(track) = self.state.tracks.get_mut(index) {
                        track.grid_length = new_length;
                    }
                }
                DawCommand::LoadTrackFile(index) => {
                    if let Some(track) = self.state.tracks.get_mut(index) {
                        if let Some(path) = FileDialog::new()
                            .add_filter("Audio", &["mp3", "wav", "ogg", "flac"])
                            .pick_file()
                        {
                            track.is_playing = false;
                            track.current_position = 0.0;
                            track.waveform_samples.clear();
                            track.audio_file = Some(path.clone());
                            track.name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Unknown")
                                .to_string();
                            track.load_waveform();
                            track.create_stream(&self.audio);
                            eprintln!("Loaded audio file: {}", path.display());
                        }
                    }
                }
                DawCommand::ToggleTrackMute(index) => {
                    if let Some(track) = self.state.tracks.get_mut(index) {
                        track.muted = !track.muted;
                    }
                }
                DawCommand::ToggleTrackSolo(index) => {
                    if let Some(track) = self.state.tracks.get_mut(index) {
                        track.soloed = !track.soloed;
                    }
                }
                DawCommand::ToggleTrackRecord(index) => {
                    if let Some(track) = self.state.tracks.get_mut(index) {
                        track.recording = !track.recording;
                    }
                }
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.on_exit();
    }
}
