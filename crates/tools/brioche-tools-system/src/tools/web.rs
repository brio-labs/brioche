//! Web fetch tool.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use tokio_util::sync::CancellationToken;

use crate::registry::{SystemTool, ToolError};

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
        use brioche_shell_runtime::{
            ALLOWED_SCHEMES, BLOCKED_HOSTS, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_RESPONSE_BYTES,
            DEFAULT_REQUEST_TIMEOUT, HttpClientError, build_http_client, read_body_with_size_limit,
            validate_url,
        };

        let url = args["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'url'".into()))?;

        validate_url(url, ALLOWED_SCHEMES, BLOCKED_HOSTS).map_err(|err| match err {
            HttpClientError::UrlNotAllowed { .. } => {
                ToolError::InvalidArgs(format!("URL not allowed: {url}"))
            }
            other => ToolError::Io(std::io::Error::other(other.to_string())),
        })?;

        let client = build_http_client(DEFAULT_REQUEST_TIMEOUT, DEFAULT_MAX_REDIRECTS)
            .map_err(|err| ToolError::Io(std::io::Error::other(err.to_string())))?;
        let request = client.get(url);

        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                return Err(ToolError::Io(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled",
                )));
            }
            result = request.send() => result,
        };

        let response =
            response.map_err(|err| ToolError::Io(std::io::Error::other(err.to_string())))?;

        let status = response.status();
        let body = read_body_with_size_limit(response, DEFAULT_MAX_RESPONSE_BYTES)
            .await
            .map_err(|err| ToolError::Io(std::io::Error::other(err.to_string())))?;
        let body = String::from_utf8_lossy(&body);

        if !status.is_success() {
            return Err(ToolError::Io(std::io::Error::other(format!(
                "HTTP {}: {}",
                status, body
            ))));
        }

        Ok(body.into_owned())
    }
}
