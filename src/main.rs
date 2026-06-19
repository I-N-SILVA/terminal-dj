mod app;
mod assets;
mod audio;
mod downloader;
mod events;
mod library;
mod metadata;
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
        .create(true)
        .append(true)
        .open(&log_path)
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

    let mut app = App::new()?;
    let mut events = events::EventHandler::new(Duration::from_millis(100));

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        match events.next().await {
            Some(events::Event::Key(key)) => match app.input_mode {
                app::InputMode::Normal => match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Down | KeyCode::Char('j') => app.next_track(),
                    KeyCode::Up | KeyCode::Char('k') => app.previous_track(),
                    KeyCode::Char(' ') => app.toggle_pause(),
                    KeyCode::Char('v') => app.toggle_visualizer(),
                    KeyCode::Char('[') => app.volume_down(),
                    KeyCode::Char(']') => app.volume_up(),
                    KeyCode::Char(',') => app.seek_backward(),
                    KeyCode::Char('.') => app.seek_forward(),
                    KeyCode::Char('/') => {
                        app.library_search_active = true;
                        app.library_search.clear();
                        app.selected_track_index = 0;
                        app.input_mode = app::InputMode::Editing;
                    }
                    KeyCode::Char('d') => {
                        app.input_mode = app::InputMode::Downloading;
                        app.download_url.clear();
                    }
                    KeyCode::Enter => {
                        app.play_selected_track();
                    }
                    _ => {}
                },
                app::InputMode::Editing => match key.code {
                    KeyCode::Enter => {
                        app.input_mode = app::InputMode::Normal;
                    }
                    KeyCode::Char(c) => {
                        if app.library_search_active {
                            app.library_search.push(c);
                            app.selected_track_index = 0;
                        }
                    }
                    KeyCode::Backspace => {
                        if app.library_search_active {
                            app.library_search.pop();
                            app.selected_track_index = 0;
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
                app::InputMode::Downloading => match key.code {
                    KeyCode::Enter => {
                        let url = app.download_url.clone();
                        app.input_mode = app::InputMode::Normal;
                        app.download_url.clear();
                        app.download_music(url);
                    }
                    KeyCode::Char(c) => {
                        app.download_url.push(c);
                    }
                    KeyCode::Backspace => {
                        app.download_url.pop();
                    }
                    KeyCode::Esc => {
                        app.download_url.clear();
                        app.input_mode = app::InputMode::Normal;
                    }
                    _ => {}
                },
            },
            Some(events::Event::Tick) => app.on_tick(),
            None => break,
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
