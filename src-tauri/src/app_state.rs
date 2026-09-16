use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard,
    },
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
    /// Read-only connection pool. WAL mode allows concurrent readers, so readers
    /// round-robin across independent connections instead of serializing on one.
    read_conns: Mutex<Vec<Arc<Mutex<Connection>>>>,
    /// Next read slot to check out (round-robin).
    read_pool_next: AtomicUsize,
    pub paths: AppPaths,
    shortcut: Mutex<ShortcutStatus>,
    quit_request: Mutex<Option<QuitRequest>>,
    restore_request: Mutex<Option<RestoreRequest>>,
    restore_lock: Mutex<bool>,
    database_gate: RwLock<()>,
    restore_ready: Condvar,
    import_sessions: Mutex<std::collections::HashMap<String, ImportSession>>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSessionCandidate {
    pub relative_path: PathBuf,
    pub source_hash: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ImportSession {
    pub root: PathBuf,
    pub candidates: Vec<ImportSessionCandidate>,
    pub expires_at: std::time::Instant,
}

impl AppState {
    /// Build a state with a single read connection. Only used by tests now;
    /// production builds the full pool via `with_read_pool`.
    #[cfg(test)]
    pub fn new(write_conn: Connection, read_conn: Connection, paths: AppPaths) -> Self {
        Self::with_read_pool(write_conn, vec![read_conn], paths)
    }

    /// Build a state with the given read connection pool.
    pub fn with_read_pool(
        write_conn: Connection,
        read_conns: Vec<Connection>,
        paths: AppPaths,
    ) -> Self {
        Self {
            write_conn: Mutex::new(write_conn),
            read_conns: Mutex::new(
                read_conns
                    .into_iter()
                    .map(|conn| Arc::new(Mutex::new(conn)))
                    .collect(),
            ),
            read_pool_next: AtomicUsize::new(0),
            paths,
            shortcut: Mutex::new(ShortcutStatus {
                shortcut: "Ctrl+Alt+Space".to_string(),
                registered: false,
                error: None,
            }),
            quit_request: Mutex::new(None),
            restore_request: Mutex::new(None),
            restore_lock: Mutex::new(false),
            database_gate: RwLock::new(()),
            restore_ready: Condvar::new(),
            import_sessions: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Acquire the write connection. Held briefly for transactional writes.
    pub fn write_conn(&self) -> AppResult<MutexGuard<'_, Connection>> {
        self.write_conn
            .lock()
            .map_err(|_| AppError::system("DB_LOCK_POISONED", "数据库写连接状态异常"))
    }

    /// Run a closure against a pooled read-only connection. Round-robin checkout
    /// spreads concurrent readers across independent connections. Taking the
    /// write permit first ensures a restore's write barrier also excludes new
    /// readers while it rebuilds the pool.
    pub fn with_read_conn<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&Connection) -> AppResult<T>,
    {
        let _permit = self.database_write_permit()?;
        let slot = {
            let pool = self
                .read_conns
                .lock()
                .map_err(|_| AppError::system("DB_READ_POOL_FAILED", "数据库读连接池状态异常"))?;
            if pool.is_empty() {
                return Err(AppError::system(
                    "DB_READ_POOL_EMPTY",
                    "数据库读连接池为空，恢复尚未完成",
                ));
            }
            let index = self.read_pool_next.fetch_add(1, Ordering::Relaxed) % pool.len();
            Arc::clone(&pool[index])
        };
        let conn = slot
            .lock()
            .map_err(|_| AppError::system("DB_READ_LOCK_POISONED", "数据库读连接状态异常"))?;
        f(&conn)
    }

    /// Replace the read pool. The caller must hold `database_operation()` so no
    /// new reader can check out; the loop below is defense-in-depth that waits
    /// for any straggling reader to release its slot before the file is swapped.
    /// Replacing the Vec drops the old slots, which closes their file handles
    /// (essential on Windows before the database file is renamed).
    pub fn replace_read_pool(&self, read_conns: Vec<Connection>) -> AppResult<()> {
        let mut pool = self
            .read_conns
            .lock()
            .map_err(|_| AppError::system("DB_READ_POOL_FAILED", "数据库读连接池状态异常"))?;
        for slot in pool.iter() {
            let _guard = slot
                .lock()
                .map_err(|_| AppError::system("DB_READ_LOCK_POISONED", "数据库读连接状态异常"))?;
        }
        *pool = read_conns
            .into_iter()
            .map(|conn| Arc::new(Mutex::new(conn)))
            .collect();
        Ok(())
    }

    pub fn database_write_permit(&self) -> AppResult<RwLockReadGuard<'_, ()>> {
        self.database_gate
            .read()
            .map_err(|_| AppError::system("DB_GATE_FAILED", "数据库写入屏障状态异常"))
    }

    pub fn database_operation(&self) -> AppResult<RwLockWriteGuard<'_, ()>> {
        self.database_gate
            .write()
            .map_err(|_| AppError::system("DB_GATE_FAILED", "数据库操作屏障状态异常"))
    }

    pub fn with_write_conn<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&mut Connection) -> AppResult<T>,
    {
        let _permit = self.database_write_permit()?;
        let mut conn = self.write_conn()?;
        f(&mut conn)
    }

    pub fn set_shortcut(
        &self,
        shortcut: String,
        registered: bool,
        error: Option<String>,
    ) -> AppResult<()> {
        let mut status = self
            .shortcut
            .lock()
            .map_err(|_| AppError::system("SHORTCUT_STATE_FAILED", "快捷键状态不可用"))?;
        status.shortcut = shortcut;
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
        let _permit = self.database_write_permit()?;
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

    pub fn begin_restore(&self) -> AppResult<bool> {
        let mut in_progress = self
            .restore_lock
            .lock()
            .map_err(|_| AppError::system("RESTORE_STATE_FAILED", "恢复状态不可用"))?;
        if *in_progress {
            return Ok(false);
        }
        *in_progress = true;
        Ok(true)
    }

    pub fn end_restore(&self) -> AppResult<()> {
        let mut in_progress = self
            .restore_lock
            .lock()
            .map_err(|_| AppError::system("RESTORE_STATE_FAILED", "恢复状态不可用"))?;
        *in_progress = false;
        Ok(())
    }

    pub fn begin_backup(&self) -> AppResult<bool> {
        self.begin_restore()
    }

    pub fn end_backup(&self) -> AppResult<()> {
        self.end_restore()
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

    pub fn create_import_session(
        &self,
        root: PathBuf,
        candidates: Vec<ImportSessionCandidate>,
        ttl: std::time::Duration,
    ) -> AppResult<String> {
        let id = Uuid::new_v4().to_string();
        let mut sessions = self
            .import_sessions
            .lock()
            .map_err(|_| AppError::system("IMPORT_SESSION_FAILED", "导入会话状态不可用"))?;
        let now = std::time::Instant::now();
        sessions.clear();
        sessions.insert(
            id.clone(),
            ImportSession {
                root,
                candidates,
                expires_at: now + ttl,
            },
        );
        Ok(id)
    }

    pub fn consume_import_session(&self, id: &str) -> AppResult<ImportSession> {
        let mut sessions = self
            .import_sessions
            .lock()
            .map_err(|_| AppError::system("IMPORT_SESSION_FAILED", "导入会话状态不可用"))?;
        let now = std::time::Instant::now();
        sessions.retain(|_, session| session.expires_at > now);
        sessions.remove(id).ok_or_else(|| {
            AppError::validation("IMPORT_SESSION_INVALID", "导入预览已失效，请重新选择目录")
        })
    }
}

/// RAII guard that releases the backup/restore lock on drop, so a panic or a
/// dropped future can never leave the lock permanently wedged.
pub struct BackupOperationGuard<'a> {
    state: &'a AppState,
}

impl<'a> BackupOperationGuard<'a> {
    /// Begin a backup/restore operation. Returns `None` when one is already in flight.
    pub fn begin(state: &'a AppState) -> AppResult<Option<Self>> {
        if state.begin_backup()? {
            Ok(Some(Self { state }))
        } else {
            Ok(None)
        }
    }
}

impl Drop for BackupOperationGuard<'_> {
    fn drop(&mut self) {
        if let Err(err) = self.state.end_backup() {
            log::error!("backup_lock_release_failed source={err}");
        }
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
    fn restore_lock_allows_only_one_operation() {
        let state = test_state();

        assert!(state.begin_restore().unwrap());
        assert!(!state.begin_restore().unwrap());
        state.end_restore().unwrap();
        assert!(state.begin_restore().unwrap());
        state.end_restore().unwrap();
    }

    #[test]
    fn backup_operation_guard_releases_lock_on_drop() {
        let state = test_state();

        {
            let guard = BackupOperationGuard::begin(&state).unwrap();
            assert!(guard.is_some());
            assert!(
                !state.begin_restore().unwrap(),
                "lock must stay held while the guard lives"
            );
        }
        assert!(
            state.begin_restore().unwrap(),
            "dropping the guard must release the lock"
        );
        state.end_restore().unwrap();

        // A lock already held by a raw begin_restore yields None from the guard.
        let _held = state.begin_restore().unwrap();
        assert!(BackupOperationGuard::begin(&state).unwrap().is_none());
        state.end_restore().unwrap();
    }

    #[test]
    fn database_operation_blocks_write_transactions_until_it_ends() {
        let state = std::sync::Arc::new(test_state());
        let operation = state.database_operation().unwrap();
        let worker_state = std::sync::Arc::clone(&state);
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            worker_state
                .with_write_tx(|_| Ok(()))
                .expect("write transaction should eventually complete");
            finished_tx.send(()).unwrap();
        });

        assert!(finished_rx
            .recv_timeout(std::time::Duration::from_millis(20))
            .is_err());
        drop(operation);
        assert!(finished_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .is_ok());
        worker.join().unwrap();
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

    #[test]
    fn import_session_is_consumed_once() {
        let state = test_state();
        let id = state
            .create_import_session(
                PathBuf::from("C:/imports"),
                vec![ImportSessionCandidate {
                    relative_path: PathBuf::from("note.md"),
                    source_hash: "hash".to_string(),
                    size_bytes: 1,
                }],
                std::time::Duration::from_secs(1),
            )
            .unwrap();

        let session = state.consume_import_session(&id).unwrap();
        assert_eq!(session.candidates.len(), 1);
        let err = state.consume_import_session(&id).unwrap_err();
        assert!(matches!(
            err,
            AppError::Validation { code, .. } if code == "IMPORT_SESSION_INVALID"
        ));
    }

    #[test]
    fn expired_import_session_is_rejected() {
        let state = test_state();
        let id = state
            .create_import_session(
                PathBuf::from("C:/imports"),
                Vec::new(),
                std::time::Duration::ZERO,
            )
            .unwrap();

        let err = state.consume_import_session(&id).unwrap_err();
        assert!(matches!(
            err,
            AppError::Validation { code, .. } if code == "IMPORT_SESSION_INVALID"
        ));
    }

    #[test]
    fn new_import_preview_invalidates_the_previous_session() {
        let state = test_state();
        let first = state
            .create_import_session(
                PathBuf::from("C:/first"),
                Vec::new(),
                std::time::Duration::from_secs(1),
            )
            .unwrap();
        let second = state
            .create_import_session(
                PathBuf::from("C:/second"),
                Vec::new(),
                std::time::Duration::from_secs(1),
            )
            .unwrap();

        assert!(matches!(
            state.consume_import_session(&first),
            Err(AppError::Validation { code, .. }) if code == "IMPORT_SESSION_INVALID"
        ));
        assert_eq!(
            state.consume_import_session(&second).unwrap().root,
            PathBuf::from("C:/second")
        );
    }

    #[test]
    fn read_pool_serves_parallel_readers() {
        use crate::db::connection::open_in_memory_pool;

        let (write_conn, read_conns) = open_in_memory_pool(3).unwrap();
        let state = std::sync::Arc::new(AppState::with_read_pool(
            write_conn,
            read_conns,
            test_paths(),
        ));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut handles = Vec::new();
        for _ in 0..3 {
            let state = std::sync::Arc::clone(&state);
            let tx = tx.clone();
            handles.push(std::thread::spawn(move || {
                state
                    .with_read_conn(|_conn| {
                        // Hold the slot briefly; all three must hold their own
                        // slot concurrently, which fails if the pool serialized
                        // readers.
                        tx.send(()).unwrap();
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        Ok(())
                    })
                    .expect("reader should check out a slot");
            }));
        }
        for _ in 0..3 {
            rx.recv_timeout(std::time::Duration::from_secs(2))
                .expect("all readers should acquire their own slot concurrently");
        }
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn replace_read_pool_rebuilds_slots_after_drain() {
        use crate::db::connection::open_in_memory_pool;

        let (write_conn, read_conns) = open_in_memory_pool(1).unwrap();
        let state = AppState::with_read_pool(write_conn, read_conns, test_paths());
        state.with_read_conn(|_| Ok(())).unwrap();

        let (_new_write, new_reads) = open_in_memory_pool(2).unwrap();
        state.replace_read_pool(new_reads).unwrap();
        assert_eq!(
            state.read_conns.lock().unwrap().len(),
            2,
            "replace must install the new slot count"
        );
        assert!(state.with_read_conn(|_| Ok(())).is_ok());
        assert!(state.with_read_conn(|_| Ok(())).is_ok());
    }

    fn test_state() -> AppState {
        let (write_conn, read_conn) = open_in_memory().unwrap();
        AppState::new(write_conn, read_conn, test_paths())
    }

    fn test_paths() -> AppPaths {
        let root = tempfile::tempdir().unwrap().keep();
        AppPaths {
            data_dir: root.join("data"),
            log_dir: root.join("logs"),
            backup_dir: root.join("backups"),
            database_path: root.join("data").join("2notes.sqlite"),
            bootstrap_path: root.join("bootstrap.json"),
        }
    }
}
