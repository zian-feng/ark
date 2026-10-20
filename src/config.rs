use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub auto_mode: bool,
    #[serde(default)]
    pub auto_confirm: bool,
}

impl Config {
    pub fn load() -> Result<Self> {
        Self::load_at(Self::path_for_current_user()?)
    }

    pub fn save(&self) -> Result<()> {
        self.save_at(Self::path_for_current_user()?)
    }

    pub fn path_for_current_user() -> Result<PathBuf> {
        let home = dirs::home_dir().context("could not determine the home directory")?;
        Ok(home.join(".ark").join("config.toml"))
    }

    fn load_at(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        match fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents)
                .with_context(|| format!("could not parse Ark config: {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => {
                Err(error).with_context(|| format!("could not read Ark config: {}", path.display()))
            }
        }
    }

    fn save_at(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "could not create Ark config directory: {}",
                    parent.display()
                )
            })?;
        }

        fs::write(path, toml::to_string_pretty(self)?)
            .with_context(|| format!("could not write Ark config: {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn missing_config_uses_defaults_and_saved_config_round_trips() -> anyhow::Result<()> {
        let temporary_directory = tempfile::tempdir()?;
        let path = temporary_directory.path().join("config.toml");

        assert!(!Config::load_at(&path)?.auto_mode);

        let config = Config {
            auto_mode: true,
            auto_confirm: true,
        };
        config.save_at(&path)?;
        let loaded = Config::load_at(&path)?;

        assert!(loaded.auto_mode);
        assert!(loaded.auto_confirm);
        Ok(())
    }
}
