use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("data error: {0}")]
    Data(String),
    #[error("PDF error: {0}")]
    Pdf(String),
    #[error("image error: {0}")]
    Image(String),
    #[error("store error: {0}")]
    Store(#[from] daftar_store::StoreError),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ExportError>;
