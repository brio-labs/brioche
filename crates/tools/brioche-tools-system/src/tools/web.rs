//! Web fetch tool.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use reqwest::redirect::Policy;
use tokio_util::sync::CancellationToken;

use crate::registry::{SystemTool, ToolError};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_REDIRECTS: usize = 3;

#[derive(Clone, Copy, Debug)]
struct HttpPolicy {
    allow_loopback: bool,
    timeout: Duration,
    max_response_bytes: usize,
    max_redirects: usize,
}

impl HttpPolicy {
    fn external() -> Self {
        Self {
            allow_loopback: false,
            timeout: REQUEST_TIMEOUT,
            max_response_bytes: MAX_RESPONSE_BYTES,
            max_redirects: MAX_REDIRECTS,
        }
    }
}

/// Performs an HTTP GET on a URL.
/// Refs: docs/SPECS.md §Book III-C
pub struct FetchUrlTool;

#[async_trait::async_trait]
impl SystemTool for FetchUrlTool {
    fn name(&self) -> String {
        "fetch_url".into()
    }

    fn description(&self) -> String {
        "Fetch the content of a URL via HTTP GET.".into()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        let mut props = serde_json::Map::new();

        let mut url = serde_json::Map::new();
        url.insert("type".into(), serde_json::Value::String("string".into()));
        url.insert(
            "description".into(),
            serde_json::Value::String("URL to fetch".into()),
        );
        props.insert("url".into(), serde_json::Value::Object(url));

        let mut schema = serde_json::Map::new();
        schema.insert("type".into(), serde_json::Value::String("object".into()));
        schema.insert("properties".into(), serde_json::Value::Object(props));
        schema.insert(
            "required".into(),
            serde_json::Value::Array(vec![serde_json::Value::String("url".into())]),
        );
        serde_json::Value::Object(schema)
    }

    async fn run(
        &self,
        args: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<String, ToolError> {
        let url = args["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'url'".into()))?;
        fetch_url(url, cancel).await
    }
}

/// Performs an external HTTP GET with scheme, host, redirect, timeout, and body limits.
///
/// Refs: docs/SPECS.md §Book III-C
/// # Cancel safety
/// Dropping this future cancels the in-flight request. No shared state is mutated.
pub async fn fetch_url(url: &str, cancel: CancellationToken) -> Result<String, ToolError> {
    execute_request(
        HttpMethod::Get,
        url,
        &BTreeMap::new(),
        None,
        cancel,
        HttpPolicy::external(),
    )
    .await
}

/// Performs an external HTTP POST with scheme, host, redirect, timeout, and body limits.
///
/// Refs: I-Shell-Runtime-OnlyIO
/// # Cancel safety
/// Dropping this future cancels the in-flight request. No shared state is mutated.
pub async fn post_json(
    url: &str,
    headers: &BTreeMap<String, String>,
    body: serde_json::Value,
    cancel: CancellationToken,
) -> Result<String, ToolError> {
    execute_request(
        HttpMethod::PostJson(body),
        url,
        headers,
        None,
        cancel,
        HttpPolicy::external(),
    )
    .await
}

enum HttpMethod {
    Get,
    PostJson(serde_json::Value),
}

async fn execute_request(
    method: HttpMethod,
    url: &str,
    headers: &BTreeMap<String, String>,
    content_type: Option<&str>,
    cancel: CancellationToken,
    policy: HttpPolicy,
) -> Result<String, ToolError> {
    let parsed = validate_external_url(url, policy)?;
    let client = reqwest::Client::builder()
        .timeout(policy.timeout)
        .redirect(Policy::limited(policy.max_redirects))
        .build()
        .map_err(|err| ToolError::Io(std::io::Error::other(err.to_string())))?;

    let mut request = match method {
        HttpMethod::Get => client.get(parsed),
        HttpMethod::PostJson(body) => client.post(parsed).json(&body),
    };
    if let Some(value) = content_type {
        request = request.header("Content-Type", value);
    }
    for (key, value) in headers {
        request = request.header(key, value);
    }

    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            return Err(ToolError::Io(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            )));
        }
        result = tokio::time::timeout(policy.timeout, request.send()) => result,
    };

    let response = response
        .map_err(|_| {
            ToolError::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "request exceeded timeout",
            ))
        })?
        .map_err(request_error)?;
    let status = response.status();
    let body = tokio::time::timeout(
        policy.timeout,
        limited_body(response, policy.max_response_bytes),
    )
    .await
    .map_err(|_| {
        ToolError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "response body exceeded timeout",
        ))
    })??;

    if !status.is_success() {
        return Err(ToolError::Io(std::io::Error::other(format!(
            "HTTP {}: {}",
            status, body
        ))));
    }

    Ok(body)
}

fn validate_external_url(url: &str, policy: HttpPolicy) -> Result<reqwest::Url, ToolError> {
    let parsed = reqwest::Url::parse(url).map_err(|err| ToolError::InvalidArgs(err.to_string()))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(ToolError::SandboxDenied(format!(
                "URL scheme '{scheme}' is not allowed"
            )));
        }
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| ToolError::SandboxDenied("URL host is required".into()))?;
    if !policy.allow_loopback && is_blocked_host(host) {
        return Err(ToolError::SandboxDenied(format!(
            "URL host '{host}' is not allowed"
        )));
    }

    Ok(parsed)
}

fn is_blocked_host(host: &str) -> bool {
    let normalized = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if matches!(normalized.as_str(), "localhost" | "localhost.localdomain") {
        return true;
    }
    match normalized.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => is_blocked_ipv4(ip),
        Ok(IpAddr::V6(ip)) => is_blocked_ipv6(ip),
        Err(_) => false,
    }
}

fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
    let first_octet = ip.octets().first().copied().map_or(0, |octet| octet);
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || first_octet == 0
}

fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    let first_segment = ip.segments().first().copied().map_or(0, |segment| segment);
    ip.is_loopback() || ip.is_unspecified() || first_segment & 0xfe00 == 0xfc00
}

fn request_error(err: reqwest::Error) -> ToolError {
    if err.is_timeout() {
        return ToolError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "request exceeded timeout",
        ));
    }
    ToolError::Io(std::io::Error::other(err.to_string()))
}

async fn limited_body(
    mut response: reqwest::Response,
    max_response_bytes: usize,
) -> Result<String, ToolError> {
    let mut bytes = Vec::with_capacity(max_response_bytes.min(4096));
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| ToolError::Io(std::io::Error::other(err.to_string())))?
    {
        if bytes.len().saturating_add(chunk.len()) > max_response_bytes {
            return Err(ToolError::Io(std::io::Error::other(format!(
                "response body exceeded {max_response_bytes} bytes"
            ))));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes)
        .map_err(|err| ToolError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, err)))
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn required_error<T>(result: Result<T, ToolError>, context: &str) -> ToolError {
        match result {
            Ok(_) => ToolError::InvalidArgs(format!("expected error: {context}")),
            Err(err) => err,
        }
    }

    #[test]
    fn validate_external_url_rejects_file_scheme() {
        let err = required_error(
            validate_external_url("file:///etc/passwd", HttpPolicy::external()),
            "file URL must be blocked",
        );
        assert!(err.to_string().contains("scheme 'file'"));
    }

    #[test]
    fn validate_external_url_rejects_localhost() {
        let err = required_error(
            validate_external_url("http://localhost:8080", HttpPolicy::external()),
            "localhost URL must be blocked",
        );
        assert!(err.to_string().contains("localhost"));
    }

    #[tokio::test]
    async fn execute_request_caps_redirects() -> Result<(), Box<dyn std::error::Error>> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/start"))
            .respond_with(ResponseTemplate::new(302).append_header("Location", "/again"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/again"))
            .respond_with(ResponseTemplate::new(302).append_header("Location", "/start"))
            .mount(&server)
            .await;

        let mut policy = HttpPolicy::external();
        policy.allow_loopback = true;
        policy.max_redirects = 1;
        let err = required_error(
            execute_request(
                HttpMethod::Get,
                &format!("{}/start", server.uri()),
                &BTreeMap::new(),
                None,
                CancellationToken::new(),
                policy,
            )
            .await,
            "redirect loop must hit the configured cap",
        );
        assert!(err.to_string().contains("redirect"));
        Ok(())
    }

    #[tokio::test]
    async fn execute_request_enforces_timeout() -> Result<(), Box<dyn std::error::Error>> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(200)))
            .mount(&server)
            .await;

        let mut policy = HttpPolicy::external();
        policy.allow_loopback = true;
        policy.timeout = Duration::from_millis(25);
        let err = required_error(
            execute_request(
                HttpMethod::Get,
                &format!("{}/slow", server.uri()),
                &BTreeMap::new(),
                None,
                CancellationToken::new(),
                policy,
            )
            .await,
            "slow response must time out",
        );
        assert!(
            err.to_string().contains("timeout") || err.to_string().contains("deadline"),
            "expected timeout error, got {err}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn execute_request_limits_response_size() -> Result<(), Box<dyn std::error::Error>> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/large"))
            .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(32)))
            .mount(&server)
            .await;

        let mut policy = HttpPolicy::external();
        policy.allow_loopback = true;
        policy.max_response_bytes = 8;
        let err = required_error(
            execute_request(
                HttpMethod::Get,
                &format!("{}/large", server.uri()),
                &BTreeMap::new(),
                None,
                CancellationToken::new(),
                policy,
            )
            .await,
            "large response must be capped",
        );
        assert!(err.to_string().contains("response body exceeded"));
        Ok(())
    }
}
