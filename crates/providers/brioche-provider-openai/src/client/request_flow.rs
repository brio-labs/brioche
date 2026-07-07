//! Request payload assembly and transient HTTP execution.
//!
//! This module bridges pure JSON construction from `crate::request` with the
//! provider HTTP retry loop. It never parses SSE events or finalizes assistant
//! history.
//!
//! Refs: docs/SPECS.md §Book III-B, I-Shell-Network-Signal

use std::time::Duration;

use brioche_core::ChatMessage;
use brioche_shell_runtime::{BriocheShell, LlmChunk, ShellError, SystemSignal};

use crate::client::{
    MAX_ERROR_BODY_BYTES, OpenAiError, OpenAiLlmClient, is_retriable_status, limited_error_body,
    parse_retry_after,
};
use crate::request::build_request_body;

impl OpenAiLlmClient {
    /// Build the HTTP request from current history and tools.
    ///
    /// # Complexity
    /// O(n) where n = history length (build_messages copies).
    pub(super) async fn build_request(&self) -> (serde_json::Value, usize) {
        let history_guard = self.history.read().await;
        let transformed = {
            let transform_guard = match self.history_transform.read() {
                Ok(g) => g,
                Err(e) => e.into_inner(),
            };
            match transform_guard.as_ref() {
                Some(transform) => transform(&history_guard),
                None => history_guard.iter().cloned().collect(),
            }
        };
        let messages = crate::request::build_messages(&transformed);
        let msg_count = messages.len();
        drop(history_guard);

        let tools_guard = self.tools_schema.read().await;
        let tools: Option<&[serde_json::Value]> = if tools_guard.is_empty() {
            None
        } else {
            Some(&*tools_guard)
        };

        let body = build_request_body(
            &self.config.model,
            messages,
            self.config.max_tokens,
            self.config.reasoning_effort.as_deref(),
            tools,
            true,
        );
        (body, msg_count)
    }

    /// Build a non-streaming summarization request for the given messages.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub(super) fn build_summary_request(&self, messages: &[ChatMessage]) -> serde_json::Value {
        let summary_prompt = ChatMessage::System {
            content: "Summarize the following conversation concisely. \
                Preserve key facts, user intent, and any decisions or tool results. \
                The summary will replace the messages it covers."
                .into(),
        };
        let mut request_messages = crate::request::build_messages(&[summary_prompt]);
        request_messages.extend(crate::request::build_messages(messages));
        crate::request::build_request_body(
            &self.config.model,
            request_messages,
            self.config.max_tokens.min(512),
            None,
            None,
            false,
        )
    }

    /// Extract the assistant content from a non-streaming completion response.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub(super) fn extract_summary_text(body: &serde_json::Value) -> Option<String> {
        body.get("choices")?
            .as_array()?
            .first()?
            .get("message")?
            .get("content")?
            .as_str()
            .map(|s| s.to_string())
    }

    /// Send the request, retrying transient failures and surfacing fatal errors.
    ///
    /// Applies `config.timeout_ms` as the time-to-first-byte timeout so that
    /// stalled connections fail fast without cutting off long streaming
    /// generations. Retries are performed for network errors, HTTP 5xx
    /// responses, and HTTP 429 responses, honouring `Retry-After` when
    /// present. When retries are exhausted the final error is emitted as
    /// `SystemSignal::NetworkUnavailable`.
    ///
    /// Refs: docs/SPECS.md §Book III-B, I-Shell-Network-Signal
    pub(super) async fn send_request(
        &self,
        shell: &BriocheShell,
        body: &serde_json::Value,
        url: &str,
    ) -> Result<reqwest::Response, ShellError> {
        let body_str = body.to_string();
        let timeout = Duration::from_millis(self.config.timeout_ms);
        let max_backoff = Duration::from_millis(self.retry_config.max_backoff_ms);

        let _ = self.ui_tx.send(LlmChunk::Status(format!(
            "HTTP POST {} — ~{} chars",
            url,
            body_str.len(),
        )));

        let mut attempt: u32 = 0;
        loop {
            let request = self
                .http
                .post(url)
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .header("Content-Type", "application/json")
                .json(body);

            match tokio::time::timeout(timeout, request.send()).await {
                Ok(Ok(response)) if response.status().is_success() => {
                    let _ = self.ui_tx.send(LlmChunk::Status(format!(
                        "HTTP {} — starting SSE stream",
                        response.status()
                    )));
                    return Ok(response);
                }
                Ok(Ok(response)) => {
                    let status = response.status();
                    let retriable = is_retriable_status(status);
                    let retry_after =
                        parse_retry_after(response.headers().get("retry-after"), max_backoff);

                    if !retriable || attempt >= self.retry_config.max_retries {
                        let body_text = limited_error_body(response, MAX_ERROR_BODY_BYTES).await;
                        let compact = if let Ok(json) =
                            serde_json::from_str::<serde_json::Value>(&body_text)
                        {
                            json.get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|m| m.as_str())
                                .map_or_else(|| body_text.clone(), |s| s.to_string())
                        } else {
                            match body_text.lines().next() {
                                Some(line) => line.to_string(),
                                None => body_text.clone(),
                            }
                        };
                        let msg = format!("HTTP {status}: {compact}");
                        tracing::error!(status = %status, error = %body_text, "OpenAI HTTP error");
                        let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
                        shell
                            .send_system_signal(SystemSignal::NetworkUnavailable {
                                reason: msg.clone(),
                            })
                            .await?;
                        return Err(OpenAiError::Http {
                            status: status.as_u16(),
                            message: compact,
                        }
                        .into());
                    }

                    attempt += 1;
                    let delay = match retry_after {
                        Some(delay) => delay,
                        None => Duration::from_millis(self.retry_config.backoff_ms(attempt)),
                    };
                    let _ = self.ui_tx.send(LlmChunk::Status(format!(
                        "HTTP {status} — retry {attempt}/{} in {:?}",
                        self.retry_config.max_retries, delay
                    )));
                    tokio::time::sleep(delay).await;
                }
                Ok(Err(err)) => {
                    let msg = format!("Network error: {err}");
                    if attempt >= self.retry_config.max_retries {
                        tracing::error!(error = %err, "OpenAI request failed");
                        let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
                        shell
                            .send_system_signal(SystemSignal::NetworkUnavailable {
                                reason: msg.clone(),
                            })
                            .await?;
                        return Err(OpenAiError::Network(err.to_string()).into());
                    }
                    attempt += 1;
                    let delay = Duration::from_millis(self.retry_config.backoff_ms(attempt));
                    tracing::warn!(
                        error = %err,
                        attempt,
                        "OpenAI request failed — retrying"
                    );
                    let _ = self.ui_tx.send(LlmChunk::Status(format!(
                        "{msg} — retry {attempt}/{} in {:?}",
                        self.retry_config.max_retries, delay
                    )));
                    tokio::time::sleep(delay).await;
                }
                Err(_) => {
                    let msg = format!("Request timed out after {timeout:?}");
                    if attempt >= self.retry_config.max_retries {
                        tracing::error!(%msg, "OpenAI request timed out");
                        let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
                        shell
                            .send_system_signal(SystemSignal::NetworkUnavailable {
                                reason: msg.clone(),
                            })
                            .await?;
                        return Err(OpenAiError::Network(msg).into());
                    }
                    attempt += 1;
                    let delay = Duration::from_millis(self.retry_config.backoff_ms(attempt));
                    tracing::warn!(%msg, attempt, "OpenAI request timed out — retrying");
                    let _ = self.ui_tx.send(LlmChunk::Status(format!(
                        "{msg} — retry {attempt}/{} in {:?}",
                        self.retry_config.max_retries, delay
                    )));
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
}
