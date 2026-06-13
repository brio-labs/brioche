//! Integration tests for the `brioche-docgen` CLI.

use std::fs;
use std::process::Command;

fn bin() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args(["run", "--package", "brioche-docgen", "--"]);
    cmd
}

fn run_docgen(args: &[&str], out: &std::path::Path) -> std::io::Result<()> {
    let status = bin().arg("--output").arg(out).args(args).status()?;
    assert!(status.success(), "docgen CLI exited with non-zero status");
    Ok(())
}

fn read_file(path: &std::path::Path) -> std::io::Result<String> {
    fs::read_to_string(path)
}

#[test]
fn trait_graph_markdown_is_written() -> std::io::Result<()> {
    let tmp = tempfile::tempdir()?;
    let out = tmp.path().join("generated");

    run_docgen(&["trait-graph", "--format", "markdown"], &out)?;

    let graph_path = out.join("trait_graph.md");
    assert!(graph_path.exists(), "trait_graph.md should exist");
    let content = read_file(&graph_path)?;
    assert!(content.contains("# Brioche Trait Dependency Graph"));
    assert!(content.contains("```mermaid"));
    Ok(())
}

#[test]
fn trait_graph_json_is_written() -> std::io::Result<()> {
    let tmp = tempfile::tempdir()?;
    let out = tmp.path().join("generated");

    run_docgen(&["trait-graph", "--format", "json"], &out)?;

    let graph_path = out.join("trait_graph.json");
    assert!(graph_path.exists(), "trait_graph.json should exist");
    let content = read_file(&graph_path)?;
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
        assert!(parsed.get("nodes").is_some());
        assert!(parsed.get("edges").is_some());
    }
    Ok(())
}

#[test]
fn sequence_diagrams_default_variants_are_written() -> std::io::Result<()> {
    let tmp = tempfile::tempdir()?;
    let out = tmp.path().join("generated");

    run_docgen(&["sequence-diagram"], &out)?;

    for variant in [
        "UserMessage",
        "LlmStream",
        "ToolCallsResult",
        "RestoreSubRoutine",
    ] {
        let path = out.join(format!("sequence_{variant}.md"));
        assert!(path.exists(), "{path:?} should exist");
        let content = read_file(&path)?;
        assert!(
            content.contains(&format!("# Sequence Diagram: `{variant}`")),
            "{variant} title missing"
        );
        assert!(content.contains("sequenceDiagram"));
    }
    Ok(())
}

#[test]
fn sequence_diagram_single_variant_is_written() -> std::io::Result<()> {
    let tmp = tempfile::tempdir()?;
    let out = tmp.path().join("generated");

    run_docgen(&["sequence-diagram", "--input", "UserMessage"], &out)?;

    let path = out.join("sequence_UserMessage.md");
    assert!(path.exists());
    let content = read_file(&path)?;
    assert!(content.contains("EngineInput::UserMessage"));
    Ok(())
}
