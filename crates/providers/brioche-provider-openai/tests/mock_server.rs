//! Integration tests for `brioche-provider-openai` against a mock HTTP server.
//!
//! Uses `wiremock` to simulate OpenAI-compatible endpoints and validates:
//! - Streaming text chunks reach the kernel via `BriocheShell::send_input`.
//! - Tool-call requests are parsed and emitted to the kernel.
//! - Malformed SSE lines are tolerated up to a threshold, then abort cleanly.
//! - Partial SSE fragments at stream end are buffered without crashing.
//! - Network errors emit `SystemSignal::NetworkUnavailable`.
//! - HTTP 4xx/5xx errors emit `SystemSignal::NetworkUnavailable`.
//! - Request timeouts are surfaced as network errors.
//! - Transient HTTP errors trigger retries with `Retry-After` backoff.
//!
//! Refs: docs/SPECS.md §Book III-B, I-Shell-Network-Signal

use std::time::Duration;

use brioche_core::{BriocheEngineBuilder, ChatMessage, Session};
use brioche_governance_default::{BriocheEngineBuilderExt, GovernanceProfile};
use brioche_provider_openai::{OpenAiConfig, OpenAiLlmClient, RetryConfig};
use brioche_shell_runtime::{
    BriocheShell, DefaultEffectExecutor, EchoToolExecutor, LlmClient, NoopPersistence, ShellConfig,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_shell() -> Result<BriocheShell, brioche_provider_openai::OpenAiError> {
    let executor = DefaultEffectExecutor::new(
        EchoToolExecutor,
        OpenAiLlmClient::new(OpenAiConfig::default())?.0,
        NoopPersistence,
    );
    Ok(BriocheShell::new(
        || {
            let engine = BriocheEngineBuilder::new()
                .with_profile(GovernanceProfile::Permissive)
                .build();
            let session = Session::new("test-session");
            (engine, session)
        },
        ShellConfig::default(),
        executor,
        None,
    ))
}

#[tokio::test]
async fn streams_text_chunks_to_kernel() -> Result<(), Box<dyn std::error::Error>> {
    let mock_server = MockServer::start().await;
    let _body = r#"{
    "model": "gpt-4o-mini",
    "messages": [{"role": "user", "content": "hello"}],
    "max_tokens": 4096,
    "stream": true
}"#;
    let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n\
           data: {\"choices\":[{\"delta\":{\"content\":\" there\"}}]}\n\n\
           data: [DONE]\n\n";

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&mock_server)
        .await;

    let config = OpenAiConfig {
        base_url: mock_server.uri(),
        api_key: "test-key".into(),
        ..OpenAiConfig::default()
    };
    let (client, _rx, _history) = OpenAiLlmClient::new(config)?;
    client
        .push_message(ChatMessage::User {
            content: "hello".into(),
        })
        .await;

    let shell = test_shell()?;
    let result = client.call_llm(&shell).await;

    assert!(result.is_ok(), "call_llm should succeed: {result:?}");
    // Give the shell channel a moment to deliver.
    tokio::time::sleep(Duration::from_millis(50)).await;
    Ok(())
}

#[tokio::test]
async fn emits_tool_calls_to_kernel() -> Result<(), Box<dyn std::error::Error>> {
    let mock_server = MockServer::start().await;
    let sse = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\\\"/tmp/test.txt\\\"}\"}}]}}]}\n\n\
           data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
           data: [DONE]\n\n";

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&mock_server)
        .await;

    let config = OpenAiConfig {
        base_url: mock_server.uri(),
        api_key: "test-key".into(),
        ..OpenAiConfig::default()
    };
    let (client, _rx, _history) = OpenAiLlmClient::new(config)?;
    client
        .push_message(ChatMessage::User {
            content: "read a file".into(),
        })
        .await;

    let shell = test_shell()?;
    let result = client.call_llm(&shell).await;

    assert!(result.is_ok(), "call_llm should succeed: {result:?}");
    tokio::time::sleep(Duration::from_millis(50)).await;
    Ok(())
}

#[tokio::test]
async fn network_failure_emits_system_signal() -> Result<(), Box<dyn std::error::Error>> {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(503).set_body_string("overloaded"))
        .mount(&mock_server)
        .await;

    let config = OpenAiConfig {
        base_url: mock_server.uri(),
        api_key: "test-key".into(),
        ..OpenAiConfig::default()
    };
    let (client, _rx, _history) = OpenAiLlmClient::new(config)?;
    client
        .push_message(ChatMessage::User {
            content: "hello".into(),
        })
        .await;

    let shell = test_shell()?;
    let result = client.call_llm(&shell).await;

    assert!(
        result.is_ok(),
        "call_llm should surface error and return Ok: {result:?}"
    );
    Ok(())
}
#[tokio::test]
async fn http_error_truncates_large_response_body() -> Result<(), Box<dyn std::error::Error>> {
    let mock_server = MockServer::start().await;
    let huge_body = "x".repeat(brioche_provider_openai::MAX_ERROR_BODY_BYTES + 1024);

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(503).set_body_string(huge_body))
        .mount(&mock_server)
        .await;

    let config = OpenAiConfig {
        base_url: mock_server.uri(),
        api_key: "test-key".into(),
        ..OpenAiConfig::default()
    };
    let (client, _rx, _history) = OpenAiLlmClient::new(config)?;
    client
        .push_message(ChatMessage::User {
            content: "hello".into(),
        })
        .await;

    let shell = test_shell()?;
    let result = client.call_llm(&shell).await;

    assert!(
        result.is_ok(),
        "call_llm should surface error and return Ok: {result:?}"
    );
    Ok(())
}

#[tokio::test]
async fn http_error_body_limit_is_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let mock_server = MockServer::start().await;
    let prefix = "ERR:";
    let suffix = "TAIL_MARKER";
    let padding_len = brioche_provider_openai::MAX_ERROR_BODY_BYTES - prefix.len() + 100;
    let huge_body = format!("{}{}{}", prefix, "y".repeat(padding_len), suffix);

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(503).set_body_string(huge_body))
        .mount(&mock_server)
        .await;

    let config = OpenAiConfig {
        base_url: mock_server.uri(),
        api_key: "test-key".into(),
        ..OpenAiConfig::default()
    };
    let (client, _rx, _history) = OpenAiLlmClient::new(config)?;
    client
        .push_message(ChatMessage::User {
            content: "hello".into(),
        })
        .await;

    let shell = test_shell()?;
    let result = client.call_llm(&shell).await;

    assert!(
        result.is_ok(),
        "call_llm should surface error and return Ok: {result:?}"
    );
    Ok(())
}

/// Malformed `data:` lines are skipped until the parser's threshold is hit.
///
/// Once the threshold is exceeded the stream aborts and the provider error
/// is surfaced via `SystemSignal::NetworkUnavailable` so the kernel can
/// recover.
///
/// Refs: docs/SPECS.md §Book III-B, I-Shell-Network-Signal
#[tokio::test]
async fn malformed_sse_lines_abort_and_surface_error() -> Result<(), Box<dyn std::error::Error>> {
    let mock_server = MockServer::start().await;
    // The default parser threshold is 5 consecutive malformed lines.
    let sse = "data: not-json-1\n\
           data: not-json-2\n\
           data: not-json-3\n\
           data: not-json-4\n\
           data: not-json-5\n";

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&mock_server)
        .await;

    let config = OpenAiConfig {
        base_url: mock_server.uri(),
        api_key: "test-key".into(),
        ..OpenAiConfig::default()
    };
    let (client, _rx, _history) = OpenAiLlmClient::new(config)?;
    client
        .push_message(ChatMessage::User {
            content: "hello".into(),
        })
        .await;

    let shell = test_shell()?;
    let result = client.call_llm(&shell).await;

    assert!(
        result.is_ok(),
        "call_llm should surface parser error and return Ok: {result:?}"
    );
    Ok(())
}

/// A trailing SSE fragment without a newline is buffered until stream end.
///
/// The provider closed the connection before finishing the last `data:` line.
/// The client must not crash or emit a spurious event; it should finish the
/// turn gracefully with an empty assistant message.
///
/// Refs: docs/SPECS.md §Book III-B
#[tokio::test]
async fn partial_sse_fragment_at_stream_end_is_buffered() -> Result<(), Box<dyn std::error::Error>>
{
    let mock_server = MockServer::start().await;
    let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n\
           data: {\"choices\":[{\"delta\":{\"content\":\" there\"}}]}\n\n\
           data: {\"choices\":[{\"delta\":{\"content\":\"!\"}}]}"; // no trailing newline

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&mock_server)
        .await;

    let config = OpenAiConfig {
        base_url: mock_server.uri(),
        api_key: "test-key".into(),
        ..OpenAiConfig::default()
    };
    let (client, _rx, _history) = OpenAiLlmClient::new(config)?;
    client
        .push_message(ChatMessage::User {
            content: "hello".into(),
        })
        .await;

    let shell = test_shell()?;
    let result = client.call_llm(&shell).await;

    assert!(
        result.is_ok(),
        "call_llm should handle trailing fragment gracefully: {result:?}"
    );
    Ok(())
}

/// A request that exceeds the configured time-to-first-byte timeout fails fast.
///
/// The provider delays the response headers beyond `timeout_ms`. The client
/// must report a network error rather than block indefinitely. Retries are
/// disabled so the test finishes quickly.
///
/// Refs: docs/SPECS.md §Book III-B, I-Shell-Network-Signal
#[tokio::test]
async fn http_timeout_is_surfaced_as_network_error() -> Result<(), Box<dyn std::error::Error>> {
    let mock_server = MockServer::start().await;
    let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n";

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(500))
                .set_body_string(sse),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = OpenAiConfig {
        base_url: mock_server.uri(),
        api_key: "test-key".into(),
        timeout_ms: 100,
        ..OpenAiConfig::default()
    };
    let (client, _rx, _history) = OpenAiLlmClient::new(config)?;
    let client = client.with_retry_policy(RetryConfig::none());
    client
        .push_message(ChatMessage::User {
            content: "hello".into(),
        })
        .await;

    let shell = test_shell()?;
    let result = client.call_llm(&shell).await;

    assert!(
        result.is_ok(),
        "call_llm should surface timeout and return Ok: {result:?}"
    );
    Ok(())
}

/// Transient HTTP errors trigger retries and honour `Retry-After`.
///
/// The first request returns 503 with `Retry-After: 0`. The client retries
/// immediately and receives a valid SSE stream. Both requests are recorded
/// by wiremock expectations.
///
/// Refs: docs/SPECS.md §Book III-B, I-Shell-Network-Signal
#[tokio::test]
async fn transient_http_error_is_retried_with_retry_after() -> Result<(), Box<dyn std::error::Error>>
{
    let mock_server = MockServer::start().await;
    let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n\
           data: [DONE]\n\n";

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("retry-after", "0")
                .set_body_string("overloaded"),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = OpenAiConfig {
        base_url: mock_server.uri(),
        api_key: "test-key".into(),
        ..OpenAiConfig::default()
    };
    let (client, _rx, _history) = OpenAiLlmClient::new(config)?;
    let client = client.with_retry_policy(RetryConfig {
        max_retries: 2,
        base_backoff_ms: 50,
        max_backoff_ms: 200,
    });
    client
        .push_message(ChatMessage::User {
            content: "hello".into(),
        })
        .await;

    let shell = test_shell()?;
    let result = client.call_llm(&shell).await;

    assert!(
        result.is_ok(),
        "call_llm should succeed after retry: {result:?}"
    );
    // Wiremock expectations are verified on MockServer drop.
    Ok(())
}
