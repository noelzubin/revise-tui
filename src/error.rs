use std::io;
use thiserror::Error;

use crate::store::ID;

pub type ReviseResult<T> = Result<T, ReviseError>;

#[derive(Error, Debug)]
pub enum ReviseError {
    #[error("Rusqlite error: {0}")]
    RusqliteError(#[from] rusqlite::Error),
    #[error("Not found: {0}")]
    NotFoundError(ID),
    #[error("data store disconnected")]
    IOError(#[from] io::Error),
}
