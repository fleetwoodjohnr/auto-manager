use serde::Serialize;

/// Error returned by every operation. Serialized to the UI as `{ kind, message, field }`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    Validation { message: String, field: Option<String> },
    #[error("{0}")]
    NotFound(String),
    /// Someone else changed the record, or the operation conflicts with existing data.
    #[error("{0}")]
    Conflict(String),
    /// A remote data provider failed. `offline` distinguishes "no network" from a bad answer.
    #[error("{message}")]
    Provider { message: String, offline: bool },
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("File error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Data error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn invalid(field: &str, message: impl Into<String>) -> Self {
        AppError::Validation { message: message.into(), field: Some(field.to_string()) }
    }
    pub fn msg(message: impl Into<String>) -> Self {
        AppError::Validation { message: message.into(), field: None }
    }
    pub fn not_found(what: &str) -> Self {
        AppError::NotFound(format!("{what} was not found. It may have been deleted."))
    }
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Validation { .. } => "validation",
            AppError::NotFound(_) => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::Provider { offline: true, .. } => "offline",
            AppError::Provider { .. } => "provider",
            AppError::Db(_) => "database",
            AppError::Io(_) => "io",
            AppError::Json(_) => "data",
            AppError::Other(_) => "other",
        }
    }

    /// Map a CHECK-constraint failure from the stock tables to a message a person understands.
    /// Everything else passes through unchanged.
    pub fn from_stock(e: rusqlite::Error) -> Self {
        let text = e.to_string();
        if text.contains("CHECK constraint failed") && (text.contains("on_hand") || text.contains("reserved")) {
            return AppError::Conflict("Not enough stock available for that. Nothing was changed.".into());
        }
        AppError::Db(e)
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 3)?;
        st.serialize_field("kind", self.kind())?;
        let message = match self {
            // Internal details go to the log, not the screen.
            AppError::Db(e) => {
                tracing::error!("database error: {e}");
                "Something went wrong saving or reading your data. Nothing was changed.".to_string()
            }
            _ => self.to_string(),
        };
        st.serialize_field("message", &message)?;
        let field = match self {
            AppError::Validation { field, .. } => field.clone(),
            _ => None,
        };
        st.serialize_field("field", &field)?;
        st.end()
    }
}
