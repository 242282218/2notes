use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};

use rusqlite::Connection;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    files::paths::AppPaths,
};

#[derive(Debug, Clone)]
pub struct ShortcutStatus {
    pub shortcut: String,
    pub registered: bool,
    pub error: Option<String>,
}

pub struct AppState {
    conn: Mutex<Connection>,
    pub paths: AppPaths,
    shortcut: Mutex<ShortcutStatus>,
    quit_request: Mutex<Option<QuitRequest>>,
}

#[derive(Debug)]
struct QuitRequest {
    id: String,
    pending_windows: HashSet<String>,
}

impl AppState {
    pub fn new(conn: Connection, paths: AppPaths) -> Self {
        Self {
            conn: Mutex::new(conn),
            paths,
            shortcut: Mutex::new(ShortcutStatus {
                shortcut: "Ctrl+Alt+Space".to_string(),
                registered: false,
                error: None,
            }),
            quit_request: Mutex::new(None),
        }
    }

    pub fn conn(&self) -> AppResult<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| AppError::system("DB_LOCK_POISONED", "数据库连接状态异常"))
    }

    pub fn set_shortcut_status(&self, registered: bool, error: Option<String>) -> AppResult<()> {
        let mut status = self
            .shortcut
            .lock()
            .map_err(|_| AppError::system("SHORTCUT_STATE_FAILED", "快捷键状态不可用"))?;
        status.registered = registered;
        status.error = error;
        Ok(())
    }

    pub fn shortcut_status(&self) -> AppResult<ShortcutStatus> {
        self.shortcut
            .lock()
            .map(|status| status.clone())
            .map_err(|_| AppError::system("SHORTCUT_STATE_FAILED", "快捷键状态不可用"))
    }

    pub fn data_dir(&self) -> PathBuf {
        self.paths.data_dir.clone()
    }

    pub fn log_dir(&self) -> PathBuf {
        self.paths.log_dir.clone()
    }

    pub fn start_quit_request(&self, windows: Vec<String>) -> AppResult<String> {
        let id = Uuid::new_v4().to_string();
        let pending_windows = windows.into_iter().collect();
        let mut request = self
            .quit_request
            .lock()
            .map_err(|_| AppError::system("QUIT_STATE_FAILED", "退出状态不可用"))?;
        *request = Some(QuitRequest {
            id: id.clone(),
            pending_windows,
        });
        Ok(id)
    }

    pub fn mark_quit_ready(&self, request_id: &str, window_label: &str) -> AppResult<bool> {
        let mut request = self
            .quit_request
            .lock()
            .map_err(|_| AppError::system("QUIT_STATE_FAILED", "退出状态不可用"))?;
        let Some(current) = request.as_mut() else {
            return Ok(false);
        };
        if current.id != request_id {
            return Ok(false);
        }
        current.pending_windows.remove(window_label);
        let complete = current.pending_windows.is_empty();
        if complete {
            *request = None;
        }
        Ok(complete)
    }
}
