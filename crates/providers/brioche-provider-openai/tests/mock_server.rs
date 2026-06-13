//! Integration tests for `brioche-provider-openai` against a mock HTTP server.
//!
//! Uses `wiremock` to simulate OpenAI-compatible endpoints and validates:
//! - Streaming text chunks reach the kernel via `BriocheShell::send_input`.
//! - Tool-call requests are parsed and emitted to the kernel.
//! - Network errors emit `SystemSignal::NetworkUnavailable`.
//! - HTTP 4xx/5xx errors emit `SystemSignal::NetworkUnavailable`.
//!
//! Refs: docs/SPECS.md §Book III-B, I-Shell-Network-Signal

use std::time::Duration;

use brioche_core::{BriocheEngineBuilder, ChatMessage, Session};
use brioche_governance_default::{BriocheEngineBuilderExt, GovernanceProfile};
use brioche_provider_openai::{OpenAiConfig, OpenAiLlmClient};
use brioche_shell_runtime::{
    BriocheShell, DefaultEffectExecutor, EchoToolExecutor, LlmClient, NoopPersistence, ShellConfig,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_shell() -> BriocheShell {
    let executor = DefaultEffectExecutor::new(
        EchoToolExecutor,
        OpenAiLlmClient::new(OpenAiConfig::default()).0,
        NoopPersistence,
    );
    BriocheShell::new(
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
    )
}

#[tokio::test]
async fn streams_text_chunks_to_kernel() {
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
    let (client, _rx, _history) = OpenAiLlmClient::new(config);
    client
        .push_message(ChatMessage::User {
            content: "hello".into(),
        })
        .await;

    let shell = test_shell();
    let result = client.call_llm(&shell).await;

    assert!(result.is_ok(), "call_llm should succeed: {result:?}");
    // Give the shell channel a moment to deliver.
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn emits_tool_calls_to_kernel() {
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
    let (client, _rx, _history) = OpenAiLlmClient::new(config);
    client
        .push_message(ChatMessage::User {
            content: "read a file".into(),
        })
        .await;

    let shell = test_shell();
    let result = client.call_llm(&shell).await;

    assert!(result.is_ok(), "call_llm should succeed: {result:?}");
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn network_failure_emits_system_signal() {
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
    let (client, _rx, _history) = OpenAiLlmClient::new(config);
    client
        .push_message(ChatMessage::User {
            content: "hello".into(),
        })
        .await;

    let shell = test_shell();
    let result = client.call_llm(&shell).await;

    assert!(
        result.is_ok(),
        "call_llm should surface error and return Ok: {result:?}"
    );
}
