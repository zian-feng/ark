use anyhow::Result;

use crate::storage::Database;

pub fn set_starred(key: &str, starred: bool) -> Result<()> {
    let database = Database::open()?;
    database.set_starred(key, starred)
}
