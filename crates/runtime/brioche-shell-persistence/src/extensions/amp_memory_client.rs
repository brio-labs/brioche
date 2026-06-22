//! Generic AMP Core-compatible memory client.
//!
//! This provider talks to any backend that implements the Agent Memory Protocol
//! (AMP) Core verbs over HTTP: `amp.encode`, `amp.recall`, `amp.forget`, and
//! `amp.stats`. Configuring a new memory backend only requires an endpoint and
//! an API key; no new Rust code is needed.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::memory_provider::{MemoryEntry, MemoryProvider, MemoryQuery, MemorySessionContext};
use super::{ExtensionMetadata, PanelSlot};

/// Configuration for a generic AMP-compatible memory endpoint.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Struct containing heap-allocated configurations. O(1).
///
/// # Panic / Safety
/// Never panics.
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
///
/// # Complexity
/// Performs HTTP network transactions on list, set, recall, and delete operations.
/// All network futures time out after 30 seconds.
///
/// # Panic / Safety
/// Never panics. Returns standard Result wrappers.
#[derive(Clone, Debug, Default)]
pub struct AmpMemoryProvider {
    endpoint: AmpMemoryEndpoint,
    client: Option<reqwest::Client>,
    session_context: Option<MemorySessionContext>,
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
    ///
    /// # Complexity
    /// O(1) allocation.
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn new(endpoint: AmpMemoryEndpoint) -> Self {
        Self {
            endpoint,
            client: None,
            session_context: None,
        }
    }

    fn ensure_client(&mut self) -> Result<&reqwest::Client, String> {
        if self.client.is_none() {
            let mut builder = reqwest::Client::builder();
            builder = builder.timeout(std::time::Duration::from_secs(30));
            if let Some(api_key) = &self.endpoint.api_key {
                let mut headers = reqwest::header::HeaderMap::new();
                let value = match reqwest::header::HeaderValue::from_str(api_key) {
                    Ok(v) => v,
                    Err(_) => {
                        return Err("Invalid API key header value".into());
                    }
                };
                headers.insert("Authorization", value);
                builder = builder.default_headers(headers);
            }
            match builder.build() {
                Ok(client) => self.client = Some(client),
                Err(err) => return Err(format!("Failed to build HTTP client: {err}")),
            }
        }
        match self.client.as_ref() {
            Some(client) => Ok(client),
            None => Err("HTTP client not initialized".into()),
        }
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
        if let Some(ctx) = &self.session_context {
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

    async fn post<Req, Res>(&mut self, path: &str, body: &Req) -> Result<Res, String>
    where
        Req: Serialize,
        Res: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url(), path);
        let client = self.ensure_client()?;
        let response = client
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|err| format!("AMP request failed: {err}"))?;
        let status = response.status();
        let envelope: AmpResponse<Res> = response
            .json()
            .await
            .map_err(|err| format!("Failed to parse AMP response: {err}"))?;
        if let Some(err) = envelope.error {
            return Err(format!("AMP error ({}): {}", err.code, err.message));
        }
        match envelope.data {
            Some(data) => Ok(data),
            None => Err(format!("AMP returned empty data (HTTP {status})")),
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

    fn initialize(&mut self, ctx: MemorySessionContext) -> Result<(), String> {
        self.session_context = Some(ctx);
        Ok(())
    }

    fn list(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>, String> {
        // AMP Core does not have a generic list verb; use recall with an empty query.
        let runtime = match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle,
            Err(_) => {
                return Err(
                    "AMP memory provider must be called from within a Tokio runtime".into(),
                );
            }
        };

        let q = match query.query.as_ref() {
            Some(s) => s.clone(),
            None => String::new(),
        };
        let category = query.category.clone().map_or(String::new(), |c| c);
        let limit = 50usize;
        let mut this = self.clone();
        tokio::task::block_in_place(|| {
            runtime.block_on(async move {
                let request = AmpRecallRequest {
                    query: q,
                    limit,
                    scope: this.scope(),
                };
                let thoughts: Vec<AmpThought> = this.post("/v1/recall", &request).await?;
                Ok(thoughts
                    .into_iter()
                    .map(|t| AmpMemoryProvider::thought_to_entry(&this.endpoint.id, t, &category))
                    .collect())
            })
        })
    }

    fn set(&mut self, key: String, value: String, category: String) -> Result<(), String> {
        let runtime = match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle,
            Err(_) => {
                return Err(
                    "AMP memory provider must be called from within a Tokio runtime".into(),
                );
            }
        };

        let mut scope = self.scope();
        scope.insert("key".into(), key);
        let request = AmpEncodeRequest {
            content: value,
            thought_type: category,
            tags: Vec::new(),
            scope,
        };
        tokio::task::block_in_place(|| {
            runtime.block_on(async move {
                self.post::<_, serde_json::Value>("/v1/encode", &request)
                    .await
                    .map(|_| ())
            })
        })
    }

    fn delete(&mut self, key: &str) -> Result<bool, String> {
        let runtime = match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle,
            Err(_) => {
                return Err(
                    "AMP memory provider must be called from within a Tokio runtime".into(),
                );
            }
        };

        let request = AmpForgetRequest { key: key.into() };
        tokio::task::block_in_place(|| {
            runtime.block_on(async move {
                self.post::<_, serde_json::Value>("/v1/forget", &request)
                    .await
                    .map(|_| true)
            })
        })
    }

    fn recall(&self, conversation_summary: &str, limit: usize) -> Result<Vec<MemoryEntry>, String> {
        let runtime = match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle,
            Err(_) => {
                return Err(
                    "AMP memory provider must be called from within a Tokio runtime".into(),
                );
            }
        };

        let mut this = self.clone();
        let summary = conversation_summary.into();
        tokio::task::block_in_place(|| {
            runtime.block_on(async move {
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
        })
    }

    fn tool_schemas(&self) -> Vec<serde_json::Value> {
        vec![
            amp_tool_schema(
                &format!("{}_recall", self.endpoint.id),
                "Recall relevant memories from the long-term memory backend.",
                vec![("query", "string", "What to search for", true)],
            ),
            amp_tool_schema(
                &format!("{}_store", self.endpoint.id),
                "Store a fact, lesson, or decision in the long-term memory backend.",
                vec![("content", "string", "Content to store", true)],
            ),
        ]
    }

    fn handle_tool_call(&self, tool_name: &str, args: serde_json::Value) -> Result<String, String> {
        let mut this = self.clone();
        if tool_name == format!("{}_recall", self.endpoint.id) {
            let query = args["query"].as_str().map_or("", |s| s);
            let entries = this.recall(query, 8)?;
            let results: Vec<String> = entries.into_iter().map(|e| e.value).collect();
            match serde_json::to_string(&results) {
                Ok(json) => return Ok(json),
                Err(_) => return Ok("[]".into()),
            }
        }
        if tool_name == format!("{}_store", self.endpoint.id) {
            let content = args["content"]
                .as_str()
                .map_or(String::new(), |s| s.to_string());
            let key = format!(
                "tool-{}-{}-{}-{}-{}-{}-{}-{}-{}-{}",
                system_time_secs(),
                rand::random::<u32>(),
                rand::random::<u32>(),
                rand::random::<u32>(),
                rand::random::<u32>(),
                rand::random::<u32>(),
                rand::random::<u32>(),
                rand::random::<u32>(),
                rand::random::<u32>(),
                rand::random::<u32>()
            );
            this.set(key, content, "tool_store".into())?;
            return Ok("{\"status\":\"stored\"}".into());
        }
        Err(format!("Unknown tool: {}", tool_name))
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
            None => {
                format!(
                    "amp-{}-{}-{}-{}-{}-{}-{}-{}-{}-{}",
                    now,
                    rand::random::<u32>(),
                    rand::random::<u32>(),
                    rand::random::<u32>(),
                    rand::random::<u32>(),
                    rand::random::<u32>(),
                    rand::random::<u32>(),
                    rand::random::<u32>(),
                    rand::random::<u32>(),
                    rand::random::<u32>()
                )
            }
        };
        let category = if category_override.is_empty() {
            match thought.thought_type {
                Some(t) => t,
                None => "memory".into(),
            }
        } else {
            category_override.into()
        };
        MemoryEntry {
            key,
            value: thought.content,
            category,
            created_at: match thought.created_at {
                Some(t) => t,
                None => now,
            },
            updated_at: match thought.created_at {
                Some(t) => t,
                None => now,
            },
            access_count: 0,
            provider_id: provider_id.into(),
        }
    }
}

fn amp_tool_schema(
    name: &str,
    description: &str,
    params: Vec<(&str, &str, &str, bool)>,
) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for (param_name, param_type, param_desc, is_required) in params {
        let mut prop = serde_json::Map::new();
        prop.insert("type".into(), serde_json::Value::String(param_type.into()));
        prop.insert(
            "description".into(),
            serde_json::Value::String(param_desc.into()),
        );
        properties.insert(param_name.into(), serde_json::Value::Object(prop));
        if is_required {
            required.push(serde_json::Value::String(param_name.into()));
        }
    }

    let mut function = serde_json::Map::new();
    function.insert("name".into(), serde_json::Value::String(name.into()));
    function.insert(
        "description".into(),
        serde_json::Value::String(description.into()),
    );
    function.insert("parameters".into(), {
        let mut params_obj = serde_json::Map::new();
        params_obj.insert("type".into(), serde_json::Value::String("object".into()));
        params_obj.insert("properties".into(), serde_json::Value::Object(properties));
        if !required.is_empty() {
            params_obj.insert("required".into(), serde_json::Value::Array(required));
        }
        serde_json::Value::Object(params_obj)
    });

    let mut obj = serde_json::Map::new();
    obj.insert("type".into(), serde_json::Value::String("function".into()));
    obj.insert("function".into(), serde_json::Value::Object(function));
    serde_json::Value::Object(obj)
}

fn system_time_secs() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    }
}
