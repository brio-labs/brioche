//! Implémentation `LlmClient` pour les endpoints compatibles OpenAI.
//!
//! Le client :
//! 1. Construit la requête JSON à partir de l'historique Brioche.
//! 2. Ouvre une connexion SSE via `reqwest`.
//! 3. Parse chaque ligne SSE en `delta.content` ou `tool_calls`.
//! 4. Segmente les fragments selon `MAX_INLINE_CHUNK`.
//! 5. Envoie chaque fragment au kernel via `shell.send_input(LlmStream(...))`.
//! 6. Diffuse simultanément les chunks sur un canal `broadcast::Sender<LlmChunk>`
//!    pour que la projection (CLI) puisse les afficher en temps réel.
//!
//! # Invariants
//! - I-Core-ChunkBudget : tout fragment > 4096 octets est segmenté.
//! - I-Shell-Network-Signal : en cas d'erreur, `SystemSignal::NetworkUnavailable`
//!   est émis via le shell.
//!
//! Refs: SPECS.md §Book III-A, I-Core-ChunkBudget

use crate::{config::OpenAiConfig, request::build_request_body, sse::SseParser};
use brioche_core::{ChatMessage, MAX_INLINE_CHUNK, StreamEvent, ToolResultDTO};
use brioche_shell_runtime::{BriocheShell, EngineInput, LlmClient, ShellError, SystemSignal};
use bytes::Bytes;
use futures_util::StreamExt;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

/// Chunk diffusé vers la projection (CLI, GUI…). Le kernel ne le voit jamais.
///
/// Refs: I-Shell-Projection-Independent
#[derive(Clone, Debug)]
pub enum LlmChunk {
    Text(String),
    ToolCallStart { id: String, name: String },
    ToolArgument { id: String, fragment: String },
    ToolCallDone { id: String },
    ToolResult { name: String, output: String },
    Done,
    Error(String),
}

/// Client LLM compatible OpenAI.
///
/// `tools_schema` est mis à jour dynamiquement par l'assembleur (CLI)
/// quand le registry d'outils change. `Arc<RwLock>` permet une mise à
/// jour sans reconstruction du client.
///
/// # Usage
/// ```ignore
/// let (client, llm_rx) = OpenAiLlmClient::new(config);
/// client.set_tools_schema(schemas).await;
/// // client est injecté dans DefaultEffectExecutor.
/// ```
/// Miroir de l'historique partagé entre le CLI et le client LLM.
///
/// Le CLI pousse les `UserMessage` ; le client pousse les `Assistant`
/// et `ToolResult` au fil du stream.
pub type SharedHistory = Arc<RwLock<Vec<ChatMessage>>>;

pub struct OpenAiLlmClient {
    config: OpenAiConfig,
    http: reqwest::Client,
    tools_schema: Arc<RwLock<Vec<serde_json::Value>>>,
    ui_tx: broadcast::Sender<LlmChunk>,
    /// Miroir de l'historique conversationnel.
    history: SharedHistory,
    /// Buffer local pour accumuler le texte assistant du stream courant.
    pending_text: tokio::sync::Mutex<String>,
}

impl Clone for OpenAiLlmClient {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http: self.http.clone(),
            tools_schema: Arc::clone(&self.tools_schema),
            ui_tx: self.ui_tx.clone(),
            history: Arc::clone(&self.history),
            pending_text: tokio::sync::Mutex::new(String::new()),
        }
    }
}

impl OpenAiLlmClient {
    /// Crée un nouveau client et retourne le receiver de broadcast.
    ///
    /// Le canal de broadcast a une capacité de 256 messages. Les
    /// récepteurs lents peuvent perdre des messages anciens.
    ///
    /// # Panics
    /// Ne panique jamais. `api_key` vide est accepté (certains endpoints
    /// locaux comme Ollama ne nécessitent pas de clé).
    pub fn new(config: OpenAiConfig) -> (Self, broadcast::Receiver<LlmChunk>, SharedHistory) {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let (ui_tx, ui_rx) = broadcast::channel(256);
        let history: SharedHistory = Arc::new(RwLock::new(Vec::new()));

        let client = Self {
            config,
            http,
            tools_schema: Arc::new(RwLock::new(Vec::new())),
            ui_tx,
            history: Arc::clone(&history),
            pending_text: tokio::sync::Mutex::new(String::new()),
        };

        (client, ui_rx, history)
    }

    /// S'abonne au canal de broadcast de chunks LLM.
    ///
    /// Chaque appel retourne un nouveau récepteur indépendant.
    pub fn subscribe(&self) -> broadcast::Receiver<LlmChunk> {
        self.ui_tx.subscribe()
    }

    /// Pousse un message dans le miroir d'historique.
    ///
    /// Le CLI appelle cette méthode avant d'envoyer un `UserMessage`
    /// au shell, garantissant que le client LLM voit l'historique complet.
    pub async fn push_message(&self, message: ChatMessage) {
        self.history.write().await.push(message);
    }

    /// Met à jour la liste des outils disponibles sans reconstruire le client.
    ///
    /// Cette liste est lue au début de chaque appel `call_llm()`.
    pub async fn set_tools_schema(&self, schemas: Vec<serde_json::Value>) {
        let mut guard = self.tools_schema.write().await;
        *guard = schemas;
    }

    /// Segment un payload `Bytes` selon `MAX_INLINE_CHUNK`.
    ///
    /// Refs: I-Core-ChunkBudget
    fn segment_bytes(bytes: Bytes, max_chunk: usize) -> Vec<Bytes> {
        if bytes.len() <= max_chunk {
            return vec![bytes];
        }
        let mut fragments = Vec::with_capacity(bytes.len().div_ceil(max_chunk));
        let mut offset = 0;
        while offset < bytes.len() {
            let end = (offset + max_chunk).min(bytes.len());
            fragments.push(bytes.slice(offset..end));
            offset = end;
        }
        fragments
    }

    /// Émet un text chunk vers le kernel et la projection.
    async fn emit_text_chunk(&self, shell: &BriocheShell, text: &str) -> Result<(), ShellError> {
        {
            let mut pending = self.pending_text.lock().await;
            pending.push_str(text);
        }
        let bytes = Bytes::from(text.to_string());
        for chunk in Self::segment_bytes(bytes, MAX_INLINE_CHUNK) {
            shell
                .send_input(EngineInput::LlmStream(StreamEvent::TextChunk {
                    path: Default::default(),
                    chunk,
                }))
                .await?;
        }
        let _ = self.ui_tx.send(LlmChunk::Text(text.to_string()));
        Ok(())
    }

    /// Émet un tool call event vers le kernel et la projection.
    async fn emit_tool_call_start(
        &self,
        shell: &BriocheShell,
        id: &str,
        name: &str,
    ) -> Result<(), ShellError> {
        shell
            .send_input(EngineInput::LlmStream(StreamEvent::ToolCallStart {
                path: Default::default(),
                id: id.to_string(),
                name: name.to_string(),
            }))
            .await?;
        let _ = self.ui_tx.send(LlmChunk::ToolCallStart {
            id: id.to_string(),
            name: name.to_string(),
        });
        Ok(())
    }

    /// Émet un fragment d'argument de tool call.
    async fn emit_tool_argument(
        &self,
        shell: &BriocheShell,
        id: &str,
        fragment: &str,
    ) -> Result<(), ShellError> {
        let bytes = Bytes::from(fragment.to_string());
        for chunk in Self::segment_bytes(bytes, MAX_INLINE_CHUNK) {
            shell
                .send_input(EngineInput::LlmStream(StreamEvent::ToolArgumentChunk {
                    path: Default::default(),
                    id: id.to_string(),
                    chunk,
                }))
                .await?;
        }
        let _ = self.ui_tx.send(LlmChunk::ToolArgument {
            id: id.to_string(),
            fragment: fragment.to_string(),
        });
        Ok(())
    }

    /// Émet le marqueur de fin de tool call.
    async fn emit_tool_call_done(&self, shell: &BriocheShell, id: &str) -> Result<(), ShellError> {
        shell
            .send_input(EngineInput::LlmStream(StreamEvent::ToolCallDone {
                path: Default::default(),
            }))
            .await?;
        let _ = self
            .ui_tx
            .send(LlmChunk::ToolCallDone { id: id.to_string() });
        Ok(())
    }

    /// Diffuse un résultat d'outil vers la projection (CLI).
    ///
    /// Appelé par `NotifyingToolExecutor` après exécution.
    pub fn emit_tool_result(&self, name: &str, output: &str) {
        let _ = self.ui_tx.send(LlmChunk::ToolResult {
            name: name.to_string(),
            output: output.to_string(),
        });
    }

    /// Pousse les résultats d'outils dans le miroir d'historique.
    ///
    /// Le CLI (via un wrapper EffectExecutor) appelle cette méthode
    /// après avoir exécuté les outils, garantissant que le prochain
    /// appel `call_llm()` voit les résultats dans l'historique.
    pub async fn push_tool_results(&self, results: &[ToolResultDTO]) {
        let mut history = self.history.write().await;
        for result in results {
            history.push(ChatMessage::ToolResult {
                id: result.tool_id.clone(),
                tool_name: result.tool_name.clone(),
                outcome: result.outcome.clone(),
            });
        }
    }
}

/// Accumulateur interne pour un tool call en cours de réception SSE.
#[derive(Clone, Debug, Default)]
struct ToolCallAccumulator {
    id: String,
    name: String,
    args: String,
    start_emitted: bool,
}

#[async_trait::async_trait]
impl LlmClient for OpenAiLlmClient {
    async fn call_llm(&self, shell: &BriocheShell) -> Result<(), ShellError> {
        let url = format!("{}/chat/completions", self.config.base_url);

        let history_guard = self.history.read().await;
        let messages = crate::request::build_messages(&history_guard);
        drop(history_guard);

        let tools_guard = self.tools_schema.read().await;
        let tools: Option<&[serde_json::Value]> = if tools_guard.is_empty() {
            None
        } else {
            Some(&*tools_guard)
        };

        let body = build_request_body(&self.config.model, messages, self.config.max_tokens, tools);
        drop(tools_guard);

        let request = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body);

        let response = match request.send().await {
            Ok(r) => r,
            Err(err) => {
                let msg = format!("Erreur réseau: {err}");
                tracing::error!(error = %err, "OpenAI request failed");
                let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
                shell
                    .send_system_signal(SystemSignal::NetworkUnavailable { reason: msg })
                    .await?;
                return Ok(());
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            // Extraire un message compact du JSON d'erreur OpenAI.
            let compact = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_text) {
                json.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or(&body_text)
                    .to_string()
            } else {
                body_text.lines().next().unwrap_or(&body_text).to_string()
            };
            let msg = format!("HTTP {status}: {compact}");
            tracing::error!(status = %status, body = %body_text, "OpenAI error response");
            let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
            shell
                .send_system_signal(SystemSignal::NetworkUnavailable { reason: msg })
                .await?;
            return Ok(());
        }

        let mut stream = response.bytes_stream();
        let mut parser = SseParser::new();

        // Accumulateur de tool calls par index OpenAI.
        let mut tool_acc: BTreeMap<usize, ToolCallAccumulator> = BTreeMap::new();
        let mut finish_reason: Option<String> = None;

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(err) => {
                    let msg = format!("Erreur SSE: {err}");
                    tracing::error!(error = %err, "SSE stream error");
                    let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
                    shell
                        .send_system_signal(SystemSignal::NetworkUnavailable { reason: msg })
                        .await?;
                    return Ok(());
                }
            };

            for event in parser.feed(&chunk) {
                let Some(choices) = event.get("choices").and_then(|c| c.as_array()) else {
                    continue;
                };

                for choice in choices {
                    let delta = choice.get("delta");
                    let finish = choice
                        .get("finish_reason")
                        .and_then(|f| f.as_str())
                        .map(|s| s.to_string());
                    if finish.is_some() {
                        finish_reason = finish;
                    }

                    // Text chunk
                    if let Some(content) = delta
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                        && !content.is_empty()
                    {
                        self.emit_text_chunk(shell, content).await?;
                    }

                    // Tool calls (deltas partiels)
                    if let Some(tool_calls) = delta
                        .and_then(|d| d.get("tool_calls"))
                        .and_then(|t| t.as_array())
                    {
                        for (idx, tc) in tool_calls.iter().enumerate() {
                            let entry = tool_acc.entry(idx).or_default();

                            if let Some(id) = tc.get("id").and_then(|i| i.as_str())
                                && !id.is_empty()
                            {
                                entry.id = id.to_string();
                            }
                            let mut arg_fragment = String::new();
                            if let Some(func) = tc.get("function") {
                                if let Some(name) = func.get("name").and_then(|n| n.as_str())
                                    && !name.is_empty()
                                {
                                    entry.name = name.to_string();
                                }
                                if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                                    entry.args.push_str(args);
                                    arg_fragment = args.to_string();
                                }
                            }

                            // Émettre ToolCallStart dès qu'on a id + name.
                            if !entry.id.is_empty()
                                && !entry.name.is_empty()
                                && !entry.start_emitted
                            {
                                self.emit_tool_call_start(shell, &entry.id, &entry.name)
                                    .await?;
                                entry.start_emitted = true;

                                // Ajouter le ToolRequest à l'historique pour
                                // que les appels suivants voient la demande.
                                self.history.write().await.push(ChatMessage::ToolRequest {
                                    id: entry.id.clone(),
                                    name: entry.name.clone(),
                                    arguments: String::new(),
                                });
                            }

                            // Émettre le fragment d'arguments (seulement le delta,
                            // pas l'accumulation complète — le kernel fait push_str).
                            if entry.start_emitted && !arg_fragment.is_empty() {
                                self.emit_tool_argument(shell, &entry.id, &arg_fragment)
                                    .await?;
                            }
                        }
                    }
                }
            }
        }

        // Si finish_reason est "tool_calls", tous les tool calls sont complets.
        // On émet un seul ToolCallDone (le kernel draine tous les pending).
        if finish_reason.as_deref() == Some("tool_calls")
            && let Some(first) = tool_acc.values().next()
        {
            self.emit_tool_call_done(shell, &first.id).await?;
        }

        // Marquer la fin du stream et persister le texte assistant.
        {
            let mut pending = self.pending_text.lock().await;
            let text = if !pending.is_empty() {
                Some(std::mem::take(&mut *pending))
            } else {
                None
            };
            drop(pending);
            if let Some(text) = text {
                self.history
                    .write()
                    .await
                    .push(ChatMessage::Assistant { content: text });
            }
        }

        shell
            .send_input(EngineInput::LlmStream(StreamEvent::Done))
            .await?;
        let _ = self.ui_tx.send(LlmChunk::Done);
        Ok(())
    }
}
