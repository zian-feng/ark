use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::App;

pub enum Action {
    None,
    Quit,
    Resume(String),
    Star(String),
    Unstar(String),
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }

    if app.searching {
        return handle_search_key(app, key);
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.move_selection(-1);
            Action::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.move_selection(1);
            Action::None
        }
        KeyCode::Enter => app.selected_key().map_or(Action::None, Action::Resume),
        KeyCode::Char('s') => app.selected_key().map_or(Action::None, Action::Star),
        KeyCode::Char('u') => app.selected_key().map_or(Action::None, Action::Unstar),
        KeyCode::Char('/') => {
            app.searching = true;
            Action::None
        }
        KeyCode::Esc | KeyCode::Char('q') => Action::Quit,
        _ => Action::None,
    }
}

fn handle_search_key(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            app.searching = false;
            app.clear_query();
        }
        KeyCode::Enter => app.searching = false,
        KeyCode::Backspace => app.remove_last_query_character(),
        KeyCode::Char(character)
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
        {
            app.append_query(character);
        }
        _ => {}
    }

    Action::None
}
