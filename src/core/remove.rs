use crate::storage::Database;
use anyhow::Result;

pub fn remove_session(session_id: &str) -> Result<()> {
    let database = Database::open()?;
    database.remove_session(session_id)
}
