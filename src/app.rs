use crate::audio::{AudioPlayer, AudioProducer};
use crate::library::Library;
use crate::db::Db;
use crate::playlist::Playlist;
use crate::spotify::{SpotifyClient, SpotifyPlaylist};
use crate::soundcloud::{SoundCloudClient, ScTrack};
use rspotify::clients::OAuthClient;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use ringbuf::{Consumer, HeapRb};
use rustfft::{FftPlanner, num_complex::Complex};
use anyhow::Result;

// ─── Enums ────────────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum SpotifyFocus { Playlists, Search }

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum InputMode { Normal, Editing, Command }

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum VisualizerMode { Bars, Cyberfield, Matrix, Plasma, Oscilloscope }

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum RepeatMode { Off, One, All }

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum ColorTheme { Neon, Amber, Mono, Dracula, Matrix }

impl ColorTheme {
    pub fn primary(self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            ColorTheme::Neon    => Color::Cyan,
            ColorTheme::Amber   => Color::Rgb(255, 176, 0),
            ColorTheme::Mono    => Color::White,
            ColorTheme::Dracula => Color::Rgb(189, 147, 249),
            ColorTheme::Matrix  => Color::Rgb(0, 255, 70),
        }
    }
    pub fn secondary(self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            ColorTheme::Neon    => Color::Yellow,
            ColorTheme::Amber   => Color::Rgb(255, 220, 100),
            ColorTheme::Mono    => Color::Gray,
            ColorTheme::Dracula => Color::Rgb(255, 121, 198),
            ColorTheme::Matrix  => Color::Green,
        }
    }
    pub fn bg(self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            ColorTheme::Dracula => Color::Rgb(40, 42, 54),
            _                   => Color::Rgb(10, 10, 35),
        }
    }
    pub fn cycle(self) -> Self {
        match self {
            ColorTheme::Neon    => ColorTheme::Amber,
            ColorTheme::Amber   => ColorTheme::Mono,
            ColorTheme::Mono    => ColorTheme::Dracula,
            ColorTheme::Dracula => ColorTheme::Matrix,
            ColorTheme::Matrix  => ColorTheme::Neon,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            ColorTheme::Neon    => "Neon",
            ColorTheme::Amber   => "Amber",
            ColorTheme::Mono    => "Mono",
            ColorTheme::Dracula => "Dracula",
            ColorTheme::Matrix  => "Matrix",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "neon"    => Some(ColorTheme::Neon),
            "amber"   => Some(ColorTheme::Amber),
            "mono"    => Some(ColorTheme::Mono),
            "dracula" => Some(ColorTheme::Dracula),
            "matrix"  => Some(ColorTheme::Matrix),
            _         => None,
        }
    }
}

// ─── App ──────────────────────────────────────────────────────────────────────

pub struct App {
    pub audio_player: Option<AudioPlayer>,
    pub library: Library,
    pub db: Option<Db>,
    pub playlists: Vec<Playlist>,
    pub spotify_client: Arc<tokio::sync::Mutex<SpotifyClient>>,
    pub spotify_playlists: Arc<tokio::sync::Mutex<Vec<SpotifyPlaylist>>>,
    pub spotify_search_results: Arc<tokio::sync::Mutex<Vec<rspotify::model::FullTrack>>>,
    pub selected_tab: usize,
    pub selected_track_index: usize,
    pub selected_playlist_index: usize,
    pub viewing_playlist_index: Option<usize>,
    pub selected_playlist_track_index: usize,

    // Spotify
    pub spotify_focus: SpotifyFocus,
    pub spotify_playlist_index: usize,
    pub spotify_search_index: usize,

    // Input
    pub input: String,
    pub input_mode: InputMode,
    pub command_buffer: String,

    // Lyrics
    pub current_lyrics: Arc<tokio::sync::Mutex<String>>,
    pub lyrics_scroll: u16,
    pub karaoke_lines: Arc<tokio::sync::Mutex<Vec<crate::karaoke::LrcLine>>>,
    pub karaoke_active: bool,

    // Playback
    pub current_track_name: String,
    pub current_artist:     String,
    pub current_vibe:       Option<String>,
    pub playback_pos:       std::time::Duration,

    pub total_duration: std::time::Duration,

    // Viz
    pub vis_buffer_producer: Arc<Mutex<AudioProducer>>,
    pub vis_buffer_consumer: Consumer<f32, Arc<HeapRb<f32>>>,
    pub spectrum_data:     Vec<f32>,
    pub spectrum_velocity: Vec<f32>,
    pub recent_samples:    Vec<f32>,

    pub intensity: f32,
    pub mood_color: Option<ratatui::style::Color>,
    pub show_help: bool,
    pub vis_mode: VisualizerMode,
    pub cursor_pos: (u16, u16),
    pub ticks: u64,
    pub fft_plan: Arc<dyn rustfft::Fft<f32>>,
    pub spotify_playback: Arc<tokio::sync::Mutex<Option<rspotify::model::CurrentPlaybackContext>>>,

    // Playback extras
    pub volume: f32,
    pub playing_track_index: Option<usize>,
    pub playing_playlist_index: Option<usize>,

    // Shuffle / Repeat / Queue
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub queue: std::collections::VecDeque<PathBuf>,

    // Color theme
    pub color_theme: ColorTheme,

    // Sleep timer
    pub sleep_at: Option<std::time::Instant>,
    pub sleep_preset_idx: usize,

    // Library search + async scan
    pub library_search: String,
    pub library_search_active: bool,
    pub library_load_rx: Option<std::sync::mpsc::Receiver<(PathBuf, crate::metadata::TrackMeta)>>,

    // Beets
    pub beets_db_path: Option<PathBuf>,

    // Cover art
    pub current_cover_art: Option<Vec<u8>>,
    pub pending_cover_art: Arc<tokio::sync::Mutex<Option<Vec<u8>>>>,

    // Discord RPC
    pub discord_tx: Option<std::sync::mpsc::SyncSender<(String, String)>>,

    // Last.fm
    pub lastfm: Option<Arc<tokio::sync::Mutex<crate::lastfm::LastfmClient>>>,
    pub listenbrainz_token: Option<String>,
    pub track_play_start: Option<std::time::Instant>,
    pub track_scrobbled: bool,

    // Misc
    pub spotify_status: Arc<tokio::sync::Mutex<String>>,
    pub capture_mic_audio: Arc<std::sync::atomic::AtomicBool>,
    pub _mic_stream: Option<cpal::Stream>,
    pub rng_state: u64,

    // ── NEW ───────────────────────────────────────────────────────────────────

    // 10-band EQ
    pub eq_gains_arc: Arc<Mutex<[f32; crate::eq::BAND_COUNT]>>,
    pub show_eq: bool,
    pub eq_focused: bool,
    pub eq_selected_band: usize,

    // Crossfade
    pub crossfade_secs: f32,
    pub crossfade_active: bool,
    pub pending_xfade_track: Option<PathBuf>,
    pub pending_xfade_duration: std::time::Duration,
    pub pending_xfade_track_idx: Option<usize>,
    pub pending_xfade_pl_idx: Option<usize>,

    // BPM
    pub current_bpm: Arc<Mutex<Option<f32>>>,

    // Notifications (downloads, auto-tag, commands)
    pub notification: Arc<tokio::sync::Mutex<String>>,
    pub reload_library_flag: Arc<std::sync::atomic::AtomicBool>,
    pub music_dir: Option<PathBuf>,

    // Media controls (system media keys / MPRIS2)
    pub media_controls: Option<souvlaki::MediaControls>,
    pub media_event_rx: Option<std::sync::mpsc::Receiver<i8>>,

    // Track analysis: BPM + normalisation + waveform (computed async)
    pub track_analysis: Arc<Mutex<Option<crate::bpm::TrackAnalysis>>>,
    pub waveform_data:  Vec<f32>,    // 200-point peak waveform, 0..1
    pub current_norm_gain: f32,

    pub zen_mode: bool,
    pub pulse_scale: f32,

    pub glitch_active: bool,
    pub glitch_ticks:  u32,

    // SoundCloud
    pub soundcloud_client:  Option<SoundCloudClient>,
    pub soundcloud_results: Arc<tokio::sync::Mutex<Vec<ScTrack>>>,
    pub soundcloud_search_index: usize,
    pub soundcloud_status:  Arc<tokio::sync::Mutex<String>>,
    pub pending_sc_bytes:   Arc<tokio::sync::Mutex<Option<Vec<u8>>>>,
    pub pending_sc_track:   Arc<tokio::sync::Mutex<Option<ScTrack>>>,
}

impl App {
    pub fn new() -> Result<Self> {
        let db = Db::new()?;

        let ring = HeapRb::<f32>::new(4096);
        let (producer, consumer) = ring.split();
        let producer_arc   = Arc::new(Mutex::new(producer));
        let capture_mic_audio = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // CPAL microphone stream
        let cpal_producer = producer_arc.clone();
        let cpal_active   = capture_mic_audio.clone();
        let mic_stream = (|| {
            use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
            let host = cpal::default_host();
            let device = host.default_input_device()?;
            let config = device.default_input_config().ok()?;
            let err_fn = |err| log::error!("cpal error: {:?}", err);
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| {
                        if cpal_active.load(std::sync::atomic::Ordering::Relaxed) {
                            if let Ok(mut p) = cpal_producer.try_lock() {
                                for &s in data { p.push(s).ok(); }
                            }
                        }
                    },
                    err_fn, None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _| {
                        if cpal_active.load(std::sync::atomic::Ordering::Relaxed) {
                            if let Ok(mut p) = cpal_producer.try_lock() {
                                for &s in data { p.push(s as f32 / i16::MAX as f32).ok(); }
                            }
                        }
                    },
                    err_fn, None,
                ),
                _ => return None,
            };
            if let Ok(s) = stream {
                if s.play().is_ok() { return Some(s); }
            }
            None
        })();

        // Discord RPC
        let discord_tx = std::env::var("DISCORD_APP_ID").ok().map(|app_id| {
            let (tx, rx) = std::sync::mpsc::sync_channel::<(String, String)>(4);
            std::thread::spawn(move || {
                use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
                let Ok(mut client) = DiscordIpcClient::new(&app_id) else { return; };
                if client.connect().is_err() { return; }
                for (title, artist) in rx {
                    let payload = activity::Activity::new()
                        .details(&title)
                        .state(&artist);
                    let _ = client.set_activity(payload);
                }
                let _ = client.close();
            });
            tx
        });

        // Last.fm
        let lastfm = crate::lastfm::LastfmClient::from_env().map(|c| {
            let arc  = Arc::new(tokio::sync::Mutex::new(c));
            let arc2 = arc.clone();
            tokio::spawn(async move {
                if let Err(e) = arc2.lock().await.authenticate().await {
                    log::warn!("Last.fm auth: {}", e);
                }
            });
            arc
        });

        // Media controls (souvlaki — MPRIS2 on Linux, NowPlaying on macOS)
        let (media_tx, media_rx) = std::sync::mpsc::channel::<i8>();
        let media_controls = (|| -> Option<souvlaki::MediaControls> {
            let cfg = souvlaki::PlatformConfig {
                #[cfg(not(target_os = "windows"))]
                dbus_name: "terminal.dj",
                display_name: "Terminal DJ",
                hwnd: None,
            };
            let mut c = souvlaki::MediaControls::new(cfg).ok()?;
            c.attach(move |ev| {
                use souvlaki::MediaControlEvent as E;
                let code: i8 = match ev {
                    E::Play   => 0,
                    E::Pause  => 1,
                    E::Toggle => 2,
                    E::Next   => 3,
                    E::Previous => 4,
                    _ => return,
                };
                media_tx.send(code).ok();
            }).ok()?;
            Some(c)
        })();

        let audio_player  = AudioPlayer::new().ok();
        let status_arc    = Arc::new(tokio::sync::Mutex::new(
            "Press 'l' on the Spotify tab to connect".to_string(),
        ));
        let notification  = Arc::new(tokio::sync::Mutex::new(String::new()));

        let music_dir = std::env::var("MUSIC_DIR").map(PathBuf::from).ok()
            .or_else(|| dirs::audio_dir());

        let mut planner = FftPlanner::new();
        let fft_plan = planner.plan_fft_forward(1024);

        let mut app = App {
            audio_player,
            library: Library::new(),
            db: Some(db),
            playlists: Vec::new(),
            spotify_client: Arc::new(tokio::sync::Mutex::new(SpotifyClient::new())),
            spotify_playlists: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            spotify_search_results: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            selected_tab: 0,
            selected_track_index: 0,
            selected_playlist_index: 0,
            viewing_playlist_index: None,
            selected_playlist_track_index: 0,
            spotify_focus: SpotifyFocus::Playlists,
            spotify_playlist_index: 0,
            spotify_search_index: 0,
            input: String::new(),
            input_mode: InputMode::Normal,
            command_buffer: String::new(),
            current_lyrics: Arc::new(tokio::sync::Mutex::new(String::new())),
            lyrics_scroll: 0,
            karaoke_lines: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            karaoke_active: true,
            current_track_name: "No track playing".to_string(),
            current_artist: "-".to_string(),
            current_vibe: None,
            playback_pos: std::time::Duration::ZERO,
            total_duration: std::time::Duration::ZERO,
            vis_buffer_producer: producer_arc,
            vis_buffer_consumer: consumer,
            spectrum_data:     vec![0.0; 64],
            spectrum_velocity: vec![0.0; 64],
            recent_samples:    Vec::new(),
            intensity: 0.0,
            mood_color: None,
            show_help: false,
            vis_mode: VisualizerMode::Bars,
            cursor_pos: (10, 10),
            ticks: 0,
            fft_plan,
            spotify_playback: Arc::new(tokio::sync::Mutex::new(None)),
            volume: 1.0,
            playing_track_index: None,
            playing_playlist_index: None,
            shuffle: false,
            repeat: RepeatMode::Off,
            queue: std::collections::VecDeque::new(),
            color_theme: ColorTheme::Neon,
            sleep_at: None,
            sleep_preset_idx: 0,
            library_search: String::new(),
            library_search_active: false,
            library_load_rx: None,
            beets_db_path: std::env::var("BEETS_DB").ok().map(PathBuf::from),
            current_cover_art: None,
            pending_cover_art: Arc::new(tokio::sync::Mutex::new(None)),
            discord_tx,
            lastfm,
            listenbrainz_token: std::env::var("LISTENBRAINZ_TOKEN").ok(),
            track_play_start: None,
            track_scrobbled: false,
            spotify_status: status_arc.clone(),
            capture_mic_audio,
            _mic_stream: mic_stream,
            rng_state: 12345678901234567,
            // new
            eq_gains_arc: Arc::new(Mutex::new([0.0; crate::eq::BAND_COUNT])),
            show_eq: false,
            eq_focused: false,
            eq_selected_band: 0,
            crossfade_secs: 0.0,
            crossfade_active: false,
            pending_xfade_track: None,
            pending_xfade_duration: std::time::Duration::ZERO,
            pending_xfade_track_idx: None,
            pending_xfade_pl_idx: None,
            current_bpm: Arc::new(Mutex::new(None)),
            notification,
            reload_library_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            music_dir,
            media_controls,
            media_event_rx: Some(media_rx),

            track_analysis:      Arc::new(Mutex::new(None)),
            waveform_data:       vec![0.0; 200],
            current_norm_gain:   1.0,
            zen_mode:            false,
            pulse_scale:         1.0,
            glitch_active:       false,
            glitch_ticks:        0,

            soundcloud_client:   SoundCloudClient::from_env(),
            soundcloud_results:  Arc::new(tokio::sync::Mutex::new(Vec::new())),
            soundcloud_search_index: 0,
            soundcloud_status:   Arc::new(tokio::sync::Mutex::new(
                if std::env::var("SOUNDCLOUD_CLIENT_ID").is_ok() {
                    "Set — press 's' to search".to_string()
                } else {
                    "Set SOUNDCLOUD_CLIENT_ID to enable".to_string()
                }
            )),
            pending_sc_bytes:    Arc::new(tokio::sync::Mutex::new(None)),
            pending_sc_track:    Arc::new(tokio::sync::Mutex::new(None)),
        };

        app.load_library();
        app.refresh_playlists();

        // Spotify background task
        let client        = app.spotify_client.clone();
        let playback_arc  = app.spotify_playback.clone();
        let playlists_arc = app.spotify_playlists.clone();
        let status_arc2   = app.spotify_status.clone();
        tokio::spawn(async move {
            let token_loaded = {
                let c = client.lock().await;
                match c.load_token().await {
                    Ok(b) => b,
                    Err(e) => {
                        *status_arc2.lock().await =
                            format!("Token file error — press 'l' to reconnect ({})", e);
                        false
                    }
                }
            };
            if token_loaded {
                *status_arc2.lock().await = "Token found — loading playlists...".to_string();
                let result = { let c = client.lock().await; c.list_playlists().await };
                match result {
                    Ok(p) => {
                        let n = p.len();
                        *playlists_arc.lock().await = p;
                        { let c = client.lock().await; let _ = c.save_token().await; }
                        *status_arc2.lock().await = format!("Connected ✓  ({} playlists)", n);
                    }
                    Err(e) => {
                        *status_arc2.lock().await =
                            format!("Token expired — press 'l' to reconnect ({})", e);
                    }
                }
            }
            let mut interval       = tokio::time::interval(std::time::Duration::from_secs(2));
            let mut consec_errors: u32 = 0;
            loop {
                interval.tick().await;
                let c = client.lock().await;
                if !c.has_token().await { consec_errors = 0; continue; }
                match c.get_current_playback().await {
                    Ok(Some(pb)) => { consec_errors = 0; *playback_arc.lock().await = Some(pb); }
                    Ok(None)     => { consec_errors = 0; }
                    Err(_) => {
                        consec_errors += 1;
                        let backoff = (2u64.pow(consec_errors.min(5))).min(60);
                        drop(c);
                        tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                    }
                }
            }
        });

        Ok(app)
    }

    // ─── Library ───────────────────────────────────────────────────────────────

    pub fn load_library(&mut self) {
        if let Some(path) = self.music_dir.clone() {
            // Clear current library to show incremental progress
            self.library.tracks.clear();
            self.library.metadata.clear();

            let (tx, rx) = std::sync::mpsc::channel::<(PathBuf, crate::metadata::TrackMeta)>();
            self.library_load_rx = Some(rx);
            // We'll walk directories quickly, send a default meta for each file
            // so the UI shows entries immediately, then spawn a small worker pool
            // to compute full metadata and send updated records.
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
                                    if matches!(ext_str.as_str(), "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus") {
                                        // Send placeholder meta immediately so UI can list the file
                                        let _ = tx.send((p.clone(), crate::metadata::TrackMeta::default()));
                                        paths.push(p);
                                    }
                                }
                            }
                        }
                    }
                }

                // Worker pool to compute metadata
                let paths_arc = std::sync::Arc::new(std::sync::Mutex::new(paths));
                let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
                let mut handles = Vec::new();
                for _ in 0..workers {
                    let txc = tx.clone();
                    let p_arc = paths_arc.clone();
                    let h = std::thread::spawn(move || {
                        loop {
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
                        }
                    });
                    handles.push(h);
                }
                // Wait for workers to finish
                for h in handles { let _ = h.join(); }
                // Dropping tx (and clones) closes the channel.
            });
        }
    }

    pub fn refresh_playlists(&mut self) {
        if let Some(db) = &self.db {
            if let Ok(data) = db.get_playlists() {
                self.playlists = data.into_iter().map(|(id, name)| {
                    let mut p = Playlist::new(id, name);
                    if let Ok(tracks) = db.get_tracks(id) { p.tracks = tracks; }
                    p
                }).collect();
            }
        }
    }

    pub fn filtered_library(&self) -> Vec<(usize, &std::path::PathBuf)> {
        if self.library_search.is_empty() {
            return self.library.tracks.iter().enumerate().collect();
        }
        let q = self.library_search.to_lowercase();
        self.library.tracks.iter().enumerate().filter(|(_, p)| {
            let meta = self.library.metadata.get(*p);
            let text = if let Some(m) = meta {
                format!("{} {}", m.title.as_deref().unwrap_or(""), m.artist.as_deref().unwrap_or("")).to_lowercase()
            } else {
                p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default().to_string()
            };
            text.contains(&q) || p.file_name()
                .map(|n| n.to_string_lossy().to_lowercase().contains(&q))
                .unwrap_or(false)
        }).collect()
    }

    // ─── Playback ──────────────────────────────────────────────────────────────

    fn play_track_path(&mut self, track: PathBuf) {
        self.lyrics_scroll = 0;
        if let Ok(mut lines) = self.karaoke_lines.try_lock() { lines.clear(); }

        let (artist, title) = if let Some(meta) = self.library.metadata.get(&track) {
            self.current_cover_art = meta.cover_art.clone();
            (meta.display_artist().to_string(), meta.display_title(&track))
        } else {
            self.current_cover_art = None;
            let stem = track.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            let parts: Vec<&str> = stem.splitn(2, " - ").collect();
            if parts.len() >= 2 { (parts[0].to_string(), parts[1].to_string()) }
            else { ("Unknown".to_string(), stem) }
        };

        self.current_artist     = artist.clone();
        self.current_track_name = title.clone();
        self.track_play_start   = Some(std::time::Instant::now());
        self.track_scrobbled    = false;
        self.update_mood();
        self.fetch_lyrics(&artist, &title);
        self.fetch_lrc(&artist, &title);
        self.update_media_controls_state();
        
        if self.current_cover_art.is_none() {
            self.fetch_missing_cover(&artist, &title);
        }

        if let Some(ref tx) = self.discord_tx {
            tx.try_send((title.clone(), artist.clone())).ok();
        }
        if let Some(ref lastfm) = self.lastfm {
            let fm = lastfm.clone();
            let (a, t) = (artist.clone(), title.clone());
            tokio::spawn(async move { let _ = fm.lock().await.now_playing(&a, &t).await; });
        }

        self.playback_pos = std::time::Duration::ZERO;
        let eq = self.eq_gains_arc.clone();
        if let Some(player) = &mut self.audio_player {
            let producer = self.vis_buffer_producer.clone();
            if let Ok(dur) = player.play_file(track.clone(), producer, eq) {
                self.total_duration = dur;
            }
        }

        // Reset analysis state, then compute async.
        self.current_norm_gain = 1.0;
        self.waveform_data = vec![0.0; 200];
        if let Ok(mut g) = self.current_bpm.lock() { *g = None; }
        if let Ok(mut g) = self.track_analysis.lock() { *g = None; }

        let analysis_arc = self.track_analysis.clone();
        tokio::task::spawn_blocking(move || {
            if let Ok(a) = crate::bpm::analyze_file(&track) {
                if let Ok(mut g) = analysis_arc.lock() { *g = Some(a); }
            }
        });
    }

    pub fn advance_track(&mut self) {
        if self.crossfade_active { return; }

        if let Some(ref mut player) = self.audio_player { player.mark_stopped(); }

        if let Some(queued) = self.queue.pop_front() {
            self.playing_playlist_index = None;
            self.play_track_path(queued);
            return;
        }

        if self.repeat == RepeatMode::One {
            if let Some(pl_idx) = self.playing_playlist_index {
                let t = self.playing_track_index
                    .and_then(|i| self.playlists.get(pl_idx)?.tracks.get(i)).cloned();
                if let Some(t) = t { self.play_track_path(t); return; }
            } else if let Some(orig) = self.playing_track_index {
                if let Some(t) = self.library.tracks.get(orig).cloned() {
                    self.play_track_path(t); return;
                }
            }
        }

        if let Some(pl_idx) = self.playing_playlist_index {
            let pl_len = self.playlists.get(pl_idx).map(|p| p.tracks.len()).unwrap_or(0);
            if pl_len == 0 { return; }
            let next_idx = if self.shuffle {
                self.next_random() as usize % pl_len
            } else {
                self.playing_track_index.map(|i| i + 1).unwrap_or(0)
            };
            let actual = if next_idx >= pl_len {
                if self.repeat == RepeatMode::All { 0 } else { return; }
            } else { next_idx };
            let track = self.playlists.get(pl_idx)
                .and_then(|p| p.tracks.get(actual)).cloned();
            if let Some(t) = track {
                self.playing_track_index = Some(actual);
                self.selected_playlist_track_index = actual;
                self.play_track_path(t);
            }
        } else if let Some(orig_idx) = self.playing_track_index {
            if self.shuffle {
                let owned: Vec<(usize, PathBuf)> = self.filtered_library()
                    .into_iter().map(|(i, p)| (i, p.clone())).collect();
                if owned.is_empty() { return; }
                let pos = self.next_random() as usize % owned.len();
                let (next_orig, track) = owned[pos].clone();
                self.playing_track_index  = Some(next_orig);
                self.selected_track_index = pos;
                self.play_track_path(track);
            } else {
                let next = {
                    let filtered = self.filtered_library();
                    let pos = filtered.iter().position(|(i, _)| *i == orig_idx);
                    pos.and_then(|p| filtered.get(p + 1))
                        .map(|(ni, p)| (*ni, (*p).clone()))
                        .or_else(|| {
                            if self.repeat == RepeatMode::All && !filtered.is_empty() {
                                Some((filtered[0].0, filtered[0].1.clone()))
                            } else { None }
                        })
                };
                if let Some((next_orig, track)) = next {
                    self.playing_track_index  = Some(next_orig);
                    self.selected_track_index = self.filtered_library()
                        .iter().position(|(i, _)| *i == next_orig).unwrap_or(0);
                    self.play_track_path(track);
                }
            }
        }
    }

    pub fn prev_track(&mut self) {
        // If more than 5 s in, restart current; otherwise skip back.
        if self.playback_pos.as_secs() > 5 {
            if let Some(p) = &self.audio_player {
                p.try_seek(std::time::Duration::ZERO);
            }
        } else {
            if let Some(p) = &self.audio_player {
                p.try_seek(std::time::Duration::ZERO);
            }
        }
    }

    // ─── Crossfade ─────────────────────────────────────────────────────────────

    /// Called every tick. Starts a crossfade when approaching end of track.
    pub fn start_crossfade_if_needed(&mut self) {
        if self.crossfade_secs <= 0.0 || self.crossfade_active { return; }
        if self.total_duration == std::time::Duration::ZERO { return; }
        let remaining = self.total_duration.saturating_sub(self.playback_pos);
        let rem_secs  = remaining.as_secs_f32();
        if rem_secs > self.crossfade_secs + 0.5 || rem_secs < 0.5 { return; }

        if let Some((next_path, track_idx, pl_idx)) = self.peek_next_track() {
            // Begin preloading metadata (lyrics, etc.)
            let (artist, title) = self.get_track_meta(&next_path);
            self.current_artist     = artist.clone();
            self.current_track_name = title.clone();
            self.fetch_lyrics(&artist, &title);
            self.fetch_lrc(&artist, &title);
            self.update_mood();
            if let Some(ref tx) = self.discord_tx {
                tx.try_send((title.clone(), artist.clone())).ok();
            }

            let producer = self.vis_buffer_producer.clone();
            let eq       = self.eq_gains_arc.clone();
            let secs     = self.crossfade_secs;
            if let Some(player) = &mut self.audio_player {
                if let Ok(dur) = player.start_crossfade(&next_path, producer, eq, secs) {
                    self.crossfade_active        = true;
                    self.pending_xfade_track     = Some(next_path);
                    self.pending_xfade_duration  = dur;
                    self.pending_xfade_track_idx = track_idx;
                    self.pending_xfade_pl_idx    = pl_idx;
                }
            }
        }
    }

    fn peek_next_track(&mut self) -> Option<(PathBuf, Option<usize>, Option<usize>)> {
        if let Some(t) = self.queue.front() {
            return Some((t.clone(), None, None));
        }
        if let Some(pl_idx) = self.playing_playlist_index {
            let pl_len = self.playlists.get(pl_idx).map(|p| p.tracks.len()).unwrap_or(0);
            if pl_len == 0 { return None; }
            let next_i = if self.shuffle {
                self.next_random() as usize % pl_len
            } else {
                self.playing_track_index.map(|i| i + 1).unwrap_or(0)
            };
            let actual = if next_i >= pl_len {
                if self.repeat == RepeatMode::All { 0 } else { return None; }
            } else { next_i };
            let t = self.playlists.get(pl_idx)
                .and_then(|p| p.tracks.get(actual)).cloned()?;
            return Some((t, Some(actual), Some(pl_idx)));
        }
        if let Some(orig_idx) = self.playing_track_index {
            if self.shuffle {
                let owned: Vec<(usize, PathBuf)> = self.filtered_library()
                    .into_iter().map(|(i, p)| (i, p.clone())).collect();
                if owned.is_empty() { return None; }
                let pos = self.next_random() as usize % owned.len();
                return Some((owned[pos].1.clone(), Some(owned[pos].0), None));
            }
            let next = {
                let filtered = self.filtered_library();
                let pos = filtered.iter().position(|(i, _)| *i == orig_idx);
                pos.and_then(|p| filtered.get(p + 1))
                    .map(|(ni, p)| (*ni, (*p).clone()))
                    .or_else(|| {
                        if self.repeat == RepeatMode::All && !filtered.is_empty() {
                            Some((filtered[0].0, filtered[0].1.clone()))
                        } else { None }
                    })
            };
            return next.map(|(ni, p)| (p, Some(ni), None));
        }
        None
    }

    fn get_track_meta(&self, path: &PathBuf) -> (String, String) {
        if let Some(meta) = self.library.metadata.get(path) {
            (meta.display_artist().to_string(), meta.display_title(path))
        } else {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            let parts: Vec<&str> = stem.splitn(2, " - ").collect();
            if parts.len() >= 2 { (parts[0].to_string(), parts[1].to_string()) }
            else { ("Unknown".to_string(), stem) }
        }
    }

    fn on_crossfade_complete(&mut self) {
        if let Some(path) = self.pending_xfade_track.take() {
            self.playing_track_index    = self.pending_xfade_track_idx.take();
            self.playing_playlist_index = self.pending_xfade_pl_idx.take();
            self.total_duration         = self.pending_xfade_duration;
            self.track_play_start       = Some(std::time::Instant::now());
            self.track_scrobbled        = false;

            // Sync cover art
            if let Some(meta) = self.library.metadata.get(&path) {
                self.current_cover_art = meta.cover_art.clone();
            }

            // Sync selected UI index
            if let Some(orig) = self.playing_track_index {
                if self.playing_playlist_index.is_none() {
                    self.selected_track_index = self.filtered_library()
                        .iter().position(|(i, _)| *i == orig).unwrap_or(0);
                }
            }

            // Trigger Last.fm now-playing
            if let Some(ref lastfm) = self.lastfm {
                let fm = lastfm.clone();
                let (a, t) = (self.current_artist.clone(), self.current_track_name.clone());
                tokio::spawn(async move { let _ = fm.lock().await.now_playing(&a, &t).await; });
            }

            // Full analysis (BPM + waveform + normalization)
            self.current_norm_gain = 1.0;
            self.waveform_data = vec![0.0; 200];
            if let Ok(mut g) = self.current_bpm.lock() { *g = None; }
            if let Ok(mut g) = self.track_analysis.lock() { *g = None; }
            let analysis_arc = self.track_analysis.clone();
            tokio::task::spawn_blocking(move || {
                if let Ok(a) = crate::bpm::analyze_file(&path) {
                    if let Ok(mut g) = analysis_arc.lock() { *g = Some(a); }
                }
            });
        }
        self.crossfade_active = false;
    }

    // ─── on_tick ───────────────────────────────────────────────────────────────

    pub fn on_tick(&mut self) {
        // Async library scan result (incremental, batched)
        if let Some(ref rx) = self.library_load_rx {
            use std::sync::mpsc::TryRecvError;
            let mut batch: Vec<(PathBuf, crate::metadata::TrackMeta)> = Vec::new();
            loop {
                match rx.try_recv() {
                    Ok((path, meta)) => batch.push((path, meta)),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => { self.library_load_rx = None; break; }
                }
                // Limit how many we pull in one tick to avoid starving UI (~200)
                if batch.len() >= 200 { break; }
            }
            if !batch.is_empty() {
                // Apply updates in a batch to reduce redraw churn and avoid duplicates
                for (p, m) in batch {
                    if !self.library.metadata.contains_key(&p) {
                        self.library.tracks.push(p.clone());
                    }
                    // Always overwrite metadata with freshest data
                    self.library.metadata.insert(p, m);
                }
            }
        }

        // Reload triggered (e.g., after yt-dlp download)
        if self.reload_library_flag.swap(false, std::sync::atomic::Ordering::Relaxed) {
            self.load_library();
        }

        // Sleep timer
        if let Some(at) = self.sleep_at {
            if std::time::Instant::now() >= at {
                self.sleep_at        = None;
                self.sleep_preset_idx = 0;
                if let Some(p) = &mut self.audio_player { if !p.is_paused() { p.toggle_pause(); } }
            }
        }

        // Media control events — collect first to release the borrow before acting.
        let media_cmds: Vec<i8> = self.media_event_rx
            .as_ref()
            .map(|rx| std::iter::from_fn(|| rx.try_recv().ok()).collect())
            .unwrap_or_default();
        for code in media_cmds {
            match code {
                0 | 1 | 2 => self.toggle_pause(),
                3          => self.advance_track(),
                4          => self.prev_track(),
                _          => {}
            }
        }

        // Crossfade progress
        if self.crossfade_active {
            let done = self.audio_player.as_mut()
                .map(|p| p.update_crossfade(0.1))
                .unwrap_or(false);
            if done { self.on_crossfade_complete(); }
        } else {
            // Auto-advance when finished
            let finished = self.audio_player.as_ref().map(|p| p.is_finished()).unwrap_or(false);
            if finished { self.advance_track(); }
            // Maybe start a crossfade
            let paused = self.audio_player.as_ref().map(|p| p.is_paused()).unwrap_or(true);
            if !paused { self.start_crossfade_if_needed(); }
        }

        if let Some(player) = &self.audio_player {
            self.playback_pos = player.get_pos();
        }

        // Apply completed analysis (BPM + waveform + norm gain + mood)
        let finished_analysis = self.track_analysis.try_lock()
            .ok()
            .and_then(|mut g| g.take());
        if let Some(analysis) = finished_analysis {
            if let Ok(mut b) = self.current_bpm.lock() { *b = analysis.bpm; }
            self.current_norm_gain = analysis.norm_gain;
            self.waveform_data     = analysis.waveform;
            
            if let Some(mood) = analysis.mood {
                self.set_notification(format!("Vibe detected: {}", mood));
                self.current_vibe = Some(mood.clone());
                // We can use this to set mood_color if we want to be even smarter
                if mood == "Chill" { self.mood_color = Some(ratatui::style::Color::Rgb(0, 255, 255)); }
                else if mood == "Energetic" { self.mood_color = Some(ratatui::style::Color::Rgb(255, 0, 127)); }
            }

            self.apply_volume();
        }

        // Play buffered SoundCloud audio
        let sc_bytes = self.pending_sc_bytes.try_lock()
            .ok()
            .and_then(|mut g| g.take());
        if let Some(data) = sc_bytes {
            let sc_track = self.pending_sc_track.try_lock().ok().and_then(|mut g| g.take());
            if let Some(ref t) = sc_track {
                self.current_track_name = t.title.clone();
                self.current_artist     = t.display_artist().to_string();
                self.total_duration     = std::time::Duration::from_millis(t.duration);
                self.fetch_lyrics(&self.current_artist.clone(), &self.current_track_name.clone());
                self.fetch_lrc(&self.current_artist.clone(), &self.current_track_name.clone());
                self.update_mood();
                self.track_play_start = Some(std::time::Instant::now());
                self.track_scrobbled  = false;
                self.playback_pos     = std::time::Duration::ZERO;
                self.update_media_controls_state();
            }
            let eq       = self.eq_gains_arc.clone();
            let producer = self.vis_buffer_producer.clone();
            if let Some(player) = &mut self.audio_player {
                if let Ok(dur) = player.play_bytes(data, producer, eq) {
                    if dur > std::time::Duration::ZERO { self.total_duration = dur; }
                }
            }
        }

        // Drain visualiser buffer
        let mut samples = Vec::new();
        while let Some(s) = self.vis_buffer_consumer.pop() { samples.push(s); }

        // Pick up pending cover art
        if let Some(art) = self.pending_cover_art.try_lock().ok().and_then(|mut g| g.take()) {
            self.current_cover_art = Some(art);
            self.update_media_controls_state();
        }

        if !samples.is_empty() {
            self.recent_samples.extend_from_slice(&samples);
            let len = self.recent_samples.len();
            if len > 4096 { self.recent_samples.drain(0..len - 4096); }
        }

        if samples.len() >= 1024 {
            let skip = samples.len() - 1024;
            let mut buf: Vec<Complex<f32>> = samples.iter().skip(skip).take(1024)
                .map(|&s| Complex { re: s, im: 0.0 }).collect();
            self.fft_plan.process(&mut buf);
            
            let gravity    = 0.05;
            let rise_speed = 0.5;

            for i in 0..64 {
                let start = (1.5f32.powf(i as f32 / 6.0) as usize).max(i);
                let end   = (1.5f32.powf((i + 1) as f32 / 6.0) as usize).max(start + 1).min(512);
                let sum: f32 = buf[start..end].iter().map(|c| c.norm()).sum();
                let raw_val = (sum / (end - start) as f32 * 1.5).min(1.0);
                
                // CAVA-style Physics
                // If raw value is higher than current, it "pushes" the bar up
                if raw_val > self.spectrum_data[i] {
                    self.spectrum_data[i] = self.spectrum_data[i] * (1.0 - rise_speed) + raw_val * rise_speed;
                    self.spectrum_velocity[i] = 0.0; // Reset falling velocity
                } else {
                    // Else, the bar falls with gravity
                    self.spectrum_velocity[i] += gravity;
                    self.spectrum_data[i] = (self.spectrum_data[i] - self.spectrum_velocity[i]).max(0.0);
                }
            }
            self.intensity = self.spectrum_data.iter().take(8).sum::<f32>() / 8.0;
            self.pulse_scale = 1.0 + (self.intensity * 0.1);
        }

        if self.glitch_ticks > 0 {
            self.glitch_ticks -= 1;
            if self.glitch_ticks == 0 { self.glitch_active = false; }
        }

        self.ticks += 1;

        // Last.fm scrobble
        if !self.track_scrobbled {
            if let Some(start) = self.track_play_start {
                if start.elapsed().as_secs() >= 30 {
                    self.track_scrobbled = true;
                    if let Some(ref fm) = self.lastfm {
                        let fm  = fm.clone();
                        let a   = self.current_artist.clone();
                        let t   = self.current_track_name.clone();
                        let ts  = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                        tokio::spawn(async move { let _ = fm.lock().await.scrobble(&a, &t, ts).await; });
                    }
                    self.scrobble_listenbrainz(&self.current_artist, &self.current_track_name);
                }
            }
        }

        let local_playing = self.audio_player.as_ref().map(|p| !p.is_finished()).unwrap_or(false);
        self.capture_mic_audio.store(!local_playing, std::sync::atomic::Ordering::Relaxed);

        // Sync Spotify state every second
        if self.ticks % 10 == 0 && !local_playing {
            let spotify_data = self.spotify_playback.try_lock().ok().and_then(|pb| {
                pb.as_ref().and_then(|state| {
                    let item_data = state.item.as_ref().and_then(|item| {
                        if let rspotify::model::PlayableItem::Track(t) = item {
                            let artist = t.artists.first().map(|a| a.name.clone()).unwrap_or_else(|| "Unknown".to_string());
                            Some((t.name.clone(), artist, t.duration.num_milliseconds().max(0) as u64))
                        } else { None }
                    });
                    state.progress.map(|p| (item_data, p.num_milliseconds().max(0) as u64))
                })
            });
            if let Some((item_info, prog_ms)) = spotify_data {
                if let Some((name, artist, dur_ms)) = item_info {
                    self.current_track_name = name;
                    self.current_artist     = artist;
                    self.total_duration     = std::time::Duration::from_millis(dur_ms);
                }
                self.playback_pos = std::time::Duration::from_millis(prog_ms);
                self.update_mood();
            }
        }
    }

    pub fn play_selected_track(&mut self) {
        match self.selected_tab {
            0 => {
                let item = {
                    let filtered = self.filtered_library();
                    filtered.get(self.selected_track_index).map(|(orig, p)| (*orig, (*p).clone()))
                };
                if let Some((orig_idx, track)) = item {
                    self.playing_track_index    = Some(orig_idx);
                    self.playing_playlist_index = None;
                    self.play_track_path(track);
                }
            }
            1 => {
                let (track, pl_idx, track_idx) = if let Some(idx) = self.viewing_playlist_index {
                    let t = self.playlists.get(idx)
                        .and_then(|p| p.tracks.get(self.selected_playlist_track_index).cloned());
                    (t, idx, self.selected_playlist_track_index)
                } else {
                    let t = self.playlists.get(self.selected_playlist_index)
                        .and_then(|p| p.tracks.first().cloned());
                    (t, self.selected_playlist_index, 0)
                };
                if let Some(track) = track {
                    self.playing_track_index    = Some(track_idx);
                    self.playing_playlist_index = Some(pl_idx);
                    self.viewing_playlist_index = Some(pl_idx);
                    self.play_track_path(track);
                }
            }
            2 => {
                let data = if self.spotify_focus == SpotifyFocus::Playlists {
                    self.spotify_playlists.try_lock().ok()
                        .and_then(|g| g.get(self.spotify_playlist_index)
                            .map(|p| (p.id.to_string(), true, String::new(), p.name.clone())))
                } else {
                    self.spotify_search_results.try_lock().ok()
                        .and_then(|g| g.get(self.spotify_search_index)
                            .and_then(|t| t.id.as_ref().map(|id| (
                                id.to_string(), false,
                                t.artists[0].name.clone(), t.name.clone(),
                            ))))
                };
                if let Some((u, is_playlist, artist, title)) = data {
                    if !is_playlist {
                        self.current_artist     = artist.clone();
                        self.current_track_name = title.clone();
                        self.fetch_lyrics(&artist, &title);
                        self.fetch_lrc(&artist, &title);
                    } else {
                        self.current_track_name = title.clone();
                        self.current_artist     = "Playlist".to_string();
                    }
                    self.update_mood();
                    self.play_spotify_uri(u, is_playlist);
                }
            }
            3 => self.play_soundcloud_selected(),
            _ => {}
        }
    }

    // ─── Tab / Navigation ──────────────────────────────────────────────────────

    pub fn switch_tab(&mut self)      { self.selected_tab = (self.selected_tab + 1) % 6; }
    pub fn switch_tab_back(&mut self) { self.selected_tab = (self.selected_tab + 5) % 6; }

    fn apply_volume(&mut self) {
        let v = self.volume * self.current_norm_gain;
        if let Some(p) = &mut self.audio_player { p.store_volume(v); }
    }

    pub fn volume_up(&mut self) {
        self.volume = (self.volume + 0.1).min(1.0);
        self.apply_volume();
    }
    pub fn volume_down(&mut self) {
        self.volume = (self.volume - 0.1).max(0.0);
        self.apply_volume();
    }

    pub fn seek_forward(&mut self) {
        let new = self.playback_pos + std::time::Duration::from_secs(5);
        if let Some(p) = &self.audio_player { p.try_seek(new); }
    }
    pub fn seek_backward(&mut self) {
        let new = self.playback_pos.saturating_sub(std::time::Duration::from_secs(5));
        if let Some(p) = &self.audio_player { p.try_seek(new); }
    }

    pub fn toggle_pause(&mut self) {
        if let Some(p) = &mut self.audio_player { p.toggle_pause(); }
        self.update_media_controls_state();
    }

    pub fn scroll_lyrics_up(&mut self)   { self.lyrics_scroll = self.lyrics_scroll.saturating_sub(3); }
    pub fn scroll_lyrics_down(&mut self) { self.lyrics_scroll = self.lyrics_scroll.saturating_add(3); }

    pub fn toggle_vis_mode(&mut self) {
        self.vis_mode = match self.vis_mode {
            VisualizerMode::Bars        => VisualizerMode::Cyberfield,
            VisualizerMode::Cyberfield  => VisualizerMode::Matrix,
            VisualizerMode::Matrix      => VisualizerMode::Plasma,
            VisualizerMode::Plasma      => VisualizerMode::Oscilloscope,
            VisualizerMode::Oscilloscope => VisualizerMode::Bars,
        };
    }

    pub fn move_cursor(&mut self, dx: i16, dy: i16) {
        let x = (self.cursor_pos.0 as i16).saturating_add(dx).max(0) as u16;
        let y = (self.cursor_pos.1 as i16).saturating_add(dy).max(0) as u16;
        self.cursor_pos = (x.min(100), y.min(40));
    }

    pub fn switch_spotify_focus(&mut self) {
        if self.selected_tab == 2 {
            self.spotify_focus = match self.spotify_focus {
                SpotifyFocus::Playlists => SpotifyFocus::Search,
                SpotifyFocus::Search    => SpotifyFocus::Playlists,
            };
        }
    }

    pub fn next_track(&mut self) {
        match self.selected_tab {
            0 => {
                let n = self.filtered_library().len();
                if n > 0 { self.selected_track_index = (self.selected_track_index + 1) % n; }
            }
            1 => {
                if let Some(idx) = self.viewing_playlist_index {
                    if let Some(p) = self.playlists.get(idx) {
                        if !p.tracks.is_empty() {
                            self.selected_playlist_track_index = (self.selected_playlist_track_index + 1) % p.tracks.len();
                        }
                    }
                } else if !self.playlists.is_empty() {
                    self.selected_playlist_index = (self.selected_playlist_index + 1) % self.playlists.len();
                }
            }
            2 => {
                if self.spotify_focus == SpotifyFocus::Playlists {
                    let n = self.spotify_playlists.try_lock().map(|g| g.len()).unwrap_or(0);
                    if n > 0 { self.spotify_playlist_index = (self.spotify_playlist_index + 1) % n; }
                } else {
                    let n = self.spotify_search_results.try_lock().map(|g| g.len()).unwrap_or(0);
                    if n > 0 { self.spotify_search_index = (self.spotify_search_index + 1) % n; }
                }
            }
            3 => {
                let n = self.soundcloud_results.try_lock().map(|g| g.len()).unwrap_or(0);
                if n > 0 { self.soundcloud_search_index = (self.soundcloud_search_index + 1) % n; }
            }
            _ => {}
        }
    }

    pub fn previous_track(&mut self) {
        match self.selected_tab {
            0 => {
                let n = self.filtered_library().len();
                if n > 0 {
                    self.selected_track_index = if self.selected_track_index > 0 {
                        self.selected_track_index - 1
                    } else { n - 1 };
                }
            }
            1 => {
                if let Some(idx) = self.viewing_playlist_index {
                    if let Some(p) = self.playlists.get(idx) {
                        if !p.tracks.is_empty() {
                            self.selected_playlist_track_index = if self.selected_playlist_track_index > 0 {
                                self.selected_playlist_track_index - 1
                            } else { p.tracks.len() - 1 };
                        }
                    }
                } else if !self.playlists.is_empty() {
                    self.selected_playlist_index = if self.selected_playlist_index > 0 {
                        self.selected_playlist_index - 1
                    } else { self.playlists.len() - 1 };
                }
            }
            2 => {
                if self.spotify_focus == SpotifyFocus::Playlists {
                    let n = self.spotify_playlists.try_lock().map(|g| g.len()).unwrap_or(0);
                    if n > 0 {
                        self.spotify_playlist_index = if self.spotify_playlist_index > 0 {
                            self.spotify_playlist_index - 1
                        } else { n - 1 };
                    }
                } else {
                    let n = self.spotify_search_results.try_lock().map(|g| g.len()).unwrap_or(0);
                    if n > 0 {
                        self.spotify_search_index = if self.spotify_search_index > 0 {
                            self.spotify_search_index - 1
                        } else { n - 1 };
                    }
                }
            }
            3 => {
                let n = self.soundcloud_results.try_lock().map(|g| g.len()).unwrap_or(0);
                if n > 0 {
                    self.soundcloud_search_index = if self.soundcloud_search_index > 0 {
                        self.soundcloud_search_index - 1
                    } else { n - 1 };
                }
            }
            _ => {}
        }
    }

    // ─── Playlist / Library ────────────────────────────────────────────────────

    pub fn enter_playlist(&mut self) {
        if self.selected_tab == 1 && self.viewing_playlist_index.is_none() {
            if !self.playlists.is_empty() {
                self.viewing_playlist_index = Some(self.selected_playlist_index);
                self.selected_playlist_track_index = 0;
            }
        } else {
            self.play_selected_track();
        }
    }

    pub fn exit_playlist(&mut self) { self.viewing_playlist_index = None; }

    pub fn create_playlist(&mut self) {
        if self.input.is_empty() { return; }
        if let Some(db) = &self.db {
            if db.create_playlist(&self.input).is_ok() { self.refresh_playlists(); }
        }
        self.input.clear();
        self.input_mode = InputMode::Normal;
    }

    pub fn add_selected_track_to_playlist(&mut self) {
        if self.selected_tab == 0 {
            let item = {
                let filtered = self.filtered_library();
                filtered.get(self.selected_track_index).map(|(_, p)| (*p).clone())
            };
            if let Some(track) = item {
                if let Some(pl) = self.playlists.get(self.selected_playlist_index) {
                    if let Some(db) = &self.db {
                        let _ = db.add_track(pl.id, &track);
                        self.refresh_playlists();
                    }
                }
            }
        }
    }

    pub fn enqueue_selected(&mut self) {
        let item = match self.selected_tab {
            0 => {
                let filtered = self.filtered_library();
                filtered.get(self.selected_track_index).map(|(_, p)| (*p).clone())
            }
            1 => {
                if let Some(idx) = self.viewing_playlist_index {
                    self.playlists.get(idx)
                        .and_then(|p| p.tracks.get(self.selected_playlist_track_index)).cloned()
                } else { None }
            }
            _ => None,
        };
        if let Some(t) = item { self.queue.push_back(t); }
    }

    // ─── Shuffle / Repeat / Theme / Sleep ──────────────────────────────────────

    pub fn toggle_shuffle(&mut self) { self.shuffle = !self.shuffle; }

    pub fn cycle_repeat(&mut self) {
        self.repeat = match self.repeat {
            RepeatMode::Off => RepeatMode::One,
            RepeatMode::One => RepeatMode::All,
            RepeatMode::All => RepeatMode::Off,
        };
    }

    pub fn cycle_theme(&mut self) { self.color_theme = self.color_theme.cycle(); }

    pub fn cycle_sleep_timer(&mut self) {
        const SECS: [u64; 5] = [0, 900, 1800, 2700, 3600];
        self.sleep_preset_idx = (self.sleep_preset_idx + 1) % 5;
        let s = SECS[self.sleep_preset_idx];
        self.sleep_at = if s > 0 {
            Some(std::time::Instant::now() + std::time::Duration::from_secs(s))
        } else { None };
    }

    pub fn sleep_timer_label(&self) -> &'static str {
        const L: [&str; 5] = ["Off", "15m", "30m", "45m", "60m"];
        L[self.sleep_preset_idx]
    }

    // ─── Karaoke / Lyrics ──────────────────────────────────────────────────────

    pub fn toggle_karaoke(&mut self) { self.karaoke_active = !self.karaoke_active; }

    pub fn fetch_lrc(&mut self, artist: &str, title: &str) {
        if artist.is_empty() || title.is_empty() { return; }
        let (a, t) = (artist.to_string(), title.to_string());
        let arc    = self.karaoke_lines.clone();
        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let Ok(resp) = client.get("https://lrclib.net/api/get")
                .query(&[("artist_name", a.as_str()), ("track_name", t.as_str())])
                .header("User-Agent", "terminal-dj/0.1")
                .send().await else { return; };
            let Ok(json) = resp.json::<serde_json::Value>().await else { return; };
            if let Some(lrc) = json.get("syncedLyrics").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                let parsed = crate::karaoke::parse_lrc(lrc);
                if !parsed.is_empty() { *arc.lock().await = parsed; }
            }
        });
    }

    // ─── Spotify ───────────────────────────────────────────────────────────────

    pub fn play_spotify_uri(&self, uri: String, is_playlist: bool) {
        let client_arc = self.spotify_client.clone();
        let status_arc = self.spotify_status.clone();
        tokio::spawn(async move {
            let client = client_arc.lock().await;
            let devices = match client.get_devices().await {
                Ok(d) => d,
                Err(e) => { *status_arc.lock().await = format!("Playback error: {}", e); return; }
            };
            let device_id = devices.iter().find(|d| d.is_active).or_else(|| devices.first())
                .and_then(|d| d.id.as_deref());
            if device_id.is_none() {
                *status_arc.lock().await = "No active Spotify device found".to_string();
                return;
            }
            let result = if is_playlist {
                match rspotify::model::PlaylistId::from_id(&uri)
                    .or_else(|_| rspotify::model::PlaylistId::from_uri(&uri)) {
                    Ok(pid) => client.spotify.start_context_playback(
                        rspotify::model::PlayContextId::Playlist(pid), device_id, None, None,
                    ).await,
                    Err(e) => { *status_arc.lock().await = format!("Invalid playlist ID: {}", e); return; }
                }
            } else {
                match rspotify::model::TrackId::from_id(&uri)
                    .or_else(|_| rspotify::model::TrackId::from_uri(&uri)) {
                    Ok(tid) => client.spotify.start_uris_playback(
                        vec![rspotify::model::PlayableId::Track(tid)], device_id, None, None,
                    ).await,
                    Err(e) => { *status_arc.lock().await = format!("Invalid track ID: {}", e); return; }
                }
            };
            if let Err(e) = result {
                let msg = e.to_string();
                *status_arc.lock().await = if msg.contains("403") || msg.contains("Premium") {
                    "Playback requires Spotify Premium".to_string()
                } else { format!("Playback failed: {}", msg) };
            }
        });
    }

    pub fn login_spotify(&mut self) {
        let client_arc    = self.spotify_client.clone();
        let playlists_arc = self.spotify_playlists.clone();
        let status_arc    = self.spotify_status.clone();
        tokio::spawn(async move {
            *status_arc.lock().await = "Opening browser...".to_string();
            let url = {
                let mut c = client_arc.lock().await;
                match c.get_auth_url().await {
                    Ok(u) => u,
                    Err(e) => { *status_arc.lock().await = format!("Auth URL error: {}", e); return; }
                }
            };
            let _ = webbrowser::open(&url);
            *status_arc.lock().await = "Waiting for browser auth…".to_string();
            let code = match crate::spotify::SpotifyClient::wait_for_auth_code().await {
                Ok(c) => c,
                Err(e) => { *status_arc.lock().await = format!("Browser auth failed: {}", e); return; }
            };
            *status_arc.lock().await = "Exchanging token...".to_string();
            let client = client_arc.lock().await;
            match client.complete_auth(&code).await {
                Ok(_) => {
                    match client.list_playlists().await {
                        Ok(p) => {
                            let n = p.len();
                            *playlists_arc.lock().await = p;
                            *status_arc.lock().await = format!("Connected ✓  ({} playlists)", n);
                        }
                        Err(e) => { *status_arc.lock().await = format!("Playlist load failed: {}", e); }
                    }
                }
                Err(e) => { *status_arc.lock().await = format!("Token exchange failed: {}", e); }
            }
        });
    }

    pub fn search_spotify(&mut self) {
        if self.input.is_empty() { return; }
        let query       = self.input.clone();
        let client_arc  = self.spotify_client.clone();
        let results_arc = self.spotify_search_results.clone();
        tokio::spawn(async move {
            let c = client_arc.lock().await;
            if let Ok(r) = c.search_tracks(&query).await { *results_arc.lock().await = r; }
        });
        self.input.clear();
        self.input_mode = InputMode::Normal;
    }

    // ─── SoundCloud ────────────────────────────────────────────────────────────

    pub fn search_soundcloud(&mut self) {
        if self.input.is_empty() { return; }
        let query   = self.input.clone();
        self.input.clear();
        self.input_mode = InputMode::Normal;

        let Some(client) = self.soundcloud_client.as_ref() else {
            self.set_notification("SOUNDCLOUD_CLIENT_ID not set".to_string());
            return;
        };
        // Build a minimal client snapshot (client_id + reqwest::Client)
        let client_id  = client.client_id.clone();
        let http       = client.http.clone();
        let results    = self.soundcloud_results.clone();
        let status     = self.soundcloud_status.clone();
        self.soundcloud_search_index = 0;

        tokio::spawn(async move {
            *status.lock().await = format!("Searching for: {}", query);
            let tmp_client = crate::soundcloud::SoundCloudClient::with_parts(client_id, http);
            match tmp_client.search(&query).await {
                Ok(tracks) => {
                    let n = tracks.len();
                    *results.lock().await = tracks;
                    *status.lock().await  = format!("{} results — j/k to navigate, Enter to stream", n);
                }
                Err(e) => {
                    *status.lock().await = format!("Error: {}", e);
                }
            }
        });
    }

    fn play_soundcloud_selected(&mut self) {
        let track = self.soundcloud_results.try_lock().ok()
            .and_then(|g| g.get(self.soundcloud_search_index).cloned());
        let Some(track) = track else { return; };

        let Some(client) = self.soundcloud_client.as_ref() else { return; };
        let client_id = client.client_id.clone();
        let http      = client.http.clone();
        let bytes_arc = self.pending_sc_bytes.clone();
        let track_arc = self.pending_sc_track.clone();
        let status    = self.soundcloud_status.clone();

        let title  = track.title.clone();
        self.set_notification(format!("Streaming: {}", title));

        tokio::spawn(async move {
            *status.lock().await = format!("Buffering: {}", title);
            let tmp_client = crate::soundcloud::SoundCloudClient::with_parts(client_id, http);
            match tmp_client.get_stream_bytes(&track).await {
                Ok(data) => {
                    *track_arc.lock().await = Some(track);
                    *bytes_arc.lock().await = Some(data);
                    *status.lock().await = "Ready".to_string();
                }
                Err(e) => {
                    *status.lock().await = format!("Stream error: {}", e);
                }
            }
        });
    }

    pub fn fetch_lyrics(&mut self, artist: &str, title: &str) {
        if artist.is_empty() || title.is_empty() { return; }
        let (a, t)    = (artist.to_string(), title.to_string());
        let lyrics_arc = self.current_lyrics.clone();
        tokio::spawn(async move {
            *lyrics_arc.lock().await = "Fetching lyrics...".to_string();
            let c = lyric_finder::Client::new();
            match c.get_lyric(&format!("{} {}", a, t)).await {
                Ok(lyric_finder::LyricResult::Some { lyric, .. }) => {
                    *lyrics_arc.lock().await = lyric;
                }
                Ok(lyric_finder::LyricResult::None) => {
                    *lyrics_arc.lock().await = format!("No lyrics found for: {} {}", a, t);
                }
                Err(e) => {
                    *lyrics_arc.lock().await = format!("Lyrics error: {}", e);
                }
            }
        });
    }

    // ─── EQ ────────────────────────────────────────────────────────────────────

    pub fn toggle_eq(&mut self) {
        self.show_eq = !self.show_eq;
        if self.show_eq { self.eq_focused = true; } else { self.eq_focused = false; }
    }

    pub fn eq_next_band(&mut self) {
        self.eq_selected_band = (self.eq_selected_band + 1) % crate::eq::BAND_COUNT;
    }
    pub fn eq_prev_band(&mut self) {
        self.eq_selected_band = self.eq_selected_band
            .checked_sub(1).unwrap_or(crate::eq::BAND_COUNT - 1);
    }

    pub fn eq_adjust(&mut self, delta_db: f32) {
        if let Ok(mut g) = self.eq_gains_arc.lock() {
            let v = (g[self.eq_selected_band] + delta_db).clamp(-12.0, 12.0);
            g[self.eq_selected_band] = v;
        }
    }

    pub fn eq_reset(&mut self) {
        if let Ok(mut g) = self.eq_gains_arc.lock() { *g = [0.0; crate::eq::BAND_COUNT]; }
    }

    pub fn eq_gains_snapshot(&self) -> [f32; crate::eq::BAND_COUNT] {
        self.eq_gains_arc.lock().map(|g| *g).unwrap_or([0.0; crate::eq::BAND_COUNT])
    }

    // ─── Command mode ──────────────────────────────────────────────────────────

    pub fn execute_command(&mut self, raw: String) {
        let cmd = raw.trim().to_string();
        if cmd.is_empty() { return; }

        let parts: Vec<&str> = cmd.splitn(3, ' ').collect();
        match parts.as_slice() {
            // Download
            ["download", url] | ["dl", url] => self.cmd_download(url.to_string()),

            // Theme
            ["theme", name] => {
                if let Some(t) = ColorTheme::from_name(name) {
                    self.color_theme = t;
                    self.set_notification(format!("Theme: {}", name));
                }
            }

            // Volume  :vol 80
            ["volume", n] | ["vol", n] => {
                if let Ok(v) = n.parse::<f32>() {
                    self.volume = (v / 100.0).clamp(0.0, 1.0);
                    self.apply_volume();
                    self.set_notification(format!("Volume: {}%", v.round() as u32));
                }
            }

            // Crossfade  :xfade 3
            ["crossfade", n] | ["xfade", n] => {
                if let Ok(s) = n.parse::<f32>() {
                    self.crossfade_secs = s.max(0.0);
                    self.set_notification(format!("Crossfade: {:.1}s", self.crossfade_secs));
                }
            }

            // Sleep  :sleep 30
            ["sleep", n] => {
                if let Ok(mins) = n.parse::<u64>() {
                    if mins == 0 {
                        self.sleep_at = None;
                        self.set_notification("Sleep timer: off".to_string());
                    } else {
                        self.sleep_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(mins * 60));
                        self.set_notification(format!("Sleep timer: {}m", mins));
                    }
                }
            }

            // EQ  :eq 5 +3   or   :eq reset
            ["eq", rest] => {
                if *rest == "reset" {
                    self.eq_reset();
                    self.set_notification("EQ reset".to_string());
                } else {
                    let sub: Vec<&str> = rest.splitn(2, ' ').collect();
                    if sub.len() == 2 {
                        if let (Ok(band), Ok(gain)) = (sub[0].parse::<usize>(), sub[1].parse::<f32>()) {
                            if band >= 1 && band <= crate::eq::BAND_COUNT {
                                if let Ok(mut g) = self.eq_gains_arc.lock() {
                                    g[band - 1] = gain.clamp(-12.0, 12.0);
                                }
                                self.set_notification(format!("EQ band {} = {:.1} dB", band, gain));
                            }
                        }
                    }
                }
            }

            // Auto-tag
            ["tag"] => self.auto_tag_current(),
            
            // Lucky Dip
            ["lucky"] | ["dip"] => self.cmd_lucky_dip(),
            
            // Beets sync
            ["beets"] => self.import_beets_library(),

            // Manually set cover art :cover https://...
            ["cover", url] => {
                let url = url.to_string();
                let pending = self.pending_cover_art.clone();
                let notif = self.notification.clone();
                tokio::spawn(async move {
                    let client = reqwest::Client::new();
                    if let Ok(resp) = client.get(&url).send().await {
                        if let Ok(bytes) = resp.bytes().await {
                            *pending.lock().await = Some(bytes.to_vec());
                            *notif.lock().await = "Cover art updated!".to_string();
                        }
                    }
                });
            }

            _ => self.set_notification(format!("Unknown command: {}", cmd)),
        }
    }

    fn cmd_lucky_dip(&mut self) {
        let n = self.library.tracks.len();
        if n == 0 {
            self.set_notification("Library empty, no luck today!".to_string());
            return;
        }
        let idx = self.next_random() as usize % n;
        if let Some(path) = self.library.tracks.get(idx).cloned() {
            self.set_notification(format!("Lucky Dip: 🎲 Playing random track..."));
            self.playing_track_index = Some(idx);
            self.playing_playlist_index = None;
            self.play_track_path(path);
        }
    }

    pub fn import_beets_library(&mut self) {
        let Some(db_path) = self.beets_db_path.clone() else {
            self.set_notification("BEETS_DB env var not set".to_string());
            return;
        };
        
        self.set_notification("Syncing with Beets...".to_string());
        let flag = self.reload_library_flag.clone();
        let notif = self.notification.clone();
        
        tokio::spawn(async move {
            // Perform rusqlite work on a blocking thread to avoid sending
            // non-Send rusqlite types across async await points.
            let res = tokio::task::spawn_blocking(move || -> Option<()> {
                if let Ok(conn) = rusqlite::Connection::open(db_path) {
                    if let Ok(mut stmt) = conn.prepare("SELECT path FROM items") {
                        if let Ok(mut rows) = stmt.query([]) {
                            // consume rows (we don't need to keep them)
                            while let Ok(Some(_row)) = rows.next() { }
                        }
                    }
                    Some(())
                } else { None }
            }).await.ok().flatten();

            if res.is_some() {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
                *notif.lock().await = "Beets sync complete".to_string();
            } else {
                *notif.lock().await = "Beets sync failed".to_string();
            }
        });
    }

    pub fn trigger_glitch(&mut self) {
        self.glitch_active = true;
        self.glitch_ticks = 5; // 500ms at 10Hz
    }

    fn cmd_download(&mut self, url: String) {
        let dir   = self.music_dir.clone()
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let notif = self.notification.clone();
        let flag  = self.reload_library_flag.clone();
        tokio::spawn(async move {
            match crate::downloader::download(&url, &dir, notif.clone()).await {
                Ok(_) => {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    *notif.lock().await = "Download complete — library rescanned".to_string();
                }
                Err(e) => {
                    *notif.lock().await = format!("Download failed: {}", e);
                }
            }
        });
    }

    fn set_notification(&mut self, msg: String) {
        let notif = self.notification.clone();
        tokio::spawn(async move { *notif.lock().await = msg; });
    }

    // ─── Auto-tag via MusicBrainz ──────────────────────────────────────────────

    pub fn auto_tag_current(&mut self) {
        let artist = self.current_artist.clone();
        let title  = self.current_track_name.clone();
        let notif  = self.notification.clone();
        tokio::spawn(async move {
            let q = format!(
                "artist:\"{}\" AND recording:\"{}\"",
                artist, title
            );
            let url = format!("https://musicbrainz.org/ws/2/recording/?query={}&fmt=json&limit=1", percent_encoding::utf8_percent_encode(&q, percent_encoding::NON_ALPHANUMERIC));
            let client = reqwest::Client::new();
            match client.get(&url)
                .header("User-Agent", "terminal-dj/0.1 (https://github.com)")
                .send().await
            {
                Ok(resp) => {
                    let json = resp.json::<serde_json::Value>().await.unwrap_or_default();
                    let msg = parse_mb_response(&json);
                    *notif.lock().await = msg;
                }
                Err(e) => { *notif.lock().await = format!("MusicBrainz error: {}", e); }
            }
        });
    }

    pub fn fetch_missing_cover(&mut self, artist: &str, title: &str) {
        let artist = artist.to_string();
        let title  = title.to_string();
        let pending = self.pending_cover_art.clone();
        
        tokio::spawn(async move {
            let client = reqwest::Client::new();
            // 1. Search MusicBrainz for recording
            let q = format!("artist:\"{}\" AND recording:\"{}\"", artist, title);
            let url = format!("https://musicbrainz.org/ws/2/recording/?query={}&fmt=json&limit=1", percent_encoding::utf8_percent_encode(&q, percent_encoding::NON_ALPHANUMERIC));
            
            let Ok(resp) = client.get(&url)
                .header("User-Agent", "terminal-dj/0.1")
                .send().await else { return; };
                
            let Ok(json) = resp.json::<serde_json::Value>().await else { return; };
            
            // 2. Get Release ID
            let release_id = json.pointer("/recordings/0/releases/0/id")
                .and_then(|v| v.as_str());
                
            if let Some(id) = release_id {
                // 3. Fetch from Cover Art Archive
                let caa_url = format!("https://coverartarchive.org/release/{}/front-250", id);
                if let Ok(art_resp) = client.get(&caa_url).send().await {
                    if let Ok(bytes) = art_resp.bytes().await {
                        *pending.lock().await = Some(bytes.to_vec());
                    }
                }
            }
        });
    }

    pub fn scrobble_listenbrainz(&self, artist: &str, title: &str) {
        let Some(token) = self.listenbrainz_token.clone() else { return; };
        let artist = artist.to_string();
        let title  = title.to_string();
        
        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let payload = serde_json::json!({
                "listen_type": "single",
                "payload": [{
                    "track_metadata": {
                        "artist_name": artist,
                        "track_name": title,
                    }
                }]
            });
            
            let _ = client.post("https://api.listenbrainz.org/1/submit-listens")
                .header("Authorization", format!("Token {}", token))
                .json(&payload)
                .send().await;
        });
    }

    // ─── Media controls state sync ────────────────────────────────────────────

    pub fn update_media_controls_state(&mut self) {
        use souvlaki::{MediaMetadata, MediaPlayback};
        let Some(ref mut c) = self.media_controls else { return; };
        let paused = self.audio_player.as_ref().map(|p| p.is_paused()).unwrap_or(true);
        let _ = c.set_playback(if paused {
            MediaPlayback::Paused { progress: None }
        } else {
            MediaPlayback::Playing { progress: None }
        });
        let _ = c.set_metadata(MediaMetadata {
            title:     Some(&self.current_track_name),
            artist:    Some(&self.current_artist),
            album:     None,
            cover_url: None,
            duration:  Some(self.total_duration),
        });
    }

    // ─── Misc helpers ──────────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub fn toggle_library_search(&mut self) {
        self.library_search_active = !self.library_search_active;
        if !self.library_search_active {
            self.library_search.clear();
            self.selected_track_index = 0;
        }
    }

    fn next_random(&mut self) -> u64 {
        self.rng_state = self.rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.rng_state
    }

    pub fn update_mood(&mut self) {
        let combined = format!("{} {}", self.current_track_name, self.current_artist).to_lowercase();
        self.mood_color = if combined.contains("chill") || combined.contains("lofi") || combined.contains("ambient") {
            Some(ratatui::style::Color::Rgb(0, 255, 255))
        } else if combined.contains("synthwave") || combined.contains("retro") || combined.contains("80s") {
            Some(ratatui::style::Color::Rgb(255, 0, 127))
        } else if combined.contains("rock") || combined.contains("metal") || combined.contains("punk") {
            Some(ratatui::style::Color::Rgb(255, 30, 30))
        } else if combined.contains("jazz") || combined.contains("soul") || combined.contains("blues") {
            Some(ratatui::style::Color::Rgb(255, 190, 0))
        } else if combined.contains("pop") || combined.contains("dance") || combined.contains("disco") {
            Some(ratatui::style::Color::Rgb(50, 255, 50))
        } else if combined.contains("dark") || combined.contains("techno") {
            Some(ratatui::style::Color::Rgb(60, 0, 255))
        } else { None };
    }

    pub fn current_bpm_label(&self) -> String {
        let bpm = self.current_bpm.lock()
            .ok()
            .and_then(|g| *g)
            .map(|b| format!("{:.0} BPM", b))
            .unwrap_or_else(|| "-- BPM".to_string());
            
        if let Some(ref vibe) = self.current_vibe {
            format!("{} [{}]", bpm, vibe)
        } else {
            bpm
        }
    }

    pub fn notification_text(&self) -> String {
        self.notification.try_lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}

// ─── MusicBrainz response parser ─────────────────────────────────────────────

fn parse_mb_response(json: &serde_json::Value) -> String {
    let rec = match json.get("recordings").and_then(|r| r.as_array()).and_then(|a| a.first()) {
        Some(r) => r,
        None    => return "No MusicBrainz match found".to_string(),
    };
    let title  = rec.get("title").and_then(|v| v.as_str()).unwrap_or("?");
    let artist = rec.pointer("/artist-credit/0/artist/name")
        .and_then(|v| v.as_str()).unwrap_or("?");
    let album  = rec.pointer("/releases/0/title").and_then(|v| v.as_str()).unwrap_or("?");
    let date   = rec.pointer("/releases/0/date").and_then(|v| v.as_str()).unwrap_or("?");
    format!("MB: {} — {} | {} ({})", title, artist, album, date)
}
