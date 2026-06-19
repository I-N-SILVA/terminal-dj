use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, InputMode};
use crate::assets::LOGO;

pub fn draw(f: &mut Frame, app: &mut App) {
    let _bg_color = Color::Rgb(10, 10, 15);
    let synth_pink = Color::Rgb(255, 0, 255); // Magenta
    let synth_cyan = Color::Rgb(0, 255, 255); // Cyan
    let synth_green = Color::Rgb(57, 255, 20); // Neon Green

    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // Left: Cover art + Library
            Constraint::Percentage(70), // Right: Visualizer + Player
        ])
        .margin(1)
        .split(f.area());

    // ── Left Column ──────────────────────────────────────────────────────────
    let left_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(16), // Cover Art
            Constraint::Min(0),     // Library
        ])
        .split(main_layout[0]);

    // Cover Art
    let cover_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(synth_pink))
        .title(" 󰎆 Cover ");

    if let Some(state) = &mut app.cover_image_state {
        f.render_widget(cover_block.clone(), left_layout[0]);
        let inner_area = cover_block.inner(left_layout[0]);
        let image_widget = ratatui_image::StatefulImage::default();
        f.render_stateful_widget(image_widget, inner_area, state);
    } else {
        f.render_widget(
            Paragraph::new(LOGO)
                .style(Style::default().fg(synth_pink))
                .block(cover_block),
            left_layout[0],
        );
    }

    // Library
    let filtered = app.filtered_library();
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, (_orig_idx, p))| {
            let is_selected = i == app.selected_track_index;

            let meta = app.library.metadata.get(*p);
            let title = if let Some(m) = meta {
                format!("{} - {}", m.display_artist(), m.display_title(p))
            } else {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            };

            let style = if is_selected {
                Style::default()
                    .fg(synth_green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            ListItem::new(format!(" {} ", title)).style(style)
        })
        .collect();

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(synth_cyan))
        .title(" 󰲹 Library ");

    let list = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().bg(Color::DarkGray).fg(synth_green))
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    state.select(Some(app.selected_track_index));
    f.render_stateful_widget(list, left_layout[1], &mut state);

    // ── Right Column ─────────────────────────────────────────────────────────
    let right_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Visualizer
            Constraint::Length(8), // Now Playing / Controls
        ])
        .split(main_layout[1]);

    // Visualizer
    let vis_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(synth_cyan))
        .title(" ░▒▓ SPECTRUM ▓▒░ ");

    let visualizer = SpectrumVisualizer {
        spectrum: &app.spectrum,
        peak_spectrum: &app.peak_spectrum,
        mode: app.vis_mode,
        block: vis_block,
    };
    f.render_widget(visualizer, right_layout[0]);

    // Footer / Controls
    let footer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(synth_green));
    f.render_widget(footer_block, right_layout[1]);

    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .margin(1)
        .split(right_layout[1]);

    let paused = app
        .audio_player
        .as_ref()
        .map(|p| p.is_paused())
        .unwrap_or(false);
    let play_icon = if paused { "⏸" } else { "󰐊" };

    let track_info = format!(
        "\n  {} {}\n  󰠃 {}",
        play_icon, app.current_track_name, app.current_artist
    );
    f.render_widget(
        Paragraph::new(track_info)
            .style(Style::default().fg(synth_cyan).add_modifier(Modifier::BOLD)),
        footer_chunks[0],
    );

    let progress_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(footer_chunks[1]);

    let ratio = if app.total_duration.as_secs() > 0 {
        (app.playback_pos.as_secs_f64() / app.total_duration.as_secs_f64()).min(1.0)
    } else {
        0.0
    };

    let time_label = format!(
        "{:02}:{:02} / {:02}:{:02}",
        app.playback_pos.as_secs() / 60,
        app.playback_pos.as_secs() % 60,
        app.total_duration.as_secs() / 60,
        app.total_duration.as_secs() % 60,
    );

    f.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(synth_pink).bg(Color::DarkGray))
            .ratio(ratio)
            .label(time_label),
        progress_layout[0],
    );

    let vol_pct = (app.volume * 100.0).round() as u32;
    let controls = format!(
        "Volume: {}% | Controls: [Space] Play/Pause [j/k] Nav [Enter] Play [/] Search [d] Download [v] Vis",
        vol_pct
    );
    f.render_widget(
        Paragraph::new(controls).style(Style::default().fg(Color::Gray)),
        progress_layout[1],
    );

    // Toast Notification
    let notif = app
        .notification
        .try_lock()
        .map(|s| s.clone())
        .unwrap_or_default();
    if !notif.is_empty() {
        let toast_area = ratatui::layout::Rect {
            x: f.area().width.saturating_sub(42),
            y: f.area().height.saturating_sub(4),
            width: 40,
            height: 3,
        };
        let toast_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(synth_cyan).bg(Color::Rgb(15, 15, 20)))
            .style(Style::default().bg(Color::Rgb(15, 15, 20)));

        f.render_widget(Clear, toast_area);
        f.render_widget(
            Paragraph::new(format!(" ℹ {}", notif))
                .block(toast_block)
                .style(Style::default().fg(Color::White)),
            toast_area,
        );
    }

    // ── Overlays ──────────────────────────────────────────────────────────────
    if app.input_mode == InputMode::Editing {
        let area = centered_rect(60, 20, f.area());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(synth_pink))
            .title(" 🔍 Search Library ");

        f.render_widget(Clear, area);
        f.render_widget(
            Paragraph::new(app.library_search.clone())
                .block(block)
                .style(Style::default().fg(Color::White)),
            area,
        );
    } else if app.input_mode == InputMode::Downloading {
        let area = centered_rect(80, 20, f.area());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(synth_green))
            .title(" ⬇ Download Music/Video URL ");

        f.render_widget(Clear, area);
        f.render_widget(
            Paragraph::new(app.download_url.clone())
                .block(block)
                .style(Style::default().fg(Color::White)),
            area,
        );
    }

    // Download Progress Modal
    let dl_state = app.download_state.lock().unwrap();
    if dl_state.is_downloading {
        let area = centered_rect(60, 20, f.area());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(synth_green))
            .title(" ⬇ DOWNLOADING ");

        f.render_widget(Clear, area);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let dl_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Length(2)])
            .margin(1)
            .split(inner);

        f.render_widget(
            Paragraph::new(dl_state.message.clone()).style(Style::default().fg(synth_cyan)),
            dl_layout[0],
        );

        f.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(synth_pink).bg(Color::DarkGray))
                .ratio((dl_state.progress / 100.0).clamp(0.0, 1.0))
                .label(format!("{:.1}%", dl_state.progress)),
            dl_layout[1],
        );
    }
}

fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    r: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
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

use crate::app::VisualizerMode;

struct SpectrumVisualizer<'a> {
    spectrum: &'a [f32],
    peak_spectrum: &'a [f32],
    mode: VisualizerMode,
    block: Block<'a>,
}

impl<'a> ratatui::widgets::Widget for SpectrumVisualizer<'a> {
    fn render(self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
        let inner = self.block.inner(area);
        ratatui::widgets::Widget::render(self.block, area, buf);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let width = inner.width;
        let height = inner.height;
        let bars = self.spectrum.len() as u16;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f32();

        for x_pos in 0..width {
            let bin_idx =
                (x_pos as f32 / width as f32 * bars as f32).clamp(0.0, bars as f32 - 1.0) as usize;
            let val = self.spectrum[bin_idx];
            let peak = self.peak_spectrum[bin_idx];

            let bar_height = (val * height as f32).round() as u16;
            let peak_height = (peak * height as f32).round() as u16;

            let x = inner.x + x_pos;

            for h in 0..height {
                let y = inner.y + height - 1 - h;

                let mut symbol = " ";
                let mut fg = Color::Rgb(0, 255, 255);

                let ratio_y = h as f32 / height as f32;
                let ratio_x = x_pos as f32 / width as f32;

                if self.mode == VisualizerMode::RetroCrt {
                    let phase = now * 3.0 - (ratio_y * 5.0) + (ratio_x * 4.0);
                    let r = ((phase.sin() * 0.5 + 0.5) * 255.0) as u8;
                    let g = (((phase + 2.0).sin() * 0.5 + 0.5) * 150.0) as u8;
                    let b = (((phase + 4.0).sin() * 0.5 + 0.5) * 255.0) as u8;
                    fg = Color::Rgb(r, g, b);

                    if h < bar_height {
                        symbol = "█";
                    } else if h == peak_height {
                        symbol = "▔";
                        fg = Color::Rgb(255, 255, 255);
                    } else {
                        if h % 2 == 0 {
                            symbol = "░";
                            fg = Color::Rgb(r / 4, g / 4, b / 4);
                        } else {
                            symbol = "▒";
                            fg = Color::Rgb(r / 6, g / 6, b / 6);
                        }
                    }
                } else if self.mode == VisualizerMode::NeonWaves {
                    let phase = now * 2.0 + (ratio_x * 8.0);
                    let r = ((phase.sin() * 0.5 + 0.5) * 255.0) as u8;
                    let g = (((phase + 2.0).sin() * 0.5 + 0.5) * 255.0) as u8;
                    let b = (((phase + 4.0).sin() * 0.5 + 0.5) * 255.0) as u8;
                    fg = Color::Rgb(r, g, b);

                    if h < bar_height {
                        let block_chars = [" ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
                        symbol = "█";
                        if h == bar_height - 1 {
                            let rem = (val * height as f32) - (h as f32);
                            let idx = (rem * 8.0).floor() as usize;
                            symbol = block_chars[idx.clamp(0, 7)];
                        }
                    } else {
                        symbol = " ";
                    }
                } else if self.mode == VisualizerMode::CyberpunkPeak {
                    let phase = now * 5.0 - (ratio_y * 10.0);
                    let r = 0;
                    let g = ((phase.sin() * 0.5 + 0.5) * 255.0) as u8;
                    let b = ((phase.cos() * 0.5 + 0.5) * 100.0) as u8;

                    if h < bar_height && h % 2 == 0 {
                        symbol = "│";
                        fg = Color::Rgb(0, g / 2, b / 2);
                    } else if h == peak_height {
                        symbol = "■";
                        fg = Color::Rgb(r, 255, 255);
                    } else {
                        symbol = " ";
                    }
                }

                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(symbol).set_fg(fg);
                }
            }
        }
    }
}
