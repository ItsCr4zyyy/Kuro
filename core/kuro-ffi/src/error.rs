use kuro_core::Error;

/// V Kotlinu `KuroException` se podtřídami — UI podle nich rozhoduje,
/// co ukázat (přihlásit znovu, zkusit znovu, nic nenalezeno).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum KuroError {
    #[error("síťová chyba: {msg}")]
    Network { msg: String },
    #[error("přihlášení vypršelo")]
    AuthExpired,
    #[error("neplatné přihlašovací údaje")]
    BadCredentials,
    #[error("je potřeba se přihlásit")]
    NotLoggedIn,
    #[error("{provider}: {msg} ({code})")]
    Provider { provider: String, code: String, msg: String },
    #[error("nenalezeno")]
    NotFound,
    #[error("{msg}")]
    Other { msg: String },
}

impl From<Error> for KuroError {
    fn from(e: Error) -> Self {
        match e {
            Error::Network(msg) => KuroError::Network { msg },
            Error::Http { status, context } if status == 429 || status >= 500 => {
                KuroError::Network { msg: format!("HTTP {status} ({context})") }
            }
            Error::AuthExpired => KuroError::AuthExpired,
            Error::BadCredentials => KuroError::BadCredentials,
            Error::NotLoggedIn => KuroError::NotLoggedIn,
            Error::Provider { provider, code, message } => KuroError::Provider { provider, code, msg: message },
            Error::NotFound => KuroError::NotFound,
            other => KuroError::Other { msg: other.to_string() },
        }
    }
}

pub type Result<T> = std::result::Result<T, KuroError>;
