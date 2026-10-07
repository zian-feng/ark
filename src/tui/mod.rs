mod app;
mod input;
mod render;

use std::{io, time::Duration};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::core;

use self::{app::App, input::Action};

type ArkTerminal = Terminal<CrosstermBackend<io::Stdout>>;

pub fn run() -> Result<Option<String>> {
    let mut terminal = setup_terminal()?;
    let result = run_event_loop(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> Result<ArkTerminal> {
    enable_raw_mode().context("could not enable terminal raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("could not enter Ark's interactive screen")?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    terminal.hide_cursor()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut ArkTerminal) -> Result<()> {
    disable_raw_mode().context("could not restore terminal mode")?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .context("could not restore the terminal screen")?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_event_loop(terminal: &mut ArkTerminal) -> Result<Option<String>> {
    let mut app = App::load()?;

    loop {
        terminal.draw(|frame| render::render(frame, &app))?;

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match input::handle_key(&mut app, key) {
            Action::None => {}
            Action::Quit => return Ok(None),
            Action::Resume(key) => return Ok(Some(key)),
            Action::Star(key) => update_star(&mut app, &key, true),
            Action::Unstar(key) => update_star(&mut app, &key, false),
        }
    }
}

fn update_star(app: &mut App, key: &str, starred: bool) {
    match core::star::set_starred(key, starred).and_then(|_| app.refresh()) {
        Ok(()) => app.set_status(if starred { "Starred." } else { "Unstarred." }),
        Err(error) => app.set_status(format!("Error: {error:#}")),
    }
}
