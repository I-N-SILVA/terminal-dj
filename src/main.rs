mod app;
mod assets;
mod audio;
mod bpm;
mod db;
mod downloader;
mod eq;
mod events;
mod karaoke;
mod lastfm;
mod library;
mod metadata;
mod playlist;
mod soundcloud;
mod spotify;
mod ui;

use app::App;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{error::Error, io, time::Duration};
use tracing_subscriber::fmt::writer::MakeWriterExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = dotenvy::dotenv();

    let mut log_path = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    log_path.push("terminal-dj");
    std::fs::create_dir_all(&log_path).ok();
    log_path.push("debug.log");
    let log_file = std::fs::OpenOptions::new()
        .create(true).append(true).open(&log_path)
        .expect("could not open debug.log");

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr.and(log_file))
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app    = App::new()?;
    let mut events = events::EventHandler::new(Duration::from_millis(100));

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        match events.next().await {
            Some(events::Event::Key(key)) => {
                match app.input_mode {
                    // ── Normal mode ───────────────────────────────────────────
                    app::InputMode::Normal => match key.code {
                        KeyCode::Char('q') => break,

                        // Tabs
                        KeyCode::Tab      => app.switch_tab(),
                        KeyCode::BackTab  => app.switch_tab_back(),
                        KeyCode::Char('1') => app.selected_tab = 0,
                        KeyCode::Char('2') => app.selected_tab = 1,
                        KeyCode::Char('3') => app.selected_tab = 2,
                        KeyCode::Char('4') => app.selected_tab = 3,
                        KeyCode::Char('5') => app.selected_tab = 4,
                        KeyCode::Char('6') => app.selected_tab = 5,

                        // Playback
                        KeyCode::Char(' ') => app.toggle_pause(),
                        KeyCode::Char('[') => app.volume_down(),
                        KeyCode::Char(']') => app.volume_up(),
                        KeyCode::Char(',') => app.seek_backward(),
                        KeyCode::Char('.') => app.seek_forward(),

                        // Shuffle / Repeat / Theme / Sleep
                        KeyCode::Char('S') => app.toggle_shuffle(),
                        KeyCode::Char('r') => app.cycle_repeat(),
                        KeyCode::Char('t') => app.cycle_theme(),
                        KeyCode::Char('T') => app.cycle_sleep_timer(),

                        // Queue
                        KeyCode::Char('e') => app.enqueue_selected(),

                        // Karaoke
                        KeyCode::Char('K') => app.toggle_karaoke(),

                        // Lyrics scroll
                        KeyCode::PageUp   => app.scroll_lyrics_up(),
                        KeyCode::PageDown => app.scroll_lyrics_down(),

                        // Zen mode
                        KeyCode::Char('z') => app.zen_mode = !app.zen_mode,

                        // Glitch effect
                        KeyCode::Char('g') => app.trigger_glitch(),

                        // Help
                        KeyCode::Char('?') => app.show_help = !app.show_help,

                        // EQ panel toggle (visualizer tab)
                        KeyCode::Char('E') => {
                            if app.selected_tab == 4 {
                                app.toggle_eq();
                            }
                        }

                        // Command mode
                        KeyCode::Char(':') => {
                            app.command_buffer.clear();
                            app.input_mode = app::InputMode::Command;
                        }

                        // Navigation
                        KeyCode::Left | KeyCode::Char('h') => {
                            if app.selected_tab == 4 && app.eq_focused {
                                app.eq_prev_band();
                            } else if app.selected_tab == 4 {
                                app.move_cursor(-1, 0);
                            } else {
                                app.switch_spotify_focus();
                            }
                        }
                        KeyCode::Right | KeyCode::Char('l') => {
                            if app.selected_tab == 4 && app.eq_focused {
                                app.eq_next_band();
                            } else if app.selected_tab == 4 {
                                app.move_cursor(1, 0);
                            } else if app.selected_tab == 2 && app.input_mode == app::InputMode::Normal {
                                app.login_spotify();
                            } else {
                                app.switch_spotify_focus();
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if app.selected_tab == 4 && app.eq_focused {
                                app.eq_adjust(-1.0);
                            } else if app.selected_tab == 4 {
                                app.move_cursor(0, 1);
                            } else {
                                app.next_track();
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if app.selected_tab == 4 && app.eq_focused {
                                app.eq_adjust(1.0);
                            } else if app.selected_tab == 4 {
                                app.move_cursor(0, -1);
                            } else {
                                app.previous_track();
                            }
                        }

                        // Visualizer mode
                        KeyCode::Char('m') => app.toggle_vis_mode(),

                        // Enter / confirm
                        KeyCode::Enter => {
                            if app.selected_tab == 2 || app.selected_tab == 3 {
                                app.play_selected_track();
                            } else {
                                app.enter_playlist();
                            }
                        }

                        // New playlist
                        KeyCode::Char('n') => {
                            app.input_mode = app::InputMode::Editing;
                            app.input.clear();
                        }

                        // Spotify / SoundCloud search
                        KeyCode::Char('s') => {
                            if app.selected_tab == 2 || app.selected_tab == 3 {
                                app.input_mode = app::InputMode::Editing;
                                app.input.clear();
                            }
                        }

                        // Add to playlist
                        KeyCode::Char('a') => app.add_selected_track_to_playlist(),

                        // Library search
                        KeyCode::Char('/') => {
                            if app.selected_tab == 0 {
                                app.library_search_active = true;
                                app.library_search.clear();
                                app.selected_track_index = 0;
                                app.input_mode = app::InputMode::Editing;
                            }
                        }

                        KeyCode::Esc | KeyCode::Backspace => {
                            app.exit_playlist();
                            app.show_help = false;
                            if app.eq_focused { app.eq_focused = false; }
                        }
                        _ => {}
                    },

                    // ── Editing mode ──────────────────────────────────────────
                    app::InputMode::Editing => match key.code {
                        KeyCode::Enter => {
                            if app.library_search_active {
                                app.input_mode = app::InputMode::Normal;
                            } else if app.selected_tab == 2 {
                                app.search_spotify();
                            } else if app.selected_tab == 3 {
                                app.search_soundcloud();
                            } else {
                                app.create_playlist();
                            }
                        }
                        KeyCode::Char(c) => {
                            if app.library_search_active {
                                app.library_search.push(c);
                                app.selected_track_index = 0;
                            } else {
                                app.input.push(c);
                            }
                        }
                        KeyCode::Backspace => {
                            if app.library_search_active {
                                app.library_search.pop();
                                app.selected_track_index = 0;
                            } else {
                                app.input.pop();
                            }
                        }
                        KeyCode::Esc => {
                            if app.library_search_active {
                                app.library_search.clear();
                                app.library_search_active = false;
                            }
                            app.input_mode = app::InputMode::Normal;
                        }
                        _ => {}
                    },

                    // ── Command mode ──────────────────────────────────────────
                    app::InputMode::Command => match key.code {
                        KeyCode::Enter => {
                            let cmd = app.command_buffer.clone();
                            app.input_mode = app::InputMode::Normal;
                            app.command_buffer.clear();
                            app.execute_command(cmd);
                        }
                        KeyCode::Char(c) => {
                            app.command_buffer.push(c);
                        }
                        KeyCode::Backspace => {
                            if app.command_buffer.is_empty() {
                                app.input_mode = app::InputMode::Normal;
                            } else {
                                app.command_buffer.pop();
                            }
                        }
                        KeyCode::Esc => {
                            app.command_buffer.clear();
                            app.input_mode = app::InputMode::Normal;
                        }
                        _ => {}
                    },
                }
            }
            Some(events::Event::Tick) => app.on_tick(),
            None => break,
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
