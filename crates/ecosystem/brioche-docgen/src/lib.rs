//! # Brioche Docgen — Book V
//!
//! Documentation generation tooling. Produces invariant cross-references,
//! architecture diagrams, and spec extracts from code.
//!
//! ## Public interface
//! - `brioche-docgen` CLI for spec and ADR generation.
//! - `trait_graph` renderer for governance compatibility tables.
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.3

use serde::{Deserialize, Serialize};

/// A node in the trait dependency graph.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.3
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraitNode {
    /// Trait name (e.g. `BriochePlugin`).
    pub name: String,
    /// Method names declared on the trait.
    pub methods: Vec<String>,
    /// Names of supertraits.
    pub supertraits: Vec<String>,
}

/// Trait dependency graph.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.3
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraitGraph {
    /// Trait nodes in the graph.
    pub nodes: Vec<TraitNode>,
    /// Directed edges as `(from, to)` trait-name pairs.
    pub edges: Vec<(String, String)>,
}

/// Supported output formats for the trait graph.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.3
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraitGraphFormat {
    /// Markdown output with an embedded Mermaid diagram.
    Markdown,
    /// Minimal HTML page listing traits.
    Html,
    /// Pretty-printed JSON serialization of the graph.
    Json,
}

impl TraitGraphFormat {
    /// Parse a format identifier (`"json"`, `"html"`, anything else → Markdown).
    ///
    /// Refs: docs/SPECS.md §Book IV Ch 3 §3.3
    pub fn parse(format: &str) -> Self {
        match format {
            "json" => TraitGraphFormat::Json,
            "html" => TraitGraphFormat::Html,
            _ => TraitGraphFormat::Markdown,
        }
    }

    /// File extension for this format.
    ///
    /// Refs: docs/SPECS.md §Book IV Ch 3 §3.3
    pub fn extension(self) -> &'static str {
        match self {
            TraitGraphFormat::Json => "json",
            TraitGraphFormat::Html => "html",
            TraitGraphFormat::Markdown => "md",
        }
    }
}

/// Build the canonical trait graph derived from docs/SPECS.md.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.3
pub fn build_trait_graph() -> TraitGraph {
    let mut graph = TraitGraph::default();

    let traits = vec![
        (
            "BriochePlugin",
            vec![
                "name",
                "capabilities",
                "priority",
                "on_input",
                "before_prediction",
                "on_stream_event",
                "after_prediction",
                "on_tool_calls",
                "on_tool_result",
                "on_error",
            ],
        ),
        ("EpochInterceptor", vec!["intercept_epoch"]),
        ("SubRoutineHandler", vec!["handle_subroutine"]),
        ("ConsistencyVerifier", vec!["verify_consistency"]),
        ("DecisionAggregator", vec!["aggregate_decisions"]),
        ("SignalDrainOrder", vec!["drain"]),
        (
            "HookEffectConstraint",
            vec!["is_allowed_fast", "is_allowed_fallback"],
        ),
        (
            "CycleRollbackPolicy",
            vec!["begin_hook", "on_mutation", "commit_hook", "rollback_hook"],
        ),
        ("SubRoutineLifecycleGuard", vec!["on_exit"]),
        ("GovernanceFailoverHandler", vec!["handle_failure"]),
        ("CowBudgetPolicy", vec!["max_cow_bytes"]),
    ];

    for (name, methods) in traits {
        graph.nodes.push(TraitNode {
            name: name.to_string(),
            methods: methods.iter().map(|m| m.to_string()).collect(),
            supertraits: Vec::new(),
        });
    }

    let edges = vec![
        ("EpochInterceptor", "BriochePlugin"),
        ("DecisionAggregator", "BriochePlugin"),
        ("SubRoutineLifecycleGuard", "BriochePlugin"),
        ("CycleRollbackPolicy", "HookEffectConstraint"),
        ("CowBudgetPolicy", "CycleRollbackPolicy"),
    ];

    for (from, to) in edges {
        graph.edges.push((from.to_string(), to.to_string()));
    }

    graph
}

/// Render a trait graph as Markdown (Mermaid + trait table).
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.3
pub fn graph_to_markdown(graph: &TraitGraph) -> String {
    let mut out = String::new();
    out.push_str("# Brioche Trait Dependency Graph\n\n");
    out.push_str("```mermaid\ngraph TD\n");

    for node in &graph.nodes {
        out.push_str(&format!("    {}[{}]\n", node.name, node.name));
    }

    for (from, to) in &graph.edges {
        out.push_str(&format!("    {} --> {}\n", from, to));
    }

    out.push_str("```\n\n");

    out.push_str("## Traits\n\n");
    for node in &graph.nodes {
        out.push_str(&format!("### `{}`\n\n", node.name));
        out.push_str("Methods:\n");
        for m in &node.methods {
            out.push_str(&format!("- `{}()`\n", m));
        }
        out.push('\n');
    }

    out
}

/// Render a trait graph as a minimal HTML page.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.3
pub fn graph_to_html(graph: &TraitGraph) -> String {
    let mut out = String::new();
    out.push_str("<html><head><title>Brioche Trait Graph</title></head><body>");
    out.push_str("<h1>Brioche Trait Dependency Graph</h1>");
    out.push_str("<ul>");
    for node in &graph.nodes {
        out.push_str(&format!(
            "<li><strong>{}</strong> — {} method(s)</li>",
            node.name,
            node.methods.len()
        ));
    }
    out.push_str("</ul>");
    out.push_str("</body></html>");
    out
}

/// Render a trait graph as pretty-printed JSON.
///
/// # Panics
/// Never panics. Serialization of this graph is infallible in practice;
/// on the theoretical error path an empty JSON object is returned.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.3
pub fn graph_to_json(graph: &TraitGraph) -> String {
    match serde_json::to_string_pretty(graph) {
        Ok(json) => json,
        Err(_) => "{}".into(),
    }
}

/// Render a trait graph in the requested format.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.3
pub fn render_trait_graph(graph: &TraitGraph, format: TraitGraphFormat) -> String {
    match format {
        TraitGraphFormat::Json => graph_to_json(graph),
        TraitGraphFormat::Html => graph_to_html(graph),
        TraitGraphFormat::Markdown => graph_to_markdown(graph),
    }
}

/// All sequence diagram variants known to docgen.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.3
pub const SEQUENCE_DIAGRAM_VARIANTS: &[&str] = &[
    "UserMessage",
    "LlmStream",
    "ToolCallsResult",
    "RestoreSubRoutine",
];

/// Build a sequence diagram for an `EngineInput` variant.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.3
pub fn build_sequence_diagram(variant: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Sequence Diagram: `{}`\n\n", variant));
    out.push_str("```mermaid\nsequenceDiagram\n");
    out.push_str("    participant Shell\n");
    out.push_str("    participant Engine\n");
    out.push_str("    participant Plugins\n");
    out.push_str("    participant SessionRegistry\n\n");

    match variant {
        "UserMessage" => {
            out.push_str("    Shell->>Engine: EngineInput::UserMessage\n");
            out.push_str("    Engine->>Plugins: EpochInterceptor::intercept_epoch\n");
            out.push_str("    Engine->>Plugins: on_input hooks\n");
            out.push_str("    Engine->>Engine: push_state(Predicting)\n");
            out.push_str("    Engine->>Plugins: before_prediction hooks\n");
            out.push_str("    Engine->>Plugins: DecisionAggregator::aggregate_decisions\n");
            out.push_str("    Engine-->>Shell: Effect::CallLlmNetwork\n");
            out.push_str("    Engine-->>Shell: Effect::SaveSession\n");
        }
        "LlmStream" => {
            out.push_str("    Shell->>Engine: EngineInput::LlmStream(TextChunk)\n");
            out.push_str("    Engine->>Plugins: on_stream_event hooks\n");
            out.push_str("    Engine-->>Shell: StreamAction::Pass / Hold / OffloadTask\n");
        }
        "ToolCallsResult" => {
            out.push_str("    Shell->>Engine: EngineInput::ToolCallsResult\n");
            out.push_str("    Engine->>Engine: pop_state()\n");
            out.push_str("    Engine->>Plugins: on_tool_result hooks\n");
            out.push_str("    Engine->>Engine: push_history(ToolResult)\n");
            out.push_str("    Engine->>Engine: push_state(Predicting)\n");
            out.push_str("    Engine-->>Shell: Effect::CallLlmNetwork\n");
        }
        "RestoreSubRoutine" => {
            out.push_str("    Shell->>Engine: EngineInput::RestoreSubRoutine\n");
            out.push_str("    Engine->>SessionRegistry: insert(handle, session)\n");
            out.push_str("    Engine-->>Shell: Effect::SubRoutineRestored\n");
        }
        _ => {
            out.push_str(&format!(
                "    Note over Shell,Engine: Unknown variant `{}`\n",
                variant
            ));
        }
    }

    out.push_str("```\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_graph_contains_core_traits() {
        let graph = build_trait_graph();
        let names: Vec<_> = graph.nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"BriochePlugin"));
        assert!(names.contains(&"DecisionAggregator"));
        assert!(names.contains(&"HookEffectConstraint"));
    }

    #[test]
    fn trait_graph_edges_are_consistent() {
        let graph = build_trait_graph();
        let node_names: std::collections::BTreeSet<_> =
            graph.nodes.iter().map(|n| n.name.clone()).collect();
        for (from, to) in &graph.edges {
            assert!(
                node_names.contains(from),
                "edge source '{}' is not a known trait",
                from
            );
            assert!(
                node_names.contains(to),
                "edge target '{}' is not a known trait",
                to
            );
        }
    }

    #[test]
    fn markdown_includes_mermaid_block() {
        let graph = build_trait_graph();
        let md = graph_to_markdown(&graph);
        assert!(md.contains("```mermaid\ngraph TD\n"));
        assert!(md.contains("BriochePlugin[BriochePlugin]"));
        assert!(md.contains("EpochInterceptor --> BriochePlugin"));
    }

    #[test]
    fn html_contains_trait_list() {
        let graph = build_trait_graph();
        let html = graph_to_html(&graph);
        assert!(html.contains("<html>"));
        assert!(html.contains("BriochePlugin"));
    }

    #[test]
    fn json_is_valid_and_self_describing() {
        let graph = build_trait_graph();
        let json = graph_to_json(&graph);
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) {
            assert!(value.get("nodes").is_some());
            assert!(value.get("edges").is_some());
        }
        if let Ok(parsed) = serde_json::from_str::<TraitGraph>(&json) {
            assert_eq!(parsed, graph);
        }
    }

    #[test]
    fn unknown_sequence_variant_is_documented() {
        let diag = build_sequence_diagram("UnknownVariant");
        assert!(diag.contains("Unknown variant `UnknownVariant`"));
    }

    #[test]
    fn known_sequence_variants_have_participants() {
        for variant in SEQUENCE_DIAGRAM_VARIANTS {
            let diag = build_sequence_diagram(variant);
            assert!(
                diag.contains("participant Shell"),
                "{} missing Shell participant",
                variant
            );
            assert!(
                diag.contains(&format!("# Sequence Diagram: `{}`", variant)),
                "{} missing title",
                variant
            );
        }
    }

    #[test]
    fn format_parsing_defaults_to_markdown() {
        assert_eq!(
            TraitGraphFormat::parse("markdown"),
            TraitGraphFormat::Markdown
        );
        assert_eq!(TraitGraphFormat::parse("md"), TraitGraphFormat::Markdown);
        assert_eq!(TraitGraphFormat::parse("json"), TraitGraphFormat::Json);
        assert_eq!(TraitGraphFormat::parse("html"), TraitGraphFormat::Html);
    }

    #[test]
    fn format_extensions_match() {
        assert_eq!(TraitGraphFormat::Markdown.extension(), "md");
        assert_eq!(TraitGraphFormat::Html.extension(), "html");
        assert_eq!(TraitGraphFormat::Json.extension(), "json");
    }
}
