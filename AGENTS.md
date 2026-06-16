# Repository Agent Guidance

## CodeWhale Stewardship

- Treat community contributors as partners. Good-faith PRs, issue reports,
  repros, logs, reviews, and verification comments are maintainer evidence,
  not queue noise.
- Keep gates warm and dry-run unless Hunter explicitly approves enforcement.
  Gate copy should guide contributors clearly and respectfully.
- Credit every harvested PR, issue report, or comment that materially shaped a
  fix. Preserve authorship when possible; otherwise use mappable GitHub
  noreply `Co-authored-by` trailers from `.github/AUTHOR_MAP`.
- Do not tag, publish, create a GitHub Release, or push release artifacts
  without Hunter approval.
- Use CodeWhale branding while keeping DeepSeek support first-class. Retiring
  legacy `deepseek-tui` names must never read as deprecating DeepSeek models or
  provider support.
- Review PRs from code, tests, linked issues, comments, and check results.
  Never merge, close, harvest, or defer community work from title or labels
  alone.
- Respect concurrent work in the tree. Do not revert or rewrite unrelated
  edits by other people or agents.

## Build / Test / Lint

```bash
# Build (default-members: CLI + TUI + app-server)
cargo build --release

# Test everything
cargo test --workspace --all-features --locked

# Test a single crate
cargo test -p codewhale-tui

# Format check
cargo fmt --all -- --check

# Clippy (treats warnings as errors in CI)
cargo clippy --workspace --all-features --locked -- -D warnings

# Check compiles without codegen
cargo check --workspace --all-features

# Offline eval harness
cargo run -p codewhale-tui --all-features -- eval

# Build docs
cargo doc --workspace --no-deps
```

- **Rust edition 2024**, MSRV **1.88** (`let_chains` stabilized in 1.88, used pervasively).
- `RUSTFLAGS=-Dwarnings` in CI. CI runs lint + test on ubuntu/macos/windows;
  Linux workspace tests are on the CNB mirror.

## Architecture

CodeWhale is a terminal-native coding agent for DeepSeek models. The TUI
presentation layer is a fork of upstream focused on a Claude Code‑light visual
style, keeping the agent runtime unchanged.

```
codewhale (CLI dispatcher) ──▶ codewhale-tui (TUI binary + agent runtime)
                                  │
          ┌───────────────────────┼───────────────────────┐
          ▼                       ▼                       ▼
     app-server (HTTP/stdio)  core (Runtime engine)  whaleflow (workflows)
          │                       │
          ▼                       ▼
     protocol (shared types)   state (SQLite persistence)
          │
          ▼
     tools (trait + registry)   mcp (MCP server mgmt)
     execpolicy (approvals)     hooks (lifecycle events)
     config (providers/TOML)    agent (model registry)
     secrets (credentials)      release (update checker)
```

- **`crates/cli`** — CLI entry point (`codewhale`). Parses args, loads config,
  dispatches to TUI binary via subprocess for most subcommands (`run`, `exec`,
  `doctor`, `models`, `sessions`, `mcp`, etc.). Handles `login`/`logout` directly.
- **`crates/tui`** — Monolithic TUI binary (`codewhale-tui`, ~8k lines in
  `main.rs` + 90+ internal modules). Houses the agent loop, LLM client,
  session manager, MCP pool, tools subsystem, sub‑agent spawning, RLM
  (recursive LM), skill system, project context, compaction, and the
  ratatui‑based UI layer.
- **`crates/core`** — Runtime engine: thread lifecycle, background job manager
  with retry/backoff, conversation turn orchestration.
- **`crates/protocol`** — Shared wire types (`Thread`, `EventFrame`, `Message`,
  `ToolPayload`, `Envelope<T>`) used across crate boundaries.
- **`crates/tools`** — Tool trait (`async_trait`), `ToolRegistry`, capability
  flags, `ToolError` enum (thiserror). Individual tool implementations live in
  `crates/tui/src/tools/`.
- **`crates/state`** — SQLite‑backed persistence for threads, messages,
  checkpoints, background jobs. Also maintains an append‑only JSONL session index.
- **`crates/config`** — TOML config loading (6.7k lines), provider definitions
  for 18+ model providers, profile support, env‑var overriding.
- **`crates/agent`** — `ModelRegistry` with canonical IDs, aliases, and
  capability flags per provider. Resolution with fallback chains.
- **`crates/execpolicy`** — Permission engine: ruleset layers (builtin/agent/user),
  trusted/denied command prefixes, `ToolAskRule`, bash arity checking.
- **`crates/mcp`** — Model Context Protocol: server config, startup lifecycle,
  tool filtering, manager for concurrent MCP server processes.
- **`crates/hooks`** — Lifecycle hook dispatch (JSONL, stdout, Unix socket).
  Events: response deltas, tool lifecycle, job lifecycle, approvals.
- **`crates/app-server`** — HTTP (axum + tower‑http CORS) and stdio server
  modes. Exposes Runtime API endpoints for prompts, threads, jobs.
- **`crates/whaleflow`** — Workflow definition language (IR, validation,
  optional Starlark authoring). Currently at the typed‑IR boundary.
- **`crates/secrets`** — Credential storage: file‑based (~/.codewhale/secrets/),
  OS keyring, in‑memory (tests).
- **`crates/release`** — Update checker, version drift detection, CNB mirror
  routing, artifact checksum verification.
- **`crates/tui-core`** — Lightweight shared TUI types (`Pane`, `UiEvent`)
  used by the TUI binary and tests.

## Key Files & Directories

| Path | Purpose |
|------|---------|
| `Cargo.toml` | Workspace root (16 crates, MSRV, shared deps) |
| `config.example.toml` | Annotated config reference (900+ lines) |
| `crates/tui/src/main.rs` | TUI binary entry + subcommand dispatch |
| `crates/cli/src/lib.rs` | CLI arg parsing + TUI delegation |
| `crates/core/src/lib.rs` | Runtime, thread/job management |
| `crates/config/src/lib.rs` | Config + provider constants |
| `crates/agent/src/lib.rs` | Model registry |
| `.codewhale/constitution.json` | Repo authority policy (protected invariants) |
| `docs/ARCHITECTURE.md` | Architecture overview |
| `docs/TUI_CLAUDE_STYLE_PLAN.md` | TUI fork migration plan (6 phases) |
| `.github/workflows/ci.yml` | CI pipeline |
| `web/` | Next.js marketing site + install page |
| `integrations/` | Feishu + Telegram bridge bots |
| `scripts/release/` | Release verification + publishing |

## Coding Conventions

- **Error handling**: `anyhow::Result` for application code, `thiserror` derive
  for library error types (see `ToolError`, `SecretsError`, `WorkflowValidationError`).
- **Naming**: `codewhale_` crate prefix. `snake_case` serde tags for enums.
  `#[must_use]` on constructor helpers (e.g. `ToolError::invalid_input`).
- **Testing**: Inline `#[cfg(test)] mod tests` inside source files. No separate
  test crates. Use `cargo test -p <crate>` for focused runs.
- **Async**: `tokio::sync` primitives (RwLock, Mutex, mpsc), `async_trait` for
  tool and hook traits. `tokio::task_local!` for execution‑lock tracking.
- **Serialization**: `serde` + `serde_json::Value` pervasively. Config uses
  `toml`. Protocol types are `Serialize + Deserialize`.
- **Module organization**: TUI crate splits concerns across many single‑file
  modules (e.g. `compaction.rs`, `session_manager.rs`, `project_context.rs`).
  Sub‑directories for larger subsystems (`tui/`, `tools/`, `client/`, `llm_client/`,
  `repl/`, `rlm/`, `sandbox/`, `snapshot/`, `skills/`, `commands/`).
- **Build scripts**: `crates/cli/build.rs` and `crates/tui/build.rs` embed git
  version info via `DEEPSEEK_BUILD_VERSION` env var.

## Git Workflow

- **Branch**: `main` (upstream tracking `Hmbown/CodeWhale`). Feature branches
  off `main`. Upstream merges are periodic and tagged with `Merge upstream/main`.
- **Commits**: Conventional‑ish — `tui:`, `docs:`, `chore(tui):`, `fix:`.
- **Remote**: `git@github.com:unicornt/CodeWhale.git` (fork).

## CI/CD

- **`lint`** job: `cargo fmt --check`, `cargo clippy -- -D warnings`, provider
  registry drift check, contributor credit check.
- **`test`** job: `cargo test --workspace --all-features --locked` on
  macos/windows (Linux tests on CNB). Includes offline eval harness.
- **`versions`** job: version drift + OHOS dependency checks.
- **`npm-wrapper-smoke`**: builds release binaries, verifies npm wrapper.
- **`mobile-smoke`**: builds for mobile target (HarmonyOS/OHOS).
- **`docs`** (scheduled): `cargo doc --workspace --no-deps -Dwarnings`.
- Linux gates for mirrored branches run on CNB (`sync-cnb.yml`).

## Tips for AI Agents

- **Two‑binary architecture**: `codewhale` CLI mostly delegates to `codewhale-tui`
  via subprocess. Changes to TUI code must keep both in sync.
- **Config is huge**: `crates/config/src/lib.rs` is 6.7k lines. Provider
  constants are at the top; `ConfigToml` struct is in the middle; env‑var
  resolution helpers at the bottom. Use `grep_files` with the provider name
  to find relevant definitions.
- **Tool implementations** live in `crates/tui/src/tools/` (not in
  `crates/tools/`). The `crates/tools` crate defines only the trait and
  registry infrastructure.
- **Protected invariants** in `.codewhale/constitution.json` must be respected:
  no nightly features, keep CLI‑TUI in sync, don't remove deprecated tool
  registrations (old‑session transcript replay).
- **Prefix cache**: the system prompt is layered most‑static‑first for DeepSeek
  KV‑prefix reuse. Append new context, don't reorder existing messages.
- **Testing pattern**: add inline `#[cfg(test)] mod tests` at the bottom of the
  source file being changed. Run with `cargo test -p <crate>`.
- **TUI fork invariant**: agent loop, model routing, tool system, config
  format, slash commands, and CLI surface are unchanged from upstream — only
  the presentation layer (TUI rendering) differs. Don't introduce agent‑level
  behavior changes in TUI‑only PRs.
- **CI split**: Linux workspace tests + npm-smoke run on CNB, not GitHub
  Actions. The `ci.yml` has `if: runner.os != 'Linux'` guards. Don't be alarmed
  if Linux steps appear skipped in GitHub Actions.
