use serde::Serialize;
use ts_rs::TS;

pub type AppResult<T> = Result<T, AppError>;
pub type CommandResult<T> = Result<T, AppErrorResponse>;

#[derive(Debug, Serialize, Clone, TS)]
pub struct AppErrorResponse {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    Validation { code: &'static str, message: String },
    #[error("{message}")]
    NotFound { code: &'static str, message: String },
    #[error("revision conflict")]
    RevisionConflict,
    #[error("{message}")]
    Migration { code: &'static str, message: String },
    #[error("{message}")]
    System { code: &'static str, message: String },
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Yaml(String),
}

impl AppError {
    pub fn validation(code: &'static str, message: impl Into<String>) -> Self {
        Self::Validation {
            code,
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            code: "NOT_FOUND",
            message: message.into(),
        }
    }

    pub fn migration(code: &'static str, message: impl Into<String>) -> Self {
        Self::Migration {
            code,
            message: message.into(),
        }
    }

    pub fn system(code: &'static str, message: impl Into<String>) -> Self {
        Self::System {
            code,
            message: message.into(),
        }
    }

    fn response(&self) -> AppErrorResponse {
        match self {
            Self::Validation { code, message } => AppErrorResponse {
                code: (*code).to_string(),
                message: message.clone(),
                recoverable: true,
            },
            Self::NotFound { code, message } => AppErrorResponse {
                code: (*code).to_string(),
                message: message.clone(),
                recoverable: true,
            },
            Self::RevisionConflict => AppErrorResponse {
                code: "REVISION_CONFLICT".to_string(),
                message: "内容已被更新，请重新加载后再保存".to_string(),
                recoverable: true,
            },
            Self::Migration { code, message } => AppErrorResponse {
                code: (*code).to_string(),
                message: message.clone(),
                recoverable: false,
            },
            Self::System { code, message } => AppErrorResponse {
                code: (*code).to_string(),
                message: message.clone(),
                recoverable: true,
            },
            Self::Db(_) => AppErrorResponse {
                code: "DB_FAILED".to_string(),
                message: "数据库操作失败".to_string(),
                recoverable: true,
            },
            Self::Io(_) => AppErrorResponse {
                code: "IO_FAILED".to_string(),
                message: "本地文件操作失败".to_string(),
                recoverable: true,
            },
            Self::Json(_) => AppErrorResponse {
                code: "CONFIG_INVALID".to_string(),
                message: "本地配置文件格式异常".to_string(),
                recoverable: false,
            },
            Self::Yaml(_) => AppErrorResponse {
                code: "MARKDOWN_EXPORT_FAILED".to_string(),
                message: "Markdown 导出失败".to_string(),
                recoverable: true,
            },
        }
    }
}

impl From<AppError> for AppErrorResponse {
    fn from(value: AppError) -> Self {
        log::error!("app_error code={} source={}", value.response().code, value);
        value.response()
    }
}

impl From<tauri::Error> for AppError {
    fn from(value: tauri::Error) -> Self {
        log::error!("tauri_error source={value}");
        Self::system("TAURI_FAILED", "桌面窗口操作失败")
    }
}
