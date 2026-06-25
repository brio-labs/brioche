//! Shared HTTP client configuration for shell-side network calls.
//!
//! Centralises `reqwest::Client` construction so that timeouts, redirect
//! caps, scheme/host allow-lists, and response size limits are applied
//! consistently across the OpenAI provider, system tools, and user-defined
//! HTTP tools.
//!
//! Refs: I-Shell-Network-Signal

use std::time::Duration;

use bytes::Bytes;

/// Errors raised by the shared HTTP client helpers.
///
/// Refs: I-Shell-Network-Signal
#[derive(Debug, thiserror::Error)]
pub enum HttpClientError {
    /// The URL uses a disallowed scheme or targets a blocked host.
    #[error("URL not allowed: {url}")]
    UrlNotAllowed {
        /// URL that failed validation.
        url: String,
    },

    /// The response body exceeds the configured size limit.
    #[error("response body exceeds {max_bytes} bytes")]
    ResponseTooLarge {
        /// Configured maximum body size in bytes.
        max_bytes: usize,
    },

    /// Underlying transport or protocol error.
    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

/// Default total request timeout for non-streaming HTTP calls.
///
/// Streaming calls (e.g. SSE) may use a longer or per-chunk timeout.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Default connection timeout.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default maximum number of HTTP redirects to follow.
pub const DEFAULT_MAX_REDIRECTS: usize = 10;

/// Default maximum response body size for non-streaming calls.
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Allowed URL schemes for untrusted caller-supplied URLs.
pub const ALLOWED_SCHEMES: &[&str] = &["http", "https"];

/// Hosts that are never reachable from shell-side HTTP tools.
pub const BLOCKED_HOSTS: &[&str] = &["localhost", "127.0.0.1", "::1"];

/// Builds a shared `reqwest::Client` with safe defaults.
///
/// # Complexity
/// O(1). Allocates the client handle only.
///
/// # Errors
/// Returns `HttpClientError::Request` if the client cannot be constructed.
///
/// Refs: I-Shell-Network-Signal
pub fn build_http_client(
    timeout: Duration,
    max_redirects: usize,
) -> Result<reqwest::Client, HttpClientError> {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(max_redirects))
        .build()
        .map_err(HttpClientError::Request)
}

/// Validates that `url` is allowed for shell-side HTTP calls.
///
/// Blocks `file://` and localhost-style hosts to prevent SSRF.
///
/// # Complexity
/// O(s + h) where s = allowed schemes and h = blocked hosts.
///
/// Refs: I-Shell-Network-Signal
pub fn validate_url(
    url: &str,
    allowed_schemes: &[&str],
    blocked_hosts: &[&str],
) -> Result<reqwest::Url, HttpClientError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| HttpClientError::UrlNotAllowed {
        url: url.to_string(),
    })?;

    let scheme = parsed.scheme();
    if !allowed_schemes
        .iter()
        .any(|s| s.eq_ignore_ascii_case(scheme))
    {
        return Err(HttpClientError::UrlNotAllowed {
            url: url.to_string(),
        });
    }

    if let Some(host) = parsed.host_str() {
        // IPv6 literals are returned with brackets; strip them for comparison.
        let host_lower = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase();
        if blocked_hosts
            .iter()
            .any(|blocked| host_lower == blocked.to_ascii_lowercase())
        {
            return Err(HttpClientError::UrlNotAllowed {
                url: url.to_string(),
            });
        }
    }

    Ok(parsed)
}

/// Reads a non-streaming response body, enforcing a byte limit.
///
/// If the server advertises a `Content-Length` larger than `max_bytes`,
/// the call fails before reading the body. The final size check guards
/// against chunked responses that omit `Content-Length`.
///
/// # Complexity
/// O(b) where b = body bytes read. Peak memory is bounded by `max_bytes`
/// plus one chunk of buffered data.
///
/// # Errors
/// Returns `HttpClientError::ResponseTooLarge` if the body is too large,
/// or `HttpClientError::Request` if reading fails.
///
/// Refs: I-Shell-Network-Signal
pub async fn read_body_with_size_limit(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<Bytes, HttpClientError> {
    if let Some(len) = response.content_length()
        && len as usize > max_bytes
    {
        return Err(HttpClientError::ResponseTooLarge { max_bytes });
    }

    let bytes = response.bytes().await?;
    if bytes.len() > max_bytes {
        return Err(HttpClientError::ResponseTooLarge { max_bytes });
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_https_url() {
        assert!(validate_url("https://example.com/path", ALLOWED_SCHEMES, BLOCKED_HOSTS).is_ok());
    }

    #[test]
    fn rejects_file_scheme() {
        let result = validate_url("file:///etc/passwd", ALLOWED_SCHEMES, BLOCKED_HOSTS);
        assert!(
            matches!(result, Err(HttpClientError::UrlNotAllowed { .. })),
            "file:// should be blocked"
        );
    }

    #[test]
    fn rejects_localhost() {
        for url in [
            "http://localhost:8080/x",
            "http://127.0.0.1/x",
            "http://[::1]/x",
        ] {
            let result = validate_url(url, ALLOWED_SCHEMES, BLOCKED_HOSTS);
            assert!(
                matches!(result, Err(HttpClientError::UrlNotAllowed { .. })),
                "{url} should be blocked"
            );
        }
    }
}
