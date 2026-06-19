use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::App;

pub fn render_library_tab(
    f: &mut Frame, app: &mut App, area: Rect,
    content_block: &Block, primary_color: Color,
) {
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

pub fn render_playlist_tab(
    f: &mut Frame, app: &mut App, area: Rect,
    content_block: &Block,
) {
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
