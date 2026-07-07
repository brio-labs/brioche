//! Generic AMP Core-compatible memory client.
//!
//! This provider talks to any backend that implements the Agent Memory Protocol
//! (AMP) Core verbs over HTTP: `amp.encode`, `amp.recall`, `amp.forget`, and
//! `amp.stats`. Configuring a new memory backend only requires an endpoint and
//! an API key; no new Rust code is needed.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, RwLock};

use brioche_shell_runtime::util::system_time_secs;
use brioche_shell_runtime::{ToolSchemaProperty, ToolSchemaPropertyType, openai_function_tool};
use serde::{Deserialize, Serialize};

use super::memory_provider::{MemoryEntry, MemoryProvider, MemoryQuery, MemorySessionContext};
use super::{ExtensionMetadata, PanelSlot, PersistenceError};

/// Configuration for a generic AMP-compatible memory endpoint.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AmpMemoryEndpoint {
    /// Machine-readable provider id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Base URL of the AMP server.
    pub url: String,
    /// Optional API key.
    pub api_key: Option<String>,
    /// Default scope for encode/recall operations.
    pub scope: Option<String>,
}

/// A generic memory provider backed by an AMP Core-compatible HTTP server.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct AmpMemoryProvider {
    endpoint: AmpMemoryEndpoint,
    client: Arc<RwLock<Option<reqwest::Client>>>,
    session_context: Arc<RwLock<Option<MemorySessionContext>>>,
}

/// AMP encode request.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize)]
struct AmpEncodeRequest {
    content: String,
    thought_type: String,
    tags: Vec<String>,
    #[serde(flatten)]
    scope: BTreeMap<String, String>,
}

/// AMP recall request.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize)]
struct AmpRecallRequest {
    query: String,
    limit: usize,
    #[serde(flatten)]
    scope: BTreeMap<String, String>,
}

/// AMP forget request.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize)]
struct AmpForgetRequest {
    key: String,
}

/// Generic AMP response envelope.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Deserialize)]
struct AmpResponse<T> {
    data: Option<T>,
    error: Option<AmpError>,
}

#[derive(Clone, Debug, Deserialize)]
struct AmpError {
    code: String,
    message: String,
}

/// A thought returned by `amp.recall`.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Deserialize)]
struct AmpThought {
    id: Option<String>,
    content: String,
    thought_type: Option<String>,
    created_at: Option<u64>,
}

impl AmpMemoryProvider {
    /// Creates a provider pointing at the given endpoint.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn new(endpoint: AmpMemoryEndpoint) -> Self {
        Self {
            endpoint,
            client: Arc::new(RwLock::new(None)),
            session_context: Arc::new(RwLock::new(None)),
        }
    }

    fn ensure_client(&self) -> Result<reqwest::Client, PersistenceError> {
        // Fast path: client already initialized and shared across clones.
        if let Ok(guard) = self.client.read()
            && let Some(client) = guard.as_ref()
        {
            return Ok(client.clone());
        }

        let mut builder = reqwest::Client::builder();
        builder = builder.timeout(std::time::Duration::from_secs(30));
        if let Some(api_key) = &self.endpoint.api_key {
            let mut headers = reqwest::header::HeaderMap::new();
            let value = match reqwest::header::HeaderValue::from_str(api_key) {
                Ok(v) => v,
                Err(_) => {
                    return Err(PersistenceError::InvalidInput(
                        "Invalid API key header value".into(),
                    ));
                }
            };
            headers.insert("Authorization", value);
            builder = builder.default_headers(headers);
        }
        let client = builder.build()?;

        if let Ok(mut guard) = self.client.write() {
            *guard = Some(client.clone());
        }
        Ok(client)
    }

    fn base_url(&self) -> String {
        let url = self.endpoint.url.trim_end_matches('/');
        if url.is_empty() {
            "http://localhost".into()
        } else {
            url.into()
        }
    }

    fn scope(&self) -> BTreeMap<String, String> {
        let mut scope = BTreeMap::new();
        let ctx = self
            .session_context
            .read()
            .ok()
            .and_then(|guard| (*guard).clone());
        if let Some(ctx) = ctx {
            scope.insert("session_id".into(), ctx.session_id.clone());
            if !ctx.workspace.is_empty() {
                scope.insert("workspace_id".into(), ctx.workspace.clone());
            }
            if let Some(user_id) = &ctx.user_id {
                scope.insert("user_id".into(), user_id.clone());
            }
            if let Some(agent_id) = &ctx.agent_id {
                scope.insert("agent_id".into(), agent_id.clone());
            }
        }
        if let Some(default_scope) = &self.endpoint.scope
            && !default_scope.is_empty()
        {
            scope.insert("scope".into(), default_scope.clone());
        }
        scope
    }

    async fn post<Req, Res>(&self, path: &str, body: &Req) -> Result<Res, PersistenceError>
    where
        Req: Serialize,
        Res: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url(), path);
        let client = self.ensure_client()?;
        let response = client.post(&url).json(body).send().await?;
        let status = response.status();
        let envelope: AmpResponse<Res> = response.json().await?;
        if let Some(err) = envelope.error {
            return Err(PersistenceError::Amp {
                code: err.code,
                message: err.message,
            });
        }
        match envelope.data {
            Some(data) => Ok(data),
            None => Err(PersistenceError::Amp {
                code: status.to_string(),
                message: "empty data".into(),
            }),
        }
    }
}

impl MemoryProvider for AmpMemoryProvider {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: self.endpoint.id.clone(),
            name: self.endpoint.name.clone(),
            version: "0.1.0".into(),
            default_panel: Some(PanelSlot::Right),
            enabled: false,
        }
    }

    fn initialize(&self, ctx: MemorySessionContext) -> Result<(), PersistenceError> {
        let mut guard = self.session_context.write()?;
        *guard = Some(ctx);
        Ok(())
    }

    fn list(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>, PersistenceError> {
        // AMP Core does not have a generic list verb; use recall with an empty query.
        let q = match &query.query {
            Some(s) => s.clone(),
            None => String::new(),
        };
        let category = match &query.category {
            Some(c) => c.clone(),
            None => String::new(),
        };
        let this = self.clone();
        block_on(async move {
            let request = AmpRecallRequest {
                query: q,
                limit: 50,
                scope: this.scope(),
            };
            let thoughts: Vec<AmpThought> = this.post("/v1/recall", &request).await?;
            Ok(thoughts
                .into_iter()
                .map(|t| AmpMemoryProvider::thought_to_entry(&this.endpoint.id, t, &category))
                .collect())
        })
    }

    fn set(
        &mut self,
        key: String,
        value: String,
        category: String,
    ) -> Result<(), PersistenceError> {
        let mut scope = self.scope();
        scope.insert("key".into(), key);
        let request = AmpEncodeRequest {
            content: value,
            thought_type: category,
            tags: Vec::new(),
            scope,
        };
        let this = self.clone();
        block_on(async move {
            this.post::<_, serde_json::Value>("/v1/encode", &request)
                .await
                .map(|_| ())
        })
    }

    fn delete(&mut self, key: &str) -> Result<bool, PersistenceError> {
        let request = AmpForgetRequest { key: key.into() };
        let this = self.clone();
        block_on(async move {
            this.post::<_, serde_json::Value>("/v1/forget", &request)
                .await
                .map(|_| true)
        })
    }

    fn recall(
        &self,
        conversation_summary: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, PersistenceError> {
        let this = self.clone();
        let summary = conversation_summary.into();
        block_on(async move {
            let request = AmpRecallRequest {
                query: summary,
                limit,
                scope: this.scope(),
            };
            let thoughts: Vec<AmpThought> = this.post("/v1/recall", &request).await?;
            Ok(thoughts
                .into_iter()
                .map(|t| AmpMemoryProvider::thought_to_entry(&this.endpoint.id, t, ""))
                .collect())
        })
    }

    fn tool_schemas(&self) -> Vec<serde_json::Value> {
        vec![
            amp_tool_schema(
                &format!("{}_recall", self.endpoint.id),
                "Recall relevant memories from the long-term memory backend.",
                &[ToolSchemaProperty::new(
                    "query",
                    ToolSchemaPropertyType::String,
                    "What to search for",
                    true,
                )],
            ),
            amp_tool_schema(
                &format!("{}_store", self.endpoint.id),
                "Store a fact, lesson, or decision in the long-term memory backend.",
                &[ToolSchemaProperty::new(
                    "content",
                    ToolSchemaPropertyType::String,
                    "Content to store",
                    true,
                )],
            ),
        ]
    }

    fn handle_tool_call(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<String, PersistenceError> {
        let mut this = self.clone();
        let recall_name = format!("{}_recall", self.endpoint.id);
        let store_name = format!("{}_store", self.endpoint.id);
        if tool_name == recall_name {
            let query = args["query"].as_str().map_or("", |s| s);
            let entries = this.recall(query, 8)?;
            let results: Vec<String> = entries.into_iter().map(|e| e.value).collect();
            return match serde_json::to_string(&results) {
                Ok(json) => Ok(json),
                Err(_) => Ok("[]".into()),
            };
        }
        if tool_name == store_name {
            let content = args["content"]
                .as_str()
                .map_or(String::new(), |s| s.to_string());
            this.set(random_key("tool"), content, "tool_store".into())?;
            return Ok("{\"status\":\"stored\"}".into());
        }
        Err(PersistenceError::NotFound(format!(
            "Unknown tool: {tool_name}"
        )))
    }
}

impl AmpMemoryProvider {
    fn thought_to_entry(
        provider_id: &str,
        thought: AmpThought,
        category_override: &str,
    ) -> MemoryEntry {
        let now = system_time_secs();
        let key = match thought.id {
            Some(id) => id,
            None => random_key("amp"),
        };
        let category = if category_override.is_empty() {
            match thought.thought_type {
                Some(t) => t,
                None => "memory".into(),
            }
        } else {
            category_override.into()
        };
        let created_at = match thought.created_at {
            Some(t) => t,
            None => now,
        };
        MemoryEntry {
            key,
            value: thought.content,
            category,
            created_at,
            updated_at: created_at,
            access_count: 0,
            provider_id: provider_id.into(),
        }
    }
}

fn amp_tool_schema(
    name: &str,
    description: &str,
    params: &[ToolSchemaProperty],
) -> serde_json::Value {
    openai_function_tool(name, description, params)
}

fn random_key(prefix: &str) -> String {
    format!(
        "{}-{}-{:08x}-{:08x}-{:08x}",
        prefix,
        system_time_secs(),
        rand::random::<u32>(),
        rand::random::<u32>(),
        rand::random::<u32>()
    )
}

/// Runs an async AMP operation on the blocking thread pool.
///
/// This helper bridges the synchronous `MemoryProvider` trait boundary to the
/// async reqwest client. It must be called from within a Tokio runtime.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) scheduling overhead plus the async operation duration.
///
/// # Panic / Safety
/// Never panics. Returns an error if no Tokio runtime is available.
///
/// # Cancel safety
/// Dropping the caller's future after this function returns does not cancel
/// the spawned blocking task; the HTTP request may run to completion. AMP
/// memory operations are short-lived idempotent reads/writes, so this is
/// acceptable.
fn block_on<T, F>(f: F) -> Result<T, PersistenceError>
where
    F: Future<Output = Result<T, PersistenceError>> + Send + 'static,
    T: Send + 'static,
{
    let _ = tokio::runtime::Handle::try_current().map_err(|_| PersistenceError::NoRuntime)?;
    let handle = tokio::task::spawn_blocking(move || tokio::runtime::Handle::current().block_on(f));
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            handle.await.map_err(|e| {
                PersistenceError::Other(format!("AMP memory provider blocking task failed: {e}"))
            })?
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amp_tool_schemas_use_typed_openai_wrapper()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let provider = AmpMemoryProvider::new(AmpMemoryEndpoint {
            id: "amp".into(),
            name: "AMP".into(),
            url: "https://memory.example".into(),
            api_key: None,
            scope: None,
        });

        let schemas = provider.tool_schemas();
        assert_eq!(schemas.len(), 2);
        let recall = schemas.first().ok_or("missing recall schema")?;
        let store = schemas.get(1).ok_or("missing store schema")?;
        assert_eq!(recall["type"], "function");
        assert_eq!(recall["function"]["name"], "amp_recall");
        assert_eq!(
            recall["function"]["parameters"]["required"],
            serde_json::Value::Array(vec![serde_json::Value::String("query".into())])
        );
        assert_eq!(store["function"]["name"], "amp_store");
        assert_eq!(
            store["function"]["parameters"]["properties"]["content"]["type"],
            "string"
        );
        Ok(())
    }
}
