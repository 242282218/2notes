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
    pub backup_dir: PathBuf,
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
    let test_root = std::env::var_os("TWONOTES_TEST_ROOT")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    prepare_paths_with_test_root(&app_config_dir, &app_data_dir, test_root.as_deref())
}

fn prepare_paths_with_test_root(
    app_config_dir: &Path,
    app_data_dir: &Path,
    test_root: Option<&Path>,
) -> crate::error::AppResult<AppPaths> {
    match test_root {
        Some(root) => prepare_paths(&root.join("config"), root),
        None => prepare_paths(app_config_dir, app_data_dir),
    }
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
    let backup_dir = app_data_dir.join("backups");
    fs::create_dir_all(&data_dir)?;
    fs::create_dir_all(&log_dir)?;
    fs::create_dir_all(&backup_dir)?;
    let database_path = data_dir.join("2notes.sqlite");
    let rollback_path = database_path.with_extension("restore.bak");
    if rollback_path.exists() {
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = database_path.as_os_str().to_os_string();
            sidecar.push(suffix);
            let sidecar = PathBuf::from(sidecar);
            if sidecar.exists() {
                fs::remove_file(sidecar)?;
            }
        }
        if database_path.exists() {
            fs::remove_file(&database_path)?;
        }
        fs::rename(&rollback_path, &database_path)?;
    }

    Ok(AppPaths {
        database_path,
        data_dir,
        log_dir,
        backup_dir,
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

    #[test]
    fn restores_interrupted_database_swap() {
        let config = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let paths = prepare_paths(config.path(), data.path()).unwrap();
        let rollback_path = paths.database_path.with_extension("restore.bak");
        fs::write(&rollback_path, "original database").unwrap();

        let restored = prepare_paths(config.path(), data.path()).unwrap();

        assert_eq!(
            fs::read_to_string(&restored.database_path).unwrap(),
            "original database"
        );
        assert!(!rollback_path.exists());
    }

    #[test]
    fn rolls_back_restore_when_live_and_rollback_databases_both_exist() {
        let config = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let paths = prepare_paths(config.path(), data.path()).unwrap();
        let rollback_path = paths.database_path.with_extension("restore.bak");
        let wal_path = PathBuf::from(format!("{}-wal", paths.database_path.display()));
        let shm_path = PathBuf::from(format!("{}-shm", paths.database_path.display()));
        fs::write(&paths.database_path, "partially restored database").unwrap();
        fs::write(&rollback_path, "original database").unwrap();
        fs::write(&wal_path, "stale wal").unwrap();
        fs::write(&shm_path, "stale shm").unwrap();

        let restored = prepare_paths(config.path(), data.path()).unwrap();

        assert_eq!(
            fs::read_to_string(&restored.database_path).unwrap(),
            "original database"
        );
        assert!(!rollback_path.exists());
        assert!(!wal_path.exists());
        assert!(!shm_path.exists());
    }

    #[test]
    fn explicit_test_root_overrides_windows_app_directories() {
        let config = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();

        let paths =
            prepare_paths_with_test_root(config.path(), data.path(), Some(root.path())).unwrap();

        assert_eq!(paths.data_dir, root.path().join("data"));
        assert_eq!(paths.log_dir, root.path().join("logs"));
        assert_eq!(paths.backup_dir, root.path().join("backups"));
        assert_eq!(
            paths.database_path,
            root.path().join("data").join("2notes.sqlite")
        );
        assert!(root.path().join("config").join("bootstrap.json").exists());
    }
}
