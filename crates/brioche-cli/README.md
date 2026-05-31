# brioche-cli

Interactive REPL for the Brioche kernel, powered by [`reedline`](https://github.com/nushell/reedline).

## Usage

```bash
cargo run --bin brioche-cli
```

## How it works

The CLI wires a `BriocheEngine` with the `Permissive` governance profile and lets you drive it manually:

1. **Type a message** — it becomes an `EngineInput::UserMessage`.
2. **Watch effects** — the kernel returns `Effect`s (e.g. `CallLlmNetwork`, `SaveSession`).
3. **Play the LLM** — when `CallLlmNetwork` appears, enter the assistant response:
   * plain text → `StreamEvent::TextChunk` + `Done`
   * `/tool <name> <args>` → simulated tool call
4. **Play the tools** — when `ExecuteTools` appears, enter each tool result.
5. **Inspect state** — use `/state` or `/history` anytime.

This is the simplest way to walk through the full transition cycle (Idle → Predicting → ExecutingTools → Idle) without a real network backend.

## Commands

| Command | Description |
|---------|-------------|
| `<text>` | Send a `UserMessage` to the engine |
| `/state` | Show current `AgentState`, stack depth, history length |
| `/history` | Print the conversation history |
| `/help` | Show help |
| `/quit` | Exit |
