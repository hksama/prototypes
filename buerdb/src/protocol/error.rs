use thiserror::Error;

#[derive(Debug, Error)]
pub enum RespError {
    #[error("Invalid number of arguments provided for command")]
    InvalidArgLength,
    #[error("Invalid frame provided for command")]
    InvalidFrame,
    #[error("Invalid length provided for command")]
    InvalidLength,
    #[error("Unexpected end of frame")]
    UnexpectedEof,
    #[error("Unknown command provided")]
    UnknownCommand,
    #[error("Invalid flags provided for command")]
    InvalidFlags,
    #[error("Unexpected command provided for command")]
    UnexpectedCommand,
}
