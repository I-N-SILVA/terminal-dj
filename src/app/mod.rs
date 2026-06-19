use crate::audio::AudioPlayer;
use crate::library::Library;
use anyhow::Result;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use rustfft::{Fft, FftPlanner};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum InputMode {
    Normal,
    Editing,
    Downloading,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum VisualizerMode {
    RetroCrt,
    NeonWaves,
    CyberpunkPeak,
}

#[derive(Default)]
pub struct DownloadState {
    pub is_downloading: bool,
    pub progress: f64,
    pub message: String,
    pub filename: Option<PathBuf>,
}

pub struct App {
    pub audio_player: Option<AudioPlayer>,
    pub library: Library,

    pub input_mode: InputMode,
    pub library_search: String,
    pub library_search_active: bool,
    pub download_url: String,

    pub selected_track_index: usize,

    pub current_track_name: String,
    pub current_artist: String,
    pub playback_pos: std::time::Duration,
    pub total_duration: std::time::Duration,
    pub volume: f32,

    pub library_load_rx: Option<std::sync::mpsc::Receiver<(PathBuf, crate::metadata::TrackMeta)>>,
    pub music_dir: Option<PathBuf>,
    pub current_cover_art: Option<Vec<u8>>,
    pub cover_image_state: Option<StatefulProtocol>,

    pub vis_mode: VisualizerMode,
    pub spectrum: Vec<f32>,
    pub peak_spectrum: Vec<f32>,
    pub fft_plan: Arc<dyn Fft<f32>>,

    pub notification: Arc<tokio::sync::Mutex<String>>,
    pub download_state: Arc<std::sync::Mutex<DownloadState>>,
}

impl App {
    pub fn new() -> Result<Self> {
        let audio_player = AudioPlayer::new().ok();
        let music_dir = std::env::var("MUSIC_DIR")
            .map(PathBuf::from)
            .ok()
            .or_else(|| dirs::audio_dir());

        let mut planner = FftPlanner::new();
        let fft_plan = planner.plan_fft_forward(2048);

        let mut app = App {
            audio_player,
            library: Library::new(),
            input_mode: InputMode::Normal,
            library_search: String::new(),
            library_search_active: false,
            download_url: String::new(),
            selected_track_index: 0,
            current_track_name: "No track playing".to_string(),
            current_artist: "-".to_string(),
            playback_pos: std::time::Duration::ZERO,
            total_duration: std::time::Duration::ZERO,
            volume: 1.0,
            library_load_rx: None,
            music_dir,
            current_cover_art: None,
            cover_image_state: None,
            vis_mode: VisualizerMode::RetroCrt,
            spectrum: vec![0.0; 64],
            peak_spectrum: vec![0.0; 64],
            fft_plan,
            notification: Arc::new(tokio::sync::Mutex::new(String::new())),
            download_state: Arc::new(std::sync::Mutex::new(DownloadState::default())),
        };

        app.load_library();
        Ok(app)
    }

    pub fn load_library(&mut self) {
        if let Some(path) = self.music_dir.clone() {
            self.library.tracks.clear();
            self.library.metadata.clear();

            let (tx, rx) = std::sync::mpsc::channel::<(PathBuf, crate::metadata::TrackMeta)>();
            self.library_load_rx = Some(rx);

            std::thread::spawn(move || {
                let mut paths: Vec<PathBuf> = Vec::new();
                let mut stack = vec![path];
                while let Some(dir) = stack.pop() {
                    if dir.is_dir() {
                        if let Ok(iter) = std::fs::read_dir(&dir) {
                            for entry in iter.filter_map(Result::ok) {
                                let p = entry.path();
                                if p.is_dir() {
                                    stack.push(p);
                                } else if let Some(ext) = p.extension() {
                                    let ext_str = ext.to_string_lossy().to_lowercase();
                                    if matches!(
                                        ext_str.as_str(),
                                        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus"
                                    ) {
                                        let _ = tx.send((
                                            p.clone(),
                                            crate::metadata::TrackMeta::default(),
                                        ));
                                        paths.push(p);
                                    }
                                }
                            }
                        }
                    }
                }

                let paths_arc = std::sync::Arc::new(std::sync::Mutex::new(paths));
                let workers = std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
                    .min(8);
                let mut handles = Vec::new();
                for _ in 0..workers {
                    let txc = tx.clone();
                    let p_arc = paths_arc.clone();
                    let h = std::thread::spawn(move || loop {
                        let p_opt = {
                            let mut guard = p_arc.lock().unwrap();
                            guard.pop()
                        };
                        match p_opt {
                            Some(p) => {
                                let meta = crate::metadata::TrackMeta::read(&p);
                                let _ = txc.send((p, meta));
                            }
                            None => break,
                        }
                    });
                    handles.push(h);
                }
                for h in handles {
                    let _ = h.join();
                }
            });
        }
    }

    pub fn filtered_library(&self) -> Vec<(usize, &std::path::PathBuf)> {
        if self.library_search.is_empty() {
            return self.library.tracks.iter().enumerate().collect();
        }
        let q = self.library_search.to_lowercase();
        self.library
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                let meta = self.library.metadata.get(*p);
                let text = if let Some(m) = meta {
                    format!(
                        "{} {}",
                        m.title.as_deref().unwrap_or(""),
                        m.artist.as_deref().unwrap_or("")
                    )
                    .to_lowercase()
                } else {
                    p.file_name()
                        .map(|n| n.to_string_lossy().to_lowercase())
                        .unwrap_or_default()
                        .to_string()
                };
                text.contains(&q)
                    || p.file_name()
                        .map(|n| n.to_string_lossy().to_lowercase().contains(&q))
                        .unwrap_or(false)
            })
            .collect()
    }

    pub fn play_selected_track(&mut self) {
        let filtered = self.filtered_library();
        if let Some((_idx, path)) = filtered.get(self.selected_track_index) {
            let track = (*path).clone();
            self.play_track_path(track);
        }
    }

    fn play_track_path(&mut self, track: PathBuf) {
        let (artist, title) = if let Some(meta) = self.library.metadata.get(&track) {
            self.current_cover_art = meta.cover_art.clone();
            (
                meta.display_artist().to_string(),
                meta.display_title(&track),
            )
        } else {
            self.current_cover_art = None;
            let stem = track
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let parts: Vec<&str> = stem.splitn(2, " - ").collect();
            if parts.len() >= 2 {
                (parts[0].to_string(), parts[1].to_string())
            } else {
                ("Unknown".to_string(), stem)
            }
        };

        if let Some(art) = &self.current_cover_art {
            if let Ok(image) = image::load_from_memory(art) {
                let picker = Picker::halfblocks();
                self.cover_image_state = Some(picker.new_resize_protocol(image));
            } else {
                self.cover_image_state = None;
            }
        } else {
            self.cover_image_state = None;
        }

        self.current_artist = artist;
        self.current_track_name = title;
        self.playback_pos = std::time::Duration::ZERO;

        if let Some(player) = &mut self.audio_player {
            if let Ok(dur) = player.play_file(track.clone()) {
                self.total_duration = dur;
            }
        }
    }

    pub fn download_music(&mut self, url: String) {
        if let Some(music_dir) = &self.music_dir {
            let dir = music_dir.clone();
            let dl_state = self.download_state.clone();
            {
                let mut st = dl_state.lock().unwrap();
                st.is_downloading = true;
                st.progress = 0.0;
                st.message = format!("Initializing download for: {}", url);
                st.filename = None;
            }

            tokio::spawn(async move {
                let _ = crate::downloader::download_with_progress(&url, &dir, dl_state).await;
            });
        }
    }

    pub fn next_track(&mut self) {
        let filtered = self.filtered_library();
        if !filtered.is_empty() {
            self.selected_track_index = (self.selected_track_index + 1) % filtered.len();
        }
    }

    pub fn previous_track(&mut self) {
        let filtered = self.filtered_library();
        if !filtered.is_empty() {
            if self.selected_track_index > 0 {
                self.selected_track_index -= 1;
            } else {
                self.selected_track_index = filtered.len() - 1;
            }
        }
    }

    pub fn toggle_pause(&mut self) {
        if let Some(player) = &mut self.audio_player {
            player.toggle_pause();
        }
    }

    pub fn volume_up(&mut self) {
        self.volume = (self.volume + 0.05).min(1.0);
        if let Some(player) = &mut self.audio_player {
            player.set_volume(self.volume);
        }
    }

    pub fn volume_down(&mut self) {
        self.volume = (self.volume - 0.05).max(0.0);
        if let Some(player) = &mut self.audio_player {
            player.set_volume(self.volume);
        }
    }

    pub fn seek_forward(&mut self) {
        if let Some(player) = &mut self.audio_player {
            let next_pos = self.playback_pos + std::time::Duration::from_secs(5);
            player.try_seek(next_pos);
            self.playback_pos = next_pos;
        }
    }

    pub fn seek_backward(&mut self) {
        if let Some(player) = &mut self.audio_player {
            let next_pos = self
                .playback_pos
                .saturating_sub(std::time::Duration::from_secs(5));
            player.try_seek(next_pos);
            self.playback_pos = next_pos;
        }
    }

    pub fn toggle_visualizer(&mut self) {
        self.vis_mode = match self.vis_mode {
            VisualizerMode::RetroCrt => VisualizerMode::NeonWaves,
            VisualizerMode::NeonWaves => VisualizerMode::CyberpunkPeak,
            VisualizerMode::CyberpunkPeak => VisualizerMode::RetroCrt,
        };
        if let Ok(mut n) = self.notification.try_lock() {
            *n = format!("Visualizer changed to {:?}", self.vis_mode);
        }
    }

    pub fn on_tick(&mut self) {
        // Auto-play finished downloads
        let dl_file = {
            let mut st = self.download_state.lock().unwrap();
            if !st.is_downloading && st.progress >= 100.0 && st.filename.is_some() {
                st.progress = 0.0;
                st.filename.take()
            } else {
                None
            }
        };

        if let Some(path) = dl_file {
            let meta = crate::metadata::TrackMeta::read(&path);
            if !self.library.tracks.contains(&path) {
                self.library.tracks.insert(0, path.clone());
                self.library.metadata.insert(path.clone(), meta);
            }
            // Auto play it
            if let Some(idx) = self
                .filtered_library()
                .iter()
                .position(|(_, p)| **p == path)
            {
                self.selected_track_index = idx;
            } else {
                self.selected_track_index = 0;
            }
            self.play_track_path(path);
        }

        // Poll for new library items
        if let Some(rx) = &self.library_load_rx {
            while let Ok((path, meta)) = rx.try_recv() {
                if !self.library.tracks.contains(&path) {
                    self.library.tracks.push(path.clone());
                }
                self.library.metadata.insert(path, meta);
            }
        }

        // Update playback pos & FFT
        if let Some(player) = &mut self.audio_player {
            if player.is_finished() && !player.is_paused() {
                let filtered = self.filtered_library();
                if !filtered.is_empty() {
                    self.selected_track_index = (self.selected_track_index + 1) % filtered.len();
                    self.play_selected_track();
                }
            } else {
                self.playback_pos = player.get_pos();

                if let Some(consumer) = &mut player.fft_consumer {
                    let mut buffer = vec![0.0; 2048];
                    let mut len = 0;
                    while let Some(s) = consumer.pop() {
                        if len < 2048 {
                            buffer[len] = s;
                            len += 1;
                        }
                    }
                    if len == 2048 {
                        let mut complex_buffer: Vec<rustfft::num_complex::Complex<f32>> = buffer
                            .iter()
                            .map(|&val| rustfft::num_complex::Complex { re: val, im: 0.0 })
                            .collect();

                        self.fft_plan.process(&mut complex_buffer);

                        let bins = 32; // Mirroring to 64
                        let max_freq_idx = 1024;
                        let step = max_freq_idx / bins;

                        let mut new_spectrum = vec![0.0; 64];

                        for i in 0..bins {
                            let mut sum = 0.0;
                            for j in 0..step {
                                let idx = i * step + j;
                                sum += complex_buffer[idx].norm();
                            }
                            let avg = sum / step as f32;
                            let new_val = (avg * 2.0).min(1.0);

                            // mirror and smooth
                            let smoothed = self.spectrum[bins - 1 - i] * 0.5 + new_val * 0.5;
                            new_spectrum[bins - 1 - i] = smoothed; // Left half
                            new_spectrum[bins + i] = smoothed; // Right half
                        }

                        for i in 0..64 {
                            self.spectrum[i] = new_spectrum[i];
                            // Gravity decay for peaks
                            self.peak_spectrum[i] =
                                (self.peak_spectrum[i] - 0.04).max(self.spectrum[i]);
                        }
                    } else {
                        // Decay spectrum
                        for i in 0..64 {
                            self.spectrum[i] *= 0.8;
                            self.peak_spectrum[i] =
                                (self.peak_spectrum[i] - 0.04).max(self.spectrum[i]);
                        }
                    }
                }
            }
        }
    }
}
