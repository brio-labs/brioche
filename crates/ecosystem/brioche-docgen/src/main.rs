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
//! Refs: SPECS.md §Book IV Ch 3 §3.3

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use brioche_docgen::{
    SEQUENCE_DIAGRAM_VARIANTS, TraitGraphFormat, build_sequence_diagram, build_trait_graph,
    render_trait_graph,
};
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
    let fmt = TraitGraphFormat::parse(format);
    let content = render_trait_graph(&graph, fmt);

    let path = output_dir.join(format!("trait_graph.{}", fmt.extension()));
    let mut file = match fs::File::create(&path) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("failed to create {}", path.display());
            std::process::exit(1);
        }
    };
    if file.write_all(content.as_bytes()).is_err() {
        eprintln!("failed to write {}", path.display());
        std::process::exit(1);
    }

    println!("Trait graph written to {}", path.display());
}

fn generate_sequence_diagram(output_dir: &PathBuf, variant: Option<&str>) {
    let _ = fs::create_dir_all(output_dir);

    let variants: Vec<&str> = match variant {
        Some(v) => vec![v],
        None => SEQUENCE_DIAGRAM_VARIANTS.to_vec(),
    };

    for v in &variants {
        let diagram = build_sequence_diagram(v);
        let path = output_dir.join(format!("sequence_{v}.md"));
        let mut file = match fs::File::create(&path) {
            Ok(f) => f,
            Err(_) => {
                eprintln!("failed to create {}", path.display());
                std::process::exit(1);
            }
        };
        if file.write_all(diagram.as_bytes()).is_err() {
            eprintln!("failed to write {}", path.display());
            std::process::exit(1);
        }
        println!("Sequence diagram written to {}", path.display());
    }
}
