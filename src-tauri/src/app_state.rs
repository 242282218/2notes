use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Condvar, Mutex, MutexGuard},
};

use rusqlite::{Connection, Transaction};
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
    /// Write connection protected by mutex. SQLite serializes writes regardless of WAL mode.
    write_conn: Mutex<Connection>,
    /// Read-only connection. Although SQLite WAL mode allows concurrent readers,
    /// rusqlite::Connection uses RefCell internally so it is not Sync. We use Mutex
    /// to protect the handle itself; the underlying SQLite connection still benefits
    /// from WAL-mode concurrency for actual query execution.
    read_conn: Mutex<Connection>,
    pub paths: AppPaths,
    shortcut: Mutex<ShortcutStatus>,
    quit_request: Mutex<Option<QuitRequest>>,
    restore_request: Mutex<Option<RestoreRequest>>,
    restore_ready: Condvar,
}

#[derive(Debug)]
struct QuitRequest {
    id: String,
    pending_windows: HashSet<String>,
}

#[derive(Debug)]
struct RestoreRequest {
    id: String,
    ready: bool,
}

impl AppState {
    pub fn new(write_conn: Connection, read_conn: Connection, paths: AppPaths) -> Self {
        Self {
            write_conn: Mutex::new(write_conn),
            read_conn: Mutex::new(read_conn),
            paths,
            shortcut: Mutex::new(ShortcutStatus {
                shortcut: "Ctrl+Alt+Space".to_string(),
                registered: false,
                error: None,
            }),
            quit_request: Mutex::new(None),
            restore_request: Mutex::new(None),
            restore_ready: Condvar::new(),
        }
    }

    /// Acquire the write connection. Held briefly for transactional writes.
    pub fn write_conn(&self) -> AppResult<MutexGuard<'_, Connection>> {
        self.write_conn
            .lock()
            .map_err(|_| AppError::system("DB_LOCK_POISONED", "数据库写连接状态异常"))
    }

    /// Acquire the read-only connection. In WAL mode concurrent readers execute
    /// without blocking each other at the SQLite level, but the Mutex protects
    /// the rusqlite::Connection handle which uses RefCell internally.
    pub fn read_conn(&self) -> AppResult<MutexGuard<'_, Connection>> {
        self.read_conn
            .lock()
            .map_err(|_| AppError::system("DB_READ_LOCK_POISONED", "数据库读连接状态异常"))
    }

    pub fn database_pair(
        &self,
    ) -> AppResult<(MutexGuard<'_, Connection>, MutexGuard<'_, Connection>)> {
        let write_conn = self.write_conn()?;
        let read_conn = self.read_conn()?;
        Ok((write_conn, read_conn))
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

    pub fn backup_dir(&self) -> PathBuf {
        self.paths.backup_dir.clone()
    }

    /// Run a closure inside a write transaction.
    /// The transaction is committed if the closure returns Ok, rolled back on Err.
    /// This reduces boilerplate in command handlers while keeping tx scope explicit.
    pub fn with_write_tx<F, T>(&self, f: F) -> crate::error::AppResult<T>
    where
        F: FnOnce(&Transaction<'_>) -> crate::error::AppResult<T>,
    {
        let mut conn = self.write_conn()?;
        let tx = conn.transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
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

    pub fn start_restore_request(&self) -> AppResult<String> {
        let id = Uuid::new_v4().to_string();
        let mut request = self
            .restore_request
            .lock()
            .map_err(|_| AppError::system("RESTORE_STATE_FAILED", "恢复状态不可用"))?;
        *request = Some(RestoreRequest {
            id: id.clone(),
            ready: false,
        });
        Ok(id)
    }

    pub fn mark_restore_ready(&self, request_id: &str) -> AppResult<bool> {
        let mut request = self
            .restore_request
            .lock()
            .map_err(|_| AppError::system("RESTORE_STATE_FAILED", "恢复状态不可用"))?;
        let Some(current) = request.as_mut() else {
            return Ok(false);
        };
        if current.id != request_id {
            return Ok(false);
        }
        current.ready = true;
        self.restore_ready.notify_all();
        Ok(true)
    }

    pub fn wait_restore_ready(
        &self,
        request_id: &str,
        timeout: std::time::Duration,
    ) -> AppResult<bool> {
        let request = self
            .restore_request
            .lock()
            .map_err(|_| AppError::system("RESTORE_STATE_FAILED", "恢复状态不可用"))?;
        let (mut request, wait) = self
            .restore_ready
            .wait_timeout_while(request, timeout, |current| {
                current
                    .as_ref()
                    .is_some_and(|value| value.id == request_id && !value.ready)
            })
            .map_err(|_| AppError::system("RESTORE_STATE_FAILED", "恢复状态不可用"))?;
        let ready = !wait.timed_out()
            && request
                .as_ref()
                .is_some_and(|value| value.id == request_id && value.ready);
        if request.as_ref().is_some_and(|value| value.id == request_id) {
            *request = None;
        }
        Ok(ready)
    }

    pub fn cancel_restore_request(&self, request_id: &str) -> AppResult<bool> {
        let mut request = self
            .restore_request
            .lock()
            .map_err(|_| AppError::system("RESTORE_STATE_FAILED", "恢复状态不可用"))?;
        let should_cancel = request
            .as_ref()
            .is_some_and(|current| current.id == request_id);
        if should_cancel {
            *request = None;
            self.restore_ready.notify_all();
        }
        Ok(should_cancel)
    }

    pub fn cancel_quit_request(&self, request_id: &str) -> AppResult<bool> {
        let mut request = self
            .quit_request
            .lock()
            .map_err(|_| AppError::system("QUIT_STATE_FAILED", "退出状态不可用"))?;
        let should_cancel = request
            .as_ref()
            .is_some_and(|current| current.id == request_id);
        if should_cancel {
            *request = None;
        }
        Ok(should_cancel)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::open_in_memory;

    #[test]
    fn only_the_current_quit_request_can_be_cancelled() {
        let state = test_state();
        let first = state.start_quit_request(vec!["main".to_string()]).unwrap();
        let second = state
            .start_quit_request(vec!["main".to_string(), "quick-capture".to_string()])
            .unwrap();

        assert!(!state.cancel_quit_request(&first).unwrap());
        assert!(state.cancel_quit_request(&second).unwrap());
        assert!(!state.mark_quit_ready(&second, "main").unwrap());
    }

    #[test]
    fn restore_request_waits_for_the_matching_acknowledgement() {
        let state = test_state();
        let request_id = state.start_restore_request().unwrap();

        assert!(state.mark_restore_ready(&request_id).unwrap());
        assert!(state
            .wait_restore_ready(&request_id, std::time::Duration::from_millis(1))
            .unwrap());

        let timed_out = state.start_restore_request().unwrap();
        assert!(!state
            .wait_restore_ready(&timed_out, std::time::Duration::from_millis(1))
            .unwrap());
    }
    fn test_state() -> AppState {
        let (write_conn, read_conn) = open_in_memory().unwrap();
        let root = tempfile::tempdir().unwrap().keep();
        AppState::new(
            write_conn,
            read_conn,
            AppPaths {
                data_dir: root.join("data"),
                log_dir: root.join("logs"),
                backup_dir: root.join("backups"),
                database_path: root.join("data").join("2notes.sqlite"),
                bootstrap_path: root.join("bootstrap.json"),
            },
        )
    }
}
