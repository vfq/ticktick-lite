use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("JSON 序列化错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("记录不存在: {0}")]
    NotFound(String),
    /// 唯一约束冲突（如重名标签），需要给用户看的可读原因
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
