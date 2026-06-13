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
    fn name(&self) -> &'static str {
        "fetch_url"
    }

    fn description(&self) -> &'static str {
        "Fetch the content of a URL via HTTP GET."
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

        let client = reqwest::Client::new();
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
        let body = response
            .text()
            .await
            .map_err(|err| ToolError::Io(std::io::Error::other(err.to_string())))?;

        if !status.is_success() {
            return Err(ToolError::Io(std::io::Error::other(format!(
                "HTTP {}: {}",
                status, body
            ))));
        }

        Ok(body)
    }
}
