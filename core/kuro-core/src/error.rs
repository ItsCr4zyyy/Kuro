//! Chyby, které core hlásí ven. Záměrně hrubé kategorie — UI podle nich
//! rozhoduje, co ukázat (znovu přihlásit, zkusit znovu, nic nenalezeno).

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// Spojení selhalo nebo vypršel čas. Dá se zkusit znovu.
    #[error("síťová chyba: {0}")]
    Network(String),

    #[error("HTTP {status} ({context})")]
    Http { status: u16, context: String },

    /// Token u providera propadl — uživatel se musí přihlásit znovu.
    #[error("přihlášení vypršelo")]
    AuthExpired,

    /// Špatné jméno nebo heslo.
    #[error("neplatné přihlašovací údaje")]
    BadCredentials,

    /// Operace potřebuje přihlášení, ale žádný token není.
    #[error("je potřeba se přihlásit")]
    NotLoggedIn,

    /// Provider odmítl požadavek s vlastním kódem.
    #[error("{provider}: {message} ({code})")]
    Provider { provider: String, code: String, message: String },

    #[error("nenalezeno")]
    NotFound,

    #[error("neočekávaná odpověď: {0}")]
    BadResponse(String),

    #[error("databáze: {0}")]
    Db(String),
}

impl Error {
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Network(_) => true,
            Error::Http { status, .. } => *status == 429 || *status >= 500,
            _ => false,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        match e.status() {
            Some(s) => Error::Http { status: s.as_u16(), context: e.url().map(|u| u.path().to_owned()).unwrap_or_default() },
            None if e.is_decode() => Error::BadResponse(e.to_string()),
            None => Error::Network(e.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
