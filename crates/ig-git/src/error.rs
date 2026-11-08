//! Error type for the git core. Mirrors upstream's use of `VcsException` / `RepoStateException`.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    /// A git command failed. The message is the presentable error output (see `GitCommandResult`).
    #[error("{0}")]
    Command(String),
    #[error("unsupported format of git --version output: {0:?}")]
    VersionParse(String),
    #[error("{message} (X={x:?}, Y={y:?}, line={line:?})")]
    StatusParse {
        message: String,
        x: char,
        y: char,
        line: String,
    },
    #[error("repository state error: {0}")]
    RepoState(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, GitError>;
