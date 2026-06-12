//! Integration tests for the `brioche-docgen` CLI.

use std::fs;
use std::process::Command;

fn bin() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args(["run", "--package", "brioche-docgen", "--"]);
    cmd
}

fn tempdir() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => {
            let _msg = format!("tempdir creation should succeed: {e}");
            unreachable!()
        }
    }
}

fn run_docgen(args: &[&str], out: &std::path::Path) {
    let status = match bin().arg("--output").arg(out).args(args).status() {
        Ok(s) => s,
        Err(e) => {
            let _msg = format!("docgen CLI should spawn: {e}");
            unreachable!()
        }
    };
    assert!(status.success(), "docgen CLI exited with non-zero status");
}

fn read_file(path: &std::path::Path) -> String {
    match fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            let _msg = format!("read_file should succeed: {e}");
            unreachable!()
        }
    }
}

#[test]
fn trait_graph_markdown_is_written() {
    let tmp = tempdir();
    let out = tmp.path().join("generated");

    run_docgen(&["trait-graph", "--format", "markdown"], &out);

    let graph_path = out.join("trait_graph.md");
    assert!(graph_path.exists(), "trait_graph.md should exist");
    let content = read_file(&graph_path);
    assert!(content.contains("# Brioche Trait Dependency Graph"));
    assert!(content.contains("```mermaid"));
}

#[test]
fn trait_graph_json_is_written() {
    let tmp = tempdir();
    let out = tmp.path().join("generated");

    run_docgen(&["trait-graph", "--format", "json"], &out);

    let graph_path = out.join("trait_graph.json");
    assert!(graph_path.exists(), "trait_graph.json should exist");
    let content = read_file(&graph_path);
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
        assert!(parsed.get("nodes").is_some());
        assert!(parsed.get("edges").is_some());
    }
}

#[test]
fn sequence_diagrams_default_variants_are_written() {
    let tmp = tempdir();
    let out = tmp.path().join("generated");

    run_docgen(&["sequence-diagram"], &out);

    for variant in [
        "UserMessage",
        "LlmStream",
        "ToolCallsResult",
        "RestoreSubRoutine",
    ] {
        let path = out.join(format!("sequence_{variant}.md"));
        assert!(path.exists(), "{path:?} should exist");
        let content = read_file(&path);
        assert!(
            content.contains(&format!("# Sequence Diagram: `{variant}`")),
            "{variant} title missing"
        );
        assert!(content.contains("sequenceDiagram"));
    }
}

#[test]
fn sequence_diagram_single_variant_is_written() {
    let tmp = tempdir();
    let out = tmp.path().join("generated");

    run_docgen(&["sequence-diagram", "--input", "UserMessage"], &out);

    let path = out.join("sequence_UserMessage.md");
    assert!(path.exists());
    let content = read_file(&path);
    assert!(content.contains("EngineInput::UserMessage"));
}
