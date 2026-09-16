use anyhow::Result;

use crate::storage::{Database, SessionSummary};

pub fn list_sessions() -> Result<Vec<SessionSummary>> {
    Database::open()?.list_sessions()
}
