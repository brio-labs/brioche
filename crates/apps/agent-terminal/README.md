# agent-terminal

Minimal terminal agent for the Brioche kernel, powered by [`reedline`](https://github.com/nushell/reedline).

## Usage

```bash
cargo run --bin agent-terminal
```

## How it works

The agent wires a `BriocheEngine` with the standard governance profile,
an OpenAI-compatible LLM client, and basic system tools (file read/write,
shell execution, directory listing, URL fetch).

1. **Type a message** — it becomes an `EngineInput::UserMessage`.
2. **LLM streaming** — assistant responses stream back in real time.
3. **Tool execution** — the agent can read files, run shell commands,
   list directories, and fetch URLs (with confirmation for unlisted commands).
4. **Session persistence** — conversation history is saved to Redb.

## Commands

| Command | Description |
|---------|-------------|
| `<text>` | Send a message to the LLM |
| `/help` | Show help |
| `/quit` | Exit |
| `/session` | Current session |
| `/session new` | Create a new session |
| `/session list` | List sessions |
| `/session load <id>` | Load a persisted session |
