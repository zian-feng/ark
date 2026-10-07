use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use super::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(frame.area());

    let search_label = if app.searching {
        "Search (typing)"
    } else {
        "Search (/)"
    };
    let search = Paragraph::new(app.query.as_str())
        .block(Block::default().borders(Borders::ALL).title(search_label));
    frame.render_widget(search, areas[0]);

    let sessions = app.visible_sessions();
    let items = sessions
        .iter()
        .map(|session| {
            let marker = if session.starred { "★" } else { " " };
            let alias = if session.id == session.session_id {
                "—"
            } else {
                &session.id
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{marker} "), Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("{alias:<25}"),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("  {:<8}  ", session.provider)),
                Span::raw(session.description.as_str()),
            ]))
        })
        .collect::<Vec<_>>();

    let title = format!("Ark sessions ({})", sessions.len());
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("› ");
    let mut state = ListState::default();
    if !sessions.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, areas[1], &mut state);

    let footer_text = if app.status.is_empty() {
        "/ search  ↑↓ move  Enter resume  s star  u unstar  q quit"
    } else {
        app.status.as_str()
    };
    let footer = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::TOP))
        .wrap(Wrap { trim: true });
    frame.render_widget(footer, areas[2]);
}
