# Brioche Desktop

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
- `BRIOCHE_MODEL` — defaults to `gpt-4o-mini`
- `BRIOCHE_BASE_URL` — defaults to `https://api.openai.com/v1`
- `BRIOCHE_MAX_TOKENS` — defaults to `4096`
- `BRIOCHE_REASONING_EFFORT` — optional

## Architecture

- **Backend**: Rust / Tauri v2 with `brioche-shell-runtime`
- **Frontend**: Astro 6.4 + React 19 + Zustand
- **IPC**: Tauri commands (`send_message`, `get_messages`, `clear_messages`) + events (`chat-message`)
- **Slash commands**: `/help`, `/clear`, `/session`, `/quit`

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
