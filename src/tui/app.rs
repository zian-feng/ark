use anyhow::Result;

use crate::{core, storage::SessionSummary};

pub struct App {
    sessions: Vec<SessionSummary>,
    pub query: String,
    pub searching: bool,
    pub selected: usize,
    pub status: String,
}

impl App {
    pub fn load() -> Result<Self> {
        Ok(Self {
            sessions: core::list::list_sessions()?,
            query: String::new(),
            searching: false,
            selected: 0,
            status: String::new(),
        })
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.sessions = core::list::list_sessions()?;
        self.clamp_selection();
        Ok(())
    }

    pub fn visible_sessions(&self) -> Vec<&SessionSummary> {
        self.sessions
            .iter()
            .filter(|session| fuzzy_matches(&self.query, &searchable_text(session)))
            .collect()
    }

    pub fn selected_key(&self) -> Option<String> {
        self.visible_sessions()
            .get(self.selected)
            .map(|session| session.id.clone())
    }

    pub fn move_selection(&mut self, amount: isize) {
        let length = self.visible_sessions().len();
        if length == 0 {
            self.selected = 0;
            return;
        }

        self.selected = (self.selected as isize + amount).clamp(0, length as isize - 1) as usize;
    }

    pub fn append_query(&mut self, character: char) {
        self.query.push(character);
        self.selected = 0;
    }

    pub fn remove_last_query_character(&mut self) {
        self.query.pop();
        self.selected = 0;
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.selected = 0;
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    fn clamp_selection(&mut self) {
        let length = self.visible_sessions().len();
        self.selected = self.selected.min(length.saturating_sub(1));
    }
}

fn searchable_text(session: &SessionSummary) -> String {
    format!(
        "{} {} {} {}",
        session.id, session.session_id, session.provider, session.description
    )
}

fn fuzzy_matches(query: &str, candidate: &str) -> bool {
    let candidate = candidate.to_lowercase();
    let mut candidate = candidate.chars();

    query
        .to_lowercase()
        .chars()
        .all(|query_character| candidate.any(|character| character == query_character))
}

#[cfg(test)]
mod tests {
    use crate::storage::SessionSummary;

    use super::{App, fuzzy_matches};

    #[test]
    fn fuzzy_match_accepts_characters_in_order() {
        assert!(fuzzy_matches("arf", "auth refactor"));
        assert!(!fuzzy_matches("fra", "auth refactor"));
    }

    #[test]
    fn selected_key_uses_the_filtered_session() {
        let app = App {
            sessions: vec![
                SessionSummary {
                    starred: false,
                    id: "auth-refactor".to_owned(),
                    session_id: "one".to_owned(),
                    provider: "codex".to_owned(),
                    description: String::new(),
                },
                SessionSummary {
                    starred: false,
                    id: "deploy".to_owned(),
                    session_id: "two".to_owned(),
                    provider: "claude".to_owned(),
                    description: String::new(),
                },
            ],
            query: "dpl".to_owned(),
            searching: false,
            selected: 0,
            status: String::new(),
        };

        assert_eq!(app.selected_key().as_deref(), Some("deploy"));
    }
}
