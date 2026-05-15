use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{BarChart, Block, Borders, List, ListItem, ListState, Paragraph, Wrap, Gauge, BorderType, Clear},
    Frame,
};

use crate::app::{App, InputMode, RepeatMode, SpotifyFocus, VisualizerMode};
use crate::assets::LOGO;
use ratatui::layout::Rect;

pub fn draw(f: &mut Frame, app: &mut App) {
    let primary_color   = app.mood_color.unwrap_or_else(|| app.color_theme.primary());
    let secondary_color = app.color_theme.secondary();
    let midnight_blue   = app.color_theme.bg();
    
    // Intensity Pulse: border color shifts toward white, and we use Double borders on peaks
    let border_color = if app.intensity > 0.6 {
        Color::White
    } else {
        primary_color
    };
    let border_type = if app.intensity > 0.7 {
        BorderType::Double
    } else {
        BorderType::Rounded
    };

    if app.zen_mode {
        draw_zen_mode(f, app, primary_color, secondary_color, midnight_blue, border_color, border_type);
        if app.glitch_active { draw_glitch_overlay(f, app); }
        return;
    }

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(0),
            Constraint::Length(10),
        ])
        .margin(if app.intensity > 0.8 { 1 } else { 0 })
        .split(f.area());

    // ── Header ────────────────────────────────────────────────────────────────
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(65), Constraint::Min(0)])
        .split(main_layout[0]);

    f.render_widget(
        Paragraph::new(LOGO).style(Style::default().fg(primary_color)),
        header_chunks[0],
    );

    let spotify_status_str = app.spotify_status.try_lock()
        .map(|s| s.clone()).unwrap_or_else(|_| "...".to_string());
    let connected = spotify_status_str.contains("Connected");
    let notif     = app.notification_text();
    let notif_line = if notif.is_empty() { String::new() } else { format!("\n   {}", notif) };

    // Playback paused indicator and library scan progress + spinner
    let paused = app.audio_player.as_ref().map(|p| p.is_paused()).unwrap_or(false);
    let pause_label = if paused { " (Paused)" } else { "" };
    let scanned = app.library.tracks.len();
    let spinner_chars = ["|", "/", "-", "\\"];
    let spinner = spinner_chars[((app.ticks as usize / 2) % spinner_chars.len())];

    let status_info = format!(
        "\n  {}  󰓇 Spotify: {}{}\n  ◈ {}\n  ◈ Library: {} tracks scanned\n   Tick: 10Hz | Theme: {}{}",
        spinner,
        if connected { "Connected ✓" } else { "Disconnected" },
        pause_label,
        &spotify_status_str,
        scanned,
        app.color_theme.name(),
        notif_line,
    );
    f.render_widget(
        Paragraph::new(status_info)
            .style(Style::default().fg(if connected { Color::Green } else { Color::DarkGray })),
        header_chunks[1],
    );

    // ── Main content ──────────────────────────────────────────────────────────
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    match app.selected_tab {
        0 | 1 | 2 | 3 => {
            let top_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(22), Constraint::Min(0)])
                .split(main_layout[1]);

            let sidebar_items = vec![
                ListItem::new(" 󰓇 Library"),
                ListItem::new(" 󰲸 Playlists"),
                ListItem::new("  Spotify"),
                ListItem::new("  SoundCloud"),
                ListItem::new(" 󰐊 Visualizer"),
                ListItem::new(" ♫  Now Playing"),
            ];
            let sidebar = List::new(sidebar_items)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Navigation ")
                    .border_style(Style::default().fg(border_color)))
                .highlight_style(Style::default().fg(secondary_color).add_modifier(Modifier::BOLD))
                .highlight_symbol(" ");
            let mut side_state = ListState::default();
            side_state.select(Some(app.selected_tab));
            f.render_stateful_widget(sidebar, top_layout[0], &mut side_state);

            render_tab_content(f, app, top_layout[1], &content_block, primary_color, secondary_color);
        }
        4 => {
            let has_art = app.current_cover_art.is_some();
            let show_eq = app.show_eq;

            // Layout: [EQ?] [visualizer] [art?] [lyrics]
            let mut constraints = Vec::new();
            if show_eq   { constraints.push(Constraint::Length(28)); }
            constraints.push(if has_art { Constraint::Percentage(35) } else { Constraint::Percentage(50) });
            if has_art   { constraints.push(Constraint::Length(22)); }
            constraints.push(Constraint::Min(0));

            let split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(constraints)
                .split(main_layout[1]);

            let mut idx = 0;
            if show_eq {
                render_eq_panel(f, app, split[idx], &content_block, primary_color, secondary_color);
                idx += 1;
            }
            render_visualizer(f, app, split[idx], &content_block, primary_color, midnight_blue);
            idx += 1;
            if has_art {
                render_cover_art(f, app, split[idx], &content_block);
                idx += 1;
            }
            render_lyrics(f, app, split[idx], &content_block, primary_color);
        }
        5 => render_now_playing(f, app, main_layout[1], primary_color, secondary_color, midnight_blue),
        _ => {}
    }

    // ── Footer ────────────────────────────────────────────────────────────────
    let footer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(secondary_color))
        .title(" System Status ");
    f.render_widget(footer_block, main_layout[2]);

    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Min(0), Constraint::Length(36)])
        .margin(1)
        .split(main_layout[2]);

    let paused    = app.audio_player.as_ref().map(|p| p.is_paused()).unwrap_or(false);
    let xfading   = app.audio_player.as_ref().map(|p| p.is_crossfading()).unwrap_or(false);
    let play_icon = if paused { "⏸" } else if xfading { "⇌" } else { "󰐊" };
    f.render_widget(
        Paragraph::new(format!("{} {}\n󰠃 {}", play_icon, app.current_track_name, app.current_artist))
            .style(Style::default().fg(primary_color)),
        footer_chunks[0],
    );

    let ratio = if app.total_duration.as_secs() > 0 {
        (app.playback_pos.as_secs_f64() / app.total_duration.as_secs_f64()).min(1.0)
    } else { 0.0 };
    let label = format!(
        "{:01}:{:02} / {:01}:{:02}",
        app.playback_pos.as_secs() / 60, app.playback_pos.as_secs() % 60,
        app.total_duration.as_secs() / 60, app.total_duration.as_secs() % 60,
    );
    f.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(primary_color).bg(midnight_blue).add_modifier(Modifier::ITALIC))
            .ratio(ratio)
            .label(label),
        footer_chunks[1],
    );

    let vol_pct      = (app.volume * 100.0).round() as u32;
    let repeat_label = match app.repeat { RepeatMode::Off => "Off", RepeatMode::One => "One", RepeatMode::All => "All" };
    let bpm_str      = app.current_bpm_label();
    let xfade_str    = if app.crossfade_secs > 0.0 { format!("{:.0}s", app.crossfade_secs) } else { "Off".to_string() };
    let eq_str       = if app.show_eq { "On" } else { "Off" };

    let control_text = match app.input_mode {
        InputMode::Command => format!(":{}_", app.command_buffer),
        InputMode::Normal => format!(
            " q:Quit ?:Help Tab Space:Pause\n ,:<<  .:>>  Vol:{}%  {}  XFade:{}\n 🔀:{} 🔁:{} Q:{} 💤:{} EQ:{}",
            vol_pct,
            bpm_str,
            xfade_str,
            if app.shuffle { "On" } else { "Off" },
            repeat_label,
            app.queue.len(),
            app.sleep_timer_label(),
            eq_str,
        ),
        InputMode::Editing => " Enter:Confirm  Esc:Cancel".to_string(),
    };
    f.render_widget(
        Paragraph::new(control_text.as_str()).style(Style::default().fg(
            if app.input_mode == InputMode::Command { primary_color } else { Color::Gray }
        )),
        footer_chunks[2],
    );

    // ── Help overlay ──────────────────────────────────────────────────────────
    if app.show_help {
        let area = centered_rect(66, 90, f.area());
        f.render_widget(Clear, area);
        let help = r#"
  󰎆 TERMINAL DJ — KEYMAPS

  Global:
    q          : Quit          ?  : Toggle Help
    Tab/S-Tab  : Cycle Tabs    1-6: Jump to Tab
    Space      : Pause/Resume  m  : Cycle Visualizer
    t          : Cycle Theme   T  : Sleep Timer
    [/]        : Volume        ,/.: Seek ±5s
    :          : Command mode  (type command, Enter to run)

  Library / Playlists:
    j/k ↑↓     : Navigate      Enter : Play/Enter
    /           : Search        a     : Add to Playlist
    e           : Enqueue       Esc   : Back

  SoundCloud (tab 4):
    s          : Search        j/k   : Navigate results
    Enter      : Stream track

  Playback:
    S          : Shuffle On/Off
    r          : Repeat Off→One→All
    PgUp/PgDn  : Scroll Lyrics
    K          : Toggle Karaoke
    6          : Now Playing full-screen view

  Visualizer (tab 5):
    m          : Cycle vis mode
    E          : Toggle EQ panel
    h/l        : (EQ mode) select band
    j/k        : (EQ mode) adjust ±1 dB

  Commands:
    :download <url>      Download via yt-dlp
    :theme <name>        neon/amber/mono/dracula/matrix
    :vol <0-100>         Set volume
    :xfade <seconds>     Crossfade duration (0=off)
    :sleep <minutes>     Sleep timer
    :eq <band 1-10> <dB> Set EQ band (-12 to +12)
    :eq reset            Reset all EQ bands
    :tag                 Lookup track on MusicBrainz

  Discord: set DISCORD_APP_ID env var
  Last.fm: set LASTFM_API_KEY + LASTFM_API_SECRET
  SoundCloud: set SOUNDCLOUD_CLIENT_ID
        "#;
        f.render_widget(
            Paragraph::new(help)
                .block(Block::default()
                    .title(" HELP ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Double)
                    .border_style(Style::default().fg(secondary_color)))
                .wrap(Wrap { trim: true }),
            area,
        );
    }
    
    if app.glitch_active { draw_glitch_overlay(f, app); }
}

// ─── Tab content ─────────────────────────────────────────────────────────────

fn render_tab_content(
    f: &mut Frame, app: &mut App, area: Rect,
    content_block: &Block, primary_color: Color, secondary_color: Color,
) {
    match app.selected_tab {
        0 => {
            let filtered = app.filtered_library();
            let items: Vec<ListItem> = filtered.iter()
                .map(|(orig_idx, path)| {
                    let is_playing = app.playing_playlist_index.is_none()
                        && app.playing_track_index == Some(*orig_idx);
                    let display = if let Some(meta) = app.library.metadata.get(*path) {
                        format!(" 󰎆 {} — {}", meta.display_title(path), meta.display_artist())
                    } else {
                        format!(" 󰎆 {}", path.file_name().unwrap_or_default().to_string_lossy())
                    };
                    if is_playing {
                        ListItem::new(format!(" 󰐊 {}", &display[5..]))
                            .style(Style::default().fg(primary_color).add_modifier(Modifier::BOLD))
                    } else {
                        ListItem::new(display)
                    }
                })
                .collect();

            if app.library_search_active || !app.library_search.is_empty() {
                let chunks = Layout::default()
                    .constraints([Constraint::Length(3), Constraint::Min(0)])
                    .split(area);
                f.render_widget(
                    Paragraph::new(format!(" / {}_", app.library_search))
                        .block(Block::default().borders(Borders::ALL).title(" Search ")),
                    chunks[0],
                );
                let list = List::new(items)
                    .block(content_block.clone().title(" Library "))
                    .highlight_symbol(" ")
                    .highlight_style(Style::default().bg(Color::Rgb(30, 30, 60)).add_modifier(Modifier::BOLD));
                let mut state = ListState::default();
                state.select(Some(app.selected_track_index));
                f.render_stateful_widget(list, chunks[1], &mut state);
            } else {
                let list = List::new(items)
                    .block(content_block.clone().title(" Library "))
                    .highlight_symbol(" ")
                    .highlight_style(Style::default().bg(Color::Rgb(30, 30, 60)).add_modifier(Modifier::BOLD));
                let mut state = ListState::default();
                state.select(Some(app.selected_track_index));
                f.render_stateful_widget(list, area, &mut state);
            }
        }
        1 => {
            if let Some(idx) = app.viewing_playlist_index {
                if let Some(p) = app.playlists.get(idx) {
                    let items: Vec<ListItem> = p.tracks.iter()
                        .map(|path| ListItem::new(format!(
                            " 󰎄 {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        )))
                        .collect();
                    let list = List::new(items)
                        .block(content_block.clone().title(format!(" PL: {} ", p.name)))
                        .highlight_symbol(" ")
                        .highlight_style(Style::default().bg(Color::Rgb(30, 30, 60)).add_modifier(Modifier::BOLD));
                    let mut state = ListState::default();
                    state.select(Some(app.selected_playlist_track_index));
                    f.render_stateful_widget(list, area, &mut state);
                }
            } else {
                let items: Vec<ListItem> = app.playlists.iter()
                    .map(|p| ListItem::new(format!(" 󰲸 {} ({} tracks)", p.name, p.tracks.len())))
                    .collect();
                let list = List::new(items)
                    .block(content_block.clone().title(" Playlists "))
                    .highlight_symbol(" ")
                    .highlight_style(Style::default().bg(Color::Rgb(30, 30, 60)).add_modifier(Modifier::BOLD));
                let mut state = ListState::default();
                state.select(Some(app.selected_playlist_index));
                f.render_stateful_widget(list, area, &mut state);
            }
        }
        2 => {
            if app.input_mode == InputMode::Editing {
                f.render_widget(
                    Paragraph::new(app.input.as_str())
                        .block(content_block.clone().title(" Spotify Search ")),
                    area,
                );
            } else {
                let spotify_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(0)])
                    .split(area);

                let status_str  = app.spotify_status.try_lock().map(|s| s.clone()).unwrap_or_else(|_| "...".to_string());
                let is_conn     = status_str.contains("Connected");
                let status_style = if is_conn {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else if status_str.contains("failed") || status_str.contains("expired") {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::Yellow)
                };
                let hint = if is_conn { " Enter: play  s: search  h/l: panel" } else { " Press 'l' to login" };
                f.render_widget(
                    Paragraph::new(format!(" ◈ {}  |{}", status_str, hint))
                        .style(status_style)
                        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)),
                    spotify_layout[0],
                );

                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(spotify_layout[1]);

                let pl_border = if app.spotify_focus == SpotifyFocus::Playlists { secondary_color } else { primary_color };
                let playlist_items: Vec<ListItem> = match app.spotify_playlists.try_lock() {
                    Ok(p) if !p.is_empty() => p.iter().map(|pl| ListItem::new(format!(" 󰲸 {}", pl.name))).collect(),
                    Ok(_) => vec![ListItem::new(" No playlists. Press 'l' to connect.").style(Style::default().fg(Color::DarkGray))],
                    Err(_) => vec![ListItem::new(" Loading…").style(Style::default().fg(Color::DarkGray))],
                };
                let mut pl_state = ListState::default();
                pl_state.select(Some(app.spotify_playlist_index));
                f.render_stateful_widget(
                    List::new(playlist_items)
                        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
                            .title(" Your Playlists ").border_style(Style::default().fg(pl_border)))
                        .highlight_style(Style::default().bg(Color::Rgb(30, 30, 60)).add_modifier(Modifier::BOLD))
                        .highlight_symbol(" "),
                    chunks[0], &mut pl_state,
                );

                let sr_border = if app.spotify_focus == SpotifyFocus::Search { secondary_color } else { primary_color };
                let search_items: Vec<ListItem> = match app.spotify_search_results.try_lock() {
                    Ok(r) if !r.is_empty() => r.iter().map(|t| ListItem::new(format!(" 󰓇 {} — {}", t.name, t.artists[0].name))).collect(),
                    Ok(_) => vec![ListItem::new(" Press 's' to search.").style(Style::default().fg(Color::DarkGray))],
                    Err(_) => vec![ListItem::new(" Loading…").style(Style::default().fg(Color::DarkGray))],
                };
                let mut sr_state = ListState::default();
                sr_state.select(Some(app.spotify_search_index));
                f.render_stateful_widget(
                    List::new(search_items)
                        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
                            .title(" Search Results ").border_style(Style::default().fg(sr_border)))
                        .highlight_style(Style::default().bg(Color::Rgb(30, 30, 60)).add_modifier(Modifier::BOLD))
                        .highlight_symbol(" "),
                    chunks[1], &mut sr_state,
                );
            }
        }
        3 => {
            // SoundCloud
            if app.input_mode == InputMode::Editing {
                f.render_widget(
                    Paragraph::new(app.input.as_str())
                        .block(content_block.clone().title(" SoundCloud Search ")),
                    area,
                );
            } else {
                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(0)])
                    .split(area);

                let sc_status = app.soundcloud_status.try_lock()
                    .map(|s| s.clone()).unwrap_or_else(|_| "…".to_string());
                let has_client = app.soundcloud_client.is_some();
                f.render_widget(
                    Paragraph::new(format!("  {}  |  s: search  j/k: navigate  Enter: stream", sc_status))
                        .style(Style::default().fg(if has_client { Color::Green } else { Color::Yellow }))
                        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)),
                    layout[0],
                );

                let results = app.soundcloud_results.try_lock()
                    .map(|g| g.clone()).unwrap_or_default();
                let items: Vec<ListItem> = if results.is_empty() {
                    vec![ListItem::new("  No results — press 's' to search")
                        .style(Style::default().fg(Color::DarkGray))]
                } else {
                    results.iter().map(|t| {
                        ListItem::new(format!(
                            "  {} — {}  [{}]",
                            t.title, t.display_artist(), t.duration_label()
                        ))
                    }).collect()
                };
                let mut sc_state = ListState::default();
                sc_state.select(Some(app.soundcloud_search_index));
                f.render_stateful_widget(
                    List::new(items)
                        .block(content_block.clone().title(" SoundCloud "))
                        .highlight_symbol("  ")
                        .highlight_style(Style::default().bg(Color::Rgb(255, 85, 0)).fg(Color::Black).add_modifier(Modifier::BOLD)),
                    layout[1], &mut sc_state,
                );
            }
        }
        _ => {}
    }
}

// ─── EQ Panel ────────────────────────────────────────────────────────────────

fn render_eq_panel(
    f: &mut Frame, app: &App, area: Rect,
    block: &Block, primary: Color, secondary: Color,
) {
    use crate::eq::BAND_LABELS;

    let gains  = app.eq_gains_snapshot();
    let sel    = app.eq_selected_band;
    let focused = app.eq_focused;

    let inner = block.inner(area);
    let title = if focused { " EQ [h/l: band  j/k: dB  E: exit] " } else { " EQ [E: focus] " };
    f.render_widget(block.clone().title(title), area);

    if inner.height < 4 { return; }

    // Bar chart: offset gains by 12 so range 0–24 fits positive-only BarChart.
    let bar_data: Vec<(String, u64)> = gains.iter().enumerate()
        .map(|(i, &g)| {
            let label = if i == sel && focused {
                format!("[{}]", BAND_LABELS[i])
            } else {
                BAND_LABELS[i].to_string()
            };
            (label, ((g + 12.0).round() as i32).max(0) as u64)
        })
        .collect();

    let bar_refs: Vec<(&str, u64)> = bar_data.iter().map(|(s, v)| (s.as_str(), *v)).collect();

    let bar_color = if focused { primary } else { secondary };
    f.render_widget(
        BarChart::default()
            .block(Block::default())
            .data(&bar_refs)
            .bar_width(2)
            .bar_gap(0)
            .max(24)
            .bar_style(Style::default().fg(bar_color))
            .value_style(Style::default().fg(Color::Black).bg(bar_color)),
        inner,
    );

    // Show current band gain value at bottom
    let gain_text = format!(
        "{}: {:+.1} dB",
        BAND_LABELS[sel],
        gains[sel],
    );
    let text_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
    };
    f.render_widget(
        Paragraph::new(gain_text)
            .style(Style::default().fg(primary).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center),
        text_area,
    );
}

// ─── Visualizer ──────────────────────────────────────────────────────────────

fn render_visualizer(
    f: &mut Frame, app: &App, area: Rect,
    block: &Block, color: Color, bg_color: Color,
) {
    match app.vis_mode {
        VisualizerMode::Bars => {
            let bars_data: Vec<(&str, u64)> = app.spectrum_data.iter()
                .map(|&v| ("", (v * 100.0) as u64))
                .collect();
            f.render_widget(
                BarChart::default()
                    .block(block.clone().title(" Spectrum Analyzer [m: mode] "))
                    .data(&bars_data)
                    .bar_width(3).bar_gap(1)
                    .bar_style(Style::default().fg(color))
                    .value_style(Style::default().fg(bg_color).bg(color)),
                area,
            );
        }
        VisualizerMode::Oscilloscope => {
            let inner = block.inner(area);
            if inner.width == 0 || inner.height == 0 { return; }
            let w = inner.width as usize;
            let h = inner.height as usize;
            let mid = h / 2;
            let samples = &app.recent_samples;

            let mut grid = vec![vec![' '; w]; h];
            for x in 0..w { grid[mid][x] = '─'; }

            if !samples.is_empty() {
                let step = (samples.len() as f32 / w as f32).max(1.0);
                for x in 0..w {
                    let idx = (x as f32 * step) as usize;
                    let s   = samples.get(idx).copied().unwrap_or(0.0).clamp(-1.0, 1.0);
                    let y   = (mid as f32 - s * (mid as f32 * 0.9)) as usize;
                    grid[y.min(h - 1)][x] = '●';
                }
            }
            let text: String = grid.iter()
                .map(|row| row.iter().collect::<String>())
                .collect::<Vec<_>>().join("\n");
            f.render_widget(
                Paragraph::new(text)
                    .block(block.clone().title(" Oscilloscope [m: mode] "))
                    .style(Style::default().fg(color)),
                inner,
            );
        }
        VisualizerMode::Cyberfield => {
            let inner = block.inner(area);
            if inner.width == 0 || inner.height == 0 { return; }
            let cx = (app.cursor_pos.0 as f32 / 100.0 * inner.width  as f32) as u16;
            let cy = (app.cursor_pos.1 as f32 /  40.0 * inner.height as f32) as u16;
            let mut lines = Vec::new();
            for y in 0..inner.height {
                let mut row = String::new();
                for x in 0..inner.width {
                    let dx   = x as f32 - cx as f32;
                    let dy   = (y as f32 - cy as f32) * 2.0;
                    let dist = (dx * dx + dy * dy).sqrt();
                    let idx  = ((x as usize * 64) / inner.width as usize).min(63);
                    let val  = app.spectrum_data[idx];
                    let ripple = ((dist * 0.5 - (app.ticks as f32 * 0.2)).sin() + 1.0) * 0.5;
                    if dist < 1.0 {
                        row.push('✸');
                    } else if ripple > 0.5 + val * 0.5 {
                        let chars = [' ', '⠁', '⠃', '⠇', '⠏', '⠟', '⠿', '⣿'];
                        row.push(chars[((val * 7.0) as usize).min(7)]);
                    } else {
                        row.push(' ');
                    }
                }
                lines.push(row);
            }
            f.render_widget(
                Paragraph::new(lines.join("\n"))
                    .block(block.clone().title(" Cyberfield [ARROWS: Move] "))
                    .style(Style::default().fg(color)),
                inner,
            );
        }
        VisualizerMode::Matrix => {
            let inner = block.inner(area);
            let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789@#$%^&*()";
            let cx = (app.cursor_pos.0 as f32 / 100.0 * inner.width  as f32) as u16;
            let cy = (app.cursor_pos.1 as f32 /  40.0 * inner.height as f32) as u16;
            let mut lines = Vec::new();
            for y in 0..inner.height {
                let mut row = String::new();
                for x in 0..inner.width {
                    let dist = (((x as f32 - cx as f32).powi(2) + ((y as f32 - cy as f32) * 2.0).powi(2)).sqrt()) as u16;
                    let seed = (x as u64 * 1337 + app.ticks / 2) % (inner.height as u64 + 10);
                    let is_active = (y as u64 + seed) % 15 < 5;
                    let idx  = ((x as usize * 64) / inner.width as usize).min(63);
                    let val  = app.spectrum_data[idx];
                    if dist < 3 {
                        row.push(' ');
                    } else if is_active || (((val * 10.0) as u64) > 5 && x % 4 == 0) {
                        row.push(chars.as_bytes()[(app.ticks as usize + x as usize + y as usize) % chars.len()] as char);
                    } else {
                        row.push(' ');
                    }
                }
                lines.push(row);
            }
            f.render_widget(
                Paragraph::new(lines.join("\n"))
                    .block(block.clone().title(" Matrix Rain "))
                    .style(Style::default().fg(Color::Green)),
                inner,
            );
        }
        VisualizerMode::Plasma => {
            let inner = block.inner(area);
            let mut lines = Vec::new();
            for y in 0..inner.height {
                let mut row = String::new();
                for x in 0..inner.width {
                    let tx = x as f32 * 0.1;
                    let ty = y as f32 * 0.2;
                    let t  = app.ticks as f32 * 0.1;
                    let v  = (tx + t).sin() + (ty + t).sin()
                           + (tx + ty + t).sin() + ((tx * tx + ty * ty).sqrt() + t).sin();
                    let nv = (v + 4.0) / 8.0;
                    let sm = app.spectrum_data[((x as usize * 64) / inner.width as usize).min(63)] * 2.0;
                    row.push(if nv + sm * 0.2 > 0.6 { '▓' } else if nv + sm * 0.1 > 0.4 { '▒' } else { ' ' });
                }
                lines.push(row);
            }
            f.render_widget(
                Paragraph::new(lines.join("\n"))
                    .block(block.clone().title(" Plasma Wave "))
                    .style(Style::default().fg(Color::Magenta)),
                inner,
            );
        }
    }
}

// ─── Cover art ───────────────────────────────────────────────────────────────

fn render_cover_art(f: &mut Frame, app: &App, area: Rect, block: &Block) {
    let Some(ref art_bytes) = app.current_cover_art else {
        f.render_widget(
            Paragraph::new("No art").block(block.clone().title(" Cover Art ")).alignment(Alignment::Center),
            area,
        );
        return;
    };

    let inner = block.inner(area);
    if inner.width == 0 || inner.height == 0 { return; }

    let Ok(img) = image::load_from_memory(art_bytes) else {
        f.render_widget(block.clone().title(" Cover Art "), area);
        return;
    };

    let tw   = inner.width as u32;
    let th   = (inner.height as u32) * 2;
    let rgb  = img.resize_exact(tw, th, image::imageops::FilterType::Nearest).to_rgb8();

    let mut lines: Vec<Line> = Vec::new();
    for row in (0..th).step_by(2) {
        let mut spans: Vec<Span> = Vec::new();
        for col in 0..tw {
            let upper = *rgb.get_pixel(col, row);
            let lower = *rgb.get_pixel(col, (row + 1).min(th - 1));
            spans.push(Span::styled(
                "▄",
                Style::default()
                    .fg(Color::Rgb(lower[0], lower[1], lower[2]))
                    .bg(Color::Rgb(upper[0], upper[1], upper[2])),
            ));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(
        Paragraph::new(Text::from(lines)).block(block.clone().title(" Cover Art ")),
        area,
    );
}

// ─── Lyrics / Karaoke ────────────────────────────────────────────────────────

fn render_lyrics(f: &mut Frame, app: &App, area: Rect, block: &Block, primary_color: Color) {
    if app.karaoke_active {
        if let Ok(lines) = app.karaoke_lines.try_lock() {
            if !lines.is_empty() {
                let pos     = app.playback_pos;
                let current = crate::karaoke::current_line_index(&lines, pos);
                let visible = area.height.saturating_sub(2) as usize;
                let auto_scroll = (current.saturating_sub(visible / 2)) as u16;

                let text_lines: Vec<Line> = lines.iter().enumerate().map(|(i, l)| {
                    if i == current {
                        Line::styled(format!("▶ {}", l.text),
                            Style::default().fg(primary_color).add_modifier(Modifier::BOLD))
                    } else {
                        Line::styled(l.text.clone(), Style::default().fg(Color::DarkGray))
                    }
                }).collect();

                f.render_widget(
                    Paragraph::new(Text::from(text_lines))
                        .block(block.clone().title(" Karaoke Mode [K: toggle] "))
                        .scroll((auto_scroll.saturating_add(app.lyrics_scroll), 0)),
                    area,
                );
                return;
            }
        }
    }

    let lyrics = app.current_lyrics.try_lock()
        .map(|g| if g.is_empty() { "Play a song to see lyrics...".to_string() } else { g.clone() })
        .unwrap_or_else(|_| "Loading...".to_string());

    f.render_widget(
        Paragraph::new(lyrics)
            .block(block.clone().title(" Lyrics  [PgUp/PgDn: scroll  K: karaoke] "))
            .wrap(Wrap { trim: true })
            .scroll((app.lyrics_scroll, 0)),
        area,
    );
}

// ─── Now Playing ─────────────────────────────────────────────────────────────

fn render_now_playing(
    f: &mut Frame, app: &App, area: Rect,
    primary: Color, secondary: Color, bg: Color,
) {
    use crate::eq::BAND_LABELS;

    // Three columns: art | info | lyrics
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(40), Constraint::Percentage(30)])
        .split(area);

    // ── Left: large cover art ─────────────────────────────────────────────────
    if let Some(ref art_bytes) = app.current_cover_art {
        let block = Block::default().borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(primary));
        let inner = block.inner(cols[0]);
        f.render_widget(block, cols[0]);

        if inner.width > 0 && inner.height > 0 {
            if let Ok(img) = image::load_from_memory(art_bytes) {
                let tw = inner.width as u32;
                let th = (inner.height as u32) * 2;
                let rgb = img.resize_exact(tw, th, image::imageops::FilterType::Lanczos3).to_rgb8();
                let mut lines: Vec<Line> = Vec::new();
                for row in (0..th).step_by(2) {
                    let mut spans: Vec<Span> = Vec::new();
                    for col in 0..tw {
                        let upper = *rgb.get_pixel(col, row);
                        let lower = *rgb.get_pixel(col, (row + 1).min(th - 1));
                        spans.push(Span::styled("▄",
                            Style::default()
                                .fg(Color::Rgb(lower[0], lower[1], lower[2]))
                                .bg(Color::Rgb(upper[0], upper[1], upper[2]))));
                    }
                    lines.push(Line::from(spans));
                }
                f.render_widget(Paragraph::new(Text::from(lines)), inner);
            }
        }
    } else {
        let ascii = format!(
            "\n\n\n{:^w$}\n{:^w$}\n{:^w$}",
            "◈", "♫", "◈",
            w = cols[0].width.saturating_sub(2) as usize,
        );
        f.render_widget(
            Paragraph::new(ascii)
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Double)
                    .border_style(Style::default().fg(primary)))
                .style(Style::default().fg(secondary))
                .alignment(Alignment::Center),
            cols[0],
        );
    }

    // ── Centre: track info + waveform + stats ─────────────────────────────────
    let info_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),  // title + artist
            Constraint::Length(3),  // progress gauge
            Constraint::Length(4),  // waveform scrubber
            Constraint::Length(2),  // BPM / vol / norm
            Constraint::Length(4),  // mini EQ
            Constraint::Min(0),     // mini spectrum
        ])
        .margin(1)
        .split(cols[1]);

    // Title + artist
    let paused  = app.audio_player.as_ref().map(|p| p.is_paused()).unwrap_or(false);
    let xfading = app.audio_player.as_ref().map(|p| p.is_crossfading()).unwrap_or(false);
    let play_icon = if paused { "⏸" } else if xfading { "⇌" } else { "▶" };
    f.render_widget(
        Paragraph::new(format!(
            "{} {}\n   {}",
            play_icon,
            app.current_track_name,
            app.current_artist,
        ))
        .style(Style::default().fg(primary).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
            .border_style(Style::default().fg(primary))),
        info_rows[0],
    );

    // Progress gauge
    let ratio = if app.total_duration.as_secs() > 0 {
        (app.playback_pos.as_secs_f64() / app.total_duration.as_secs_f64()).min(1.0)
    } else { 0.0 };
    let label = format!(
        "{:01}:{:02} / {:01}:{:02}",
        app.playback_pos.as_secs() / 60, app.playback_pos.as_secs() % 60,
        app.total_duration.as_secs() / 60, app.total_duration.as_secs() % 60,
    );
    f.render_widget(
        Gauge::default()
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
            .gauge_style(Style::default().fg(primary).bg(bg))
            .ratio(ratio)
            .label(label),
        info_rows[1],
    );

    // Waveform scrubber
    {
        let wf = &app.waveform_data;
        let w  = info_rows[2].width.saturating_sub(2) as usize;
        let cursor = if app.total_duration.as_secs_f32() > 0.0 {
            ((app.playback_pos.as_secs_f32() / app.total_duration.as_secs_f32()) * w as f32) as usize
        } else { 0 };
        let block_chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let mut spans: Vec<Span> = Vec::new();
        for x in 0..w {
            let idx   = (x * wf.len()) / w.max(1);
            let amp   = wf.get(idx).copied().unwrap_or(0.0);
            let ch    = block_chars[(amp * 7.0) as usize % 8];
            let style = if x == cursor {
                Style::default().fg(Color::White).bg(primary)
            } else if x < cursor {
                Style::default().fg(primary)
            } else {
                Style::default().fg(Color::Rgb(60, 60, 90))
            };
            spans.push(Span::styled(ch.to_string(), style));
        }
        let inner = Block::default().borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Waveform ")
            .border_style(Style::default().fg(secondary))
            .inner(info_rows[2]);
        f.render_widget(
            Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
                .title(" Waveform ").border_style(Style::default().fg(secondary)),
            info_rows[2],
        );
        f.render_widget(Paragraph::new(Line::from(spans)), inner);
    }

    // Stats row
    let bpm_str   = app.current_bpm_label();
    let vol_pct   = (app.volume * 100.0).round() as u32;
    let gain_str  = if (app.current_norm_gain - 1.0).abs() < 0.05 {
        "norm: flat".to_string()
    } else {
        format!("norm: {:+.1} dB", 20.0 * app.current_norm_gain.log10())
    };
    f.render_widget(
        Paragraph::new(format!("  ♩ {}   vol {}%   {}", bpm_str, vol_pct, gain_str))
            .style(Style::default().fg(secondary)),
        info_rows[3],
    );

    // Mini EQ
    {
        let gains = app.eq_gains_snapshot();
        let bar_data: Vec<(String, u64)> = gains.iter().enumerate()
            .map(|(i, &g)| (BAND_LABELS[i].to_string(), ((g + 12.0).round() as i32).max(0) as u64))
            .collect();
        let bar_refs: Vec<(&str, u64)> = bar_data.iter().map(|(s, v)| (s.as_str(), *v)).collect();
        f.render_widget(
            BarChart::default()
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
                    .title(" EQ ").border_style(Style::default().fg(secondary)))
                .data(&bar_refs)
                .bar_width(2).bar_gap(0).max(24)
                .bar_style(Style::default().fg(primary))
                .value_style(Style::default().fg(bg).bg(primary)),
            info_rows[4],
        );
    }

    // Mini spectrum
    {
        let bars: Vec<(&str, u64)> = app.spectrum_data.iter()
            .step_by(4)
            .map(|&v| ("", (v * 100.0) as u64))
            .collect();
        f.render_widget(
            BarChart::default()
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
                    .title(" Spectrum ").border_style(Style::default().fg(secondary)))
                .data(&bars)
                .bar_width(2).bar_gap(1).max(100)
                .bar_style(Style::default().fg(secondary))
                .value_style(Style::default().fg(bg).bg(secondary)),
            info_rows[5],
        );
    }

    // ── Right: lyrics/karaoke ─────────────────────────────────────────────────
    let lyr_block = Block::default().borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(secondary));
    render_lyrics(f, app, cols[2], &lyr_block, primary);
}

fn draw_zen_mode(
    f: &mut Frame, app: &mut App,
    primary: Color, _secondary: Color, bg: Color,
    border_color: Color, border_type: BorderType,
) {
    let area = f.area();
    let has_art = app.current_cover_art.is_some();
    
    // Zen Mode Layout: [Visualizer] [Art?] [Lyrics]
    let constraints = if has_art {
        [Constraint::Percentage(40), Constraint::Length(30), Constraint::Min(0)]
    } else {
        [Constraint::Percentage(60), Constraint::Length(0), Constraint::Min(0)]
    };
    
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .margin(if app.intensity > 0.8 { 1 } else { 0 })
        .constraints(constraints)
        .split(area);
        
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(border_color));
        
    render_visualizer(f, app, layout[0], &content_block, primary, bg);
    
    if has_art {
        render_cover_art(f, app, layout[1], &content_block);
    }
    
    render_lyrics(f, app, layout[2], &content_block, primary);
    
    // Add a tiny subtle progress bar at the very bottom
    let ratio = if app.total_duration.as_secs() > 0 {
        (app.playback_pos.as_secs_f64() / app.total_duration.as_secs_f64()).min(1.0)
    } else { 0.0 };
    
    let gauge_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    f.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(primary).bg(bg))
            .ratio(ratio)
            .label(""),
        gauge_area,
    );
}

// ─── Utility ─────────────────────────────────────────────────────────────────

fn draw_glitch_overlay(f: &mut Frame, _app: &App) {
    // Minimal placeholder overlay for glitch effect.
    // Keeps rendering simple while the real effect is implemented.
    let area = f.area();
    // Render an empty paragraph with a subtle style so the call site compiles.
    f.render_widget(
        Paragraph::new("").block(Block::default()),
        area,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
