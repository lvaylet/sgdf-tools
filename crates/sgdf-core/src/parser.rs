use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("Erreur de décodage encodage : {0}")]
    Encoding(String),

    #[error("Format HTML invalide ou table absente")]
    InvalidFormat,

    #[error("Erreur I/O : {0}")]
    Io(#[from] std::io::Error),
}
