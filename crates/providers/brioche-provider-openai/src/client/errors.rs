//! Provider-specific error taxonomy for OpenAI-compatible clients.
//!
//! Errors keep provider context locally and convert to `ShellError` only at
//! the shell runtime boundary.
//!
//! Refs: docs/SPECS.md §Book III-B

use brioche_shell_runtime::ShellError;

/// Provider-specific error returned by `OpenAiLlmClient` operations.
///
/// Preserves OpenAI-specific context (HTTP status, SSE diagnostics, parse
/// failures) and is converted to a generic [`ShellError`] at the trait
/// boundary.
///
/// Refs: docs/SPECS.md §Book III-B
#[derive(Debug, thiserror::Error)]
pub enum OpenAiError {
    /// The HTTP client could not be constructed from the provided configuration.
    ///
    /// Refs: docs/SPECS.md §Book III-B
    #[error("failed to build HTTP client: {0}")]
    HttpClientBuilder(reqwest::Error),
    /// The HTTP request could not be sent or the connection failed.
    #[error("network request failed: {0}")]
    Network(String),
    /// The provider returned a non-success HTTP status.
    #[error("HTTP {status}: {message}")]
    Http {
        /// HTTP status code returned by the provider.
        status: u16,
        /// Compacted error message extracted from the response body.
        message: String,
    },
    /// No SSE data was received within the configured idle timeout.
    #[error("SSE stream idle timeout")]
    IdleTimeout,
    /// The SSE stream failed or contained malformed data.
    #[error("SSE provider error: {0}")]
    Sse(String),
    /// The summary response could not be parsed as JSON.
    #[error("failed to parse summary response: {0}")]
    SummaryParse(String),
}

impl From<OpenAiError> for ShellError {
    fn from(err: OpenAiError) -> Self {
        ShellError::EffectExecution(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use brioche_shell_runtime::ShellError;

    use super::OpenAiError;

    #[test]
    fn openai_error_network_preserves_context() {
        let err = OpenAiError::Network("connection refused".into());
        let shell_err: ShellError = err.into();
        let msg = format!("{shell_err}");
        assert!(msg.contains("connection refused"), "{msg}");
    }

    #[test]
    fn openai_error_http_preserves_status_and_message() {
        let err = OpenAiError::Http {
            status: 503,
            message: "overloaded".into(),
        };
        let shell_err: ShellError = err.into();
        let msg = format!("{shell_err}");
        assert!(msg.contains("503") && msg.contains("overloaded"), "{msg}");
    }

    #[test]
    fn openai_error_sse_preserves_message() {
        let err = OpenAiError::Sse("stream closed".into());
        let shell_err: ShellError = err.into();
        let msg = format!("{shell_err}");
        assert!(msg.contains("stream closed"), "{msg}");
    }
}
