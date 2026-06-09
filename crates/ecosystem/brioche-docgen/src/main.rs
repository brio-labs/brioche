//! `brioche-docgen` — documentation generator for Brioche specs.
//!
//! Produces trait dependency graphs, sequence diagrams, and invariant
//! cross-references from the codebase.
//!
//! ## Usage
//! ```text
//! brioche-docgen trait-graph --format markdown --output docs/
//! brioche-docgen sequence-diagram --input EngineInput --output docs/
//! ```
//!
//! Refs: SPECS.md §Book V

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// CLI arguments.
#[derive(Parser)]
#[command(name = "brioche-docgen")]
#[command(about = "Generate Brioche architecture documentation from source")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output directory.
    #[arg(short, long, default_value = "docs/generated")]
    output: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a trait dependency graph.
    TraitGraph {
        /// Output format: markdown, html, json.
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },
    /// Generate sequence diagrams for EngineInput variants.
    SequenceDiagram {
        /// Input variant name (e.g. `UserMessage`).
        #[arg(short, long)]
        input: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::TraitGraph { format } => {
            generate_trait_graph(&cli.output, &format);
        }
        Commands::SequenceDiagram { input } => {
            generate_sequence_diagram(&cli.output, input.as_deref());
        }
    }
}

fn generate_trait_graph(output_dir: &PathBuf, format: &str) {
    let _ = fs::create_dir_all(output_dir);

    let graph = build_trait_graph();

    let content = match format {
        "json" => graph_to_json(&graph),
        "html" => graph_to_html(&graph),
        _ => graph_to_markdown(&graph),
    };

    let ext = match format {
        "json" => "json",
        "html" => "html",
        _ => "md",
    };

    let path = output_dir.join(format!("trait_graph.{ext}"));
    let mut file = fs::File::create(&path).unwrap_or_else(|_| {
        eprintln!("failed to create {}", path.display());
        std::process::exit(1);
    });
    file.write_all(content.as_bytes()).unwrap_or_else(|_| {
        eprintln!("failed to write {}", path.display());
        std::process::exit(1);
    });

    println!("Trait graph written to {}", path.display());
}

fn generate_sequence_diagram(output_dir: &PathBuf, variant: Option<&str>) {
    let _ = fs::create_dir_all(output_dir);

    let variants = match variant {
        Some(v) => vec![v.to_string()],
        None => vec![
            "UserMessage".into(),
            "LlmStream".into(),
            "ToolCallsResult".into(),
            "RestoreSubRoutine".into(),
        ],
    };

    for v in &variants {
        let diagram = build_sequence_diagram(v);
        let path = output_dir.join(format!("sequence_{v}.md"));
        let mut file = fs::File::create(&path).unwrap_or_else(|_| {
            eprintln!("failed to create {}", path.display());
            std::process::exit(1);
        });
        file.write_all(diagram.as_bytes()).unwrap_or_else(|_| {
            eprintln!("failed to write {}", path.display());
            std::process::exit(1);
        });
        println!("Sequence diagram written to {}", path.display());
    }
}

// ---------------------------------------------------------------------------
// Trait graph
// ---------------------------------------------------------------------------

#[derive(Debug, Default, serde::Serialize)]
struct TraitNode {
    name: String,
    methods: Vec<String>,
    supertraits: Vec<String>,
}

#[derive(Debug, Default, serde::Serialize)]
struct TraitGraph {
    nodes: Vec<TraitNode>,
    edges: Vec<(String, String)>, // (from, to)
}

fn build_trait_graph() -> TraitGraph {
    let mut graph = TraitGraph::default();

    // Hard-coded canonical trait graph derived from SPECS.md.
    // In a full implementation this would be extracted from source AST.
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

    // Governance trait dependencies (conceptual edges).
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

fn graph_to_markdown(graph: &TraitGraph) -> String {
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

fn graph_to_html(graph: &TraitGraph) -> String {
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

fn graph_to_json(graph: &TraitGraph) -> String {
    serde_json::to_string_pretty(graph).unwrap_or_else(|_| "{}".into())
}

// ---------------------------------------------------------------------------
// Sequence diagrams
// ---------------------------------------------------------------------------

fn build_sequence_diagram(variant: &str) -> String {
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
