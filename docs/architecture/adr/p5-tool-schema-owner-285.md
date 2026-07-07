# ADR: Shell-owned tool schema construction for issue #285

## Status

Accepted

## Context

Issue #285 identified duplicated JSON schema construction across Shell Runtime,
Shell Persistence, Providers, and Tools:

- **Book III-A — Shell Runtime** owns the async shell mechanism and existing tool
  executor boundaries.
- **Book III-B — Providers** need OpenAI-compatible function-tool wrappers.
- **Book III-C — Tools** expose parameter schemas for built-in filesystem,
  shell, and web tools.
- **Shell Persistence / Projection** expose descriptors for desktop extension and
  settings surfaces.

The previous shape assembled `serde_json::Map` objects in each crate. That made
schema ownership stringly and duplicated the OpenAI function wrapper. The change
is cross-book within the shell layer because providers, tools, and persistence
must produce the same schema shape without adding a new dependency cycle.

## Decisions

1. **Shell Runtime owns the typed schema builder.**
   - `brioche-shell-runtime::tool_schema` defines the primitive parameter types,
     parameter metadata, JSON Schema object construction, and OpenAI function-tool
     wrapping.
   - The owner remains mechanism code: it builds structural metadata only and
     performs no I/O or policy checks.

2. **Built-in tools use typed parameter metadata.**
   - Filesystem, shell, and web tool schemas now call `tool_parameters_schema`
     instead of assembling raw maps locally.
   - The supported primitive property types are the current built-in contract:
     `string`, `boolean`, and `integer`.

3. **OpenAI wrapper construction is centralized.**
   - `SystemToolExecutor` and AMP memory provider tool schemas use the shared
     OpenAI function-tool wrapper.
   - The request body builder accepts owned tool schema objects and moves them
     into the payload; shared caches clone before the ownership boundary and
     document that allocation.

4. **Extension boundaries remain explicit JSON.**
   - `UserToolDefinition.parameters` remains `serde_json::Value` because user and
     extension tools are the intentional arbitrary JSON boundary.
   - Built-in `ToolDescriptor` values are produced through constructors that use
     typed schema metadata.

## Consequences

- Built-in schema output shape stays stable while construction has one owner.
- Provider/tool/persistence crates depend on the Shell Runtime schema owner they
  already depend on, avoiding new crate cycles.
- Adding a new primitive parameter type now requires one enum variant and focused
  shape tests instead of hand-editing raw maps across crates.
- User-defined tool schemas remain flexible, but the clone at that boundary is
  documented as extension-owned arbitrary JSON.

## Invariants

Refs: I-Shell-Runtime-OnlyIO, I-Shell-ToolResult-PassThrough
