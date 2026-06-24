# Brioche Desktop 0.1

Tauri v2 desktop GUI for Brioche.

## Quick Start

### Platform Prerequisites

Before building, ensure the following tools are installed for your platform.

### Linux

- **webkit2gtk / webkit2gtk-4.1** — required by Tauri for WebKit rendering.
- **build-essential** — C/C++ compiler and Make.
- **curl** or **wget** — for fetching tooling.
- **Node.js** and **pnpm** — for the frontend build.

On Debian/Ubuntu:

```bash
sudo apt update
sudo apt install webkit2gtk-4.1 build-essential curl
```

Enable pnpm via corepack:

```bash
corepack enable
corepack prepare pnpm@latest --activate
```

### macOS

- **Xcode Command Line Tools** — provides clang and other build tools:
  ```bash
  xcode-select --install
  ```
- **Node.js** and **pnpm**.

### Windows

- **Microsoft Edge WebView2 Runtime** — required by Tauri; usually pre-installed
  on Windows 11 and recent Windows 10 builds. Download from Microsoft if it is
  missing.
- **Visual Studio Build Tools** with the **C++ workload** (or a full Visual
  Studio install with C++ support).
- **Node.js** and **pnpm**.

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

## No Telemetry

Brioche Desktop does not collect telemetry, analytics, crash reports, or usage
statistics, and it makes no network requests to Brioche or third-party analytics
services. The only network traffic the app generates is the chat inference
traffic you configure (for example, calls to your chosen base URL and model
provider).

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

## Distribution

Brioche Desktop 0.1 release binaries are currently **unsigned and unnotarized**.

Because the project does not yet have code-signing certificates or notarization
set up, platform-specific security warnings are expected when downloading or
running a prebuilt binary:

- **macOS**: Gatekeeper will show a warning such as "cannot be opened because the
  developer cannot be verified" or "the application is damaged". To run the app,
  right-click it and choose **Open**, or build from source.
- **Windows**: Microsoft Defender SmartScreen may display a "Windows protected
  your PC" or "Unknown publisher" warning. Click **More info** → **Run anyway**
  to proceed, or build from source.
- **Linux (AppImage)**: The AppImage is unsigned. Some distributions may warn
  about executing downloaded files; you may need to mark it executable with
  `chmod +x` before running. For maximum trust, build from source.

To build from source, see the [Quick Start](#quick-start) section above.

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
