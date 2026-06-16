# Brioche Desktop 0.1

Tauri v2 desktop GUI for Brioche.

## Quick Start

### Release mode (uses static frontend, no dev server needed)

```bash
# Build frontend
cd crates/apps/brioche-desktop/frontend
npm run build

# Build and run release binary
cargo run -p brioche-desktop --release
```

### Development mode (requires frontend dev server)

Terminal 1:

```bash
cd crates/apps/brioche-desktop/frontend
npm run dev
```

Terminal 2:

```bash
cargo run -p brioche-desktop
```

## Environment Variables

- `BRIOCHE_API_KEY` — OpenAI API key
- `BRIOCHE_MODEL` — defaults to `qwen/qwen3.7-plus`
- `BRIOCHE_BASE_URL` — defaults to `https://openrouter.ai/api/v1`
- `BRIOCHE_MAX_TOKENS` — defaults to `4096`
- `BRIOCHE_REASONING_EFFORT` — optional

## Architecture

- **Backend**: Rust / Tauri v2 with `brioche-shell-runtime`
- **Frontend**: Astro 6.4 + React 19 + Zustand
- **IPC**: Tauri commands and events
- **Layout**: modular panel slots (left, right, center, bottom) + footer
- **Extensions**: context engine, memory providers, tool registry, skill registry,
  settings sections and footer metrics

## Extension Points

Brioche Desktop 0.1 exposes typed extension points so features can be added
without modifying the Tauri shell or the kernel:

| Extension | Backend Trait | Default |
|-----------|---------------|---------|
| Context engine | `ContextEngine` | Sliding-window compressor |
| Memory | `MemoryProvider` | Local JSON memory |
| Tools | `ToolProvider` | Built-in system tools + user-defined tools |
| Skills | `SkillProvider` | Hermes skill scanner |
| Settings | `SettingsSectionProvider` | Chat, Model Identity, Context, Memory |
| Footer metrics | `FooterMetricProvider` | Version, session duration, model, context |

## Slash Commands

- `/help` — show help
- `/clear` — clear conversation history
- `/session` — show current session
- `/session new` — create a new session
- `/session list` — list sessions
- `/session load <id>` — load a persisted session
- `/quit` — exit the app

## Troubleshooting

### "Connection refused" error

This means the app is trying to connect to `http://localhost:4321` (dev mode) but the frontend dev server is not running. Either:

- Run in **release mode**: `cargo run -p brioche-desktop --release`
- Or start the dev server: `cd frontend && npm run dev`

### "Frontend assets not found" error

The frontend hasn't been built. Run:

```bash
cd crates/apps/brioche-desktop/frontend
npm run build
```
