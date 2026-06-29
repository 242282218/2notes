use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub database_path: PathBuf,
    #[cfg(test)]
    pub bootstrap_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct BootstrapConfig {
    data_dir: PathBuf,
}

pub fn prepare_app_paths(app: &AppHandle) -> crate::error::AppResult<AppPaths> {
    let app_config_dir = app.path().app_config_dir()?;
    let app_data_dir = app.path().app_data_dir()?;
    prepare_paths(&app_config_dir, &app_data_dir)
}

pub fn prepare_paths(
    app_config_dir: &Path,
    app_data_dir: &Path,
) -> crate::error::AppResult<AppPaths> {
    fs::create_dir_all(app_config_dir)?;
    fs::create_dir_all(app_data_dir)?;

    let bootstrap_path = app_config_dir.join("bootstrap.json");
    let default_data_dir = app_data_dir.join("data");
    let data_dir = if bootstrap_path.exists() {
        let raw = fs::read_to_string(&bootstrap_path)?;
        serde_json::from_str::<BootstrapConfig>(&raw)?.data_dir
    } else {
        let config = BootstrapConfig {
            data_dir: default_data_dir,
        };
        fs::write(&bootstrap_path, serde_json::to_string_pretty(&config)?)?;
        config.data_dir
    };

    let log_dir = app_data_dir.join("logs");
    fs::create_dir_all(&data_dir)?;
    fs::create_dir_all(&log_dir)?;

    Ok(AppPaths {
        database_path: data_dir.join("2notes.sqlite"),
        data_dir,
        log_dir,
        #[cfg(test)]
        bootstrap_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_default_bootstrap() {
        let config = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();

        let paths = prepare_paths(config.path(), data.path()).unwrap();

        assert!(paths.bootstrap_path.exists());
        assert!(paths.data_dir.exists());
        assert!(paths.database_path.ends_with("2notes.sqlite"));
    }

    #[test]
    fn uses_existing_bootstrap_data_dir() {
        let config = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let custom = data.path().join("custom-data");
        let bootstrap = BootstrapConfig {
            data_dir: custom.clone(),
        };
        fs::write(
            config.path().join("bootstrap.json"),
            serde_json::to_string(&bootstrap).unwrap(),
        )
        .unwrap();

        let paths = prepare_paths(config.path(), data.path()).unwrap();

        assert_eq!(paths.data_dir, custom);
        assert!(paths.data_dir.exists());
    }
}
