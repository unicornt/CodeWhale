# CodeWhale (TUI fork)

> A personal fork of [Hmbown/CodeWhale](https://github.com/Hmbown/CodeWhale) that
> rewrites the terminal UI to look closer to Claude Code's chrome-light style,
> while keeping every CodeWhale-specific feature (sidebar panels, mode switch,
> provider chip, reasoning effort chip, sub-agents, file tree, Status indicator,
> etc.) intact.

**Languages**: English · [简体中文](README.zh-CN.md)

---

## Demo

Empty-state welcome — chat area starts at row 0, no persistent header,
sidebar shares one canvas with the chat.

![Empty-state welcome](docs/start_snapshot.png)

A live turn — thinking block, tool calls, sidebar showing recent tools and
session state, footer running the whale brand glyph next to `mode · model ·
cost · saved`, with `% context left`, `Cache: hit` and `worked` chips on the
right.

![Active turn — thinking + tool calls](docs/running_snapshot2.png)

A second active-turn snapshot — different turn, mostly tool output, showing
how the cell rail keeps the chat readable even when several tool cards
stack.

![Active turn — tool-heavy](docs/running_snapshot.png)

---

## What this fork is

This is **not** a separate product. It is the upstream CodeWhale agent runtime
with its TUI presentation layer rewritten. The agent loop, model routing, tool
system, configuration format, slash commands and CLI surface are unchanged —
only what you *see* on screen has moved.

If you want the canonical project (docs, install instructions, model support,
release notes), read the upstream README:

➡️ **Upstream README**: <https://github.com/Hmbown/CodeWhale/blob/main/README.md>

If you want the binary, install from upstream — this fork has not been
released and does not publish to npm / Cargo / Homebrew.

## Base version

This branch was forked from upstream `main` at commit
[`8dff2f7525ead210a01347b48f53ae3f20d094ec`](https://github.com/Hmbown/CodeWhale/commit/8dff2f7525ead210a01347b48f53ae3f20d094ec)
(2026-06-03), which corresponds to **CodeWhale v0.8.53**. Pull from upstream
periodically; this fork does not auto-track new releases.

## Why this fork exists

The CodeWhale TUI ships a fairly dense chrome:

- Persistent header bar at the top (mode + workspace + model + chip cluster)
- Composer wrapped in a full `Borders::ALL` rounded box
- Sidebar with full-bordered panels
- Footer with an animated water-spout strip
- Tool cells wrapped in `╭ │ ╰` card frames

After spending some time using both Claude Code and CodeWhale side by side, I
wanted CodeWhale to feel as visually quiet as Claude — same chat-area focus,
same single-rule sidebar, same minimal footer — without losing any of the
information CodeWhale's sidebar and chips actually carry.

**Hard rules during the rewrite:**

1. No agent / state-machine / data-flow changes. Pure presentation layer.
2. Every CodeWhale-specific cue (mode, provider, reasoning effort, sub-agents,
   sidebar panels, status indicator) must survive the migration — only its
   *position* may change.
3. Tests are updated alongside each phase, no skipped suites.

## Phase progress

The migration plan lives at
[`docs/TUI_CLAUDE_STYLE_PLAN.md`](docs/TUI_CLAUDE_STYLE_PLAN.md). Six phases
are scoped; phases 1–5 plus a follow-up visual polish pass have landed in this
fork's working tree. Phase 6 is not started.

| Phase | Scope | Status |
|---|---|---|
| 1 | Welcome copy & accent palette — single ACCENT_PRIMARY, drop `#[allow(dead_code)]`, refreshed empty-state block, Claude Light theme tokens | ✅ committed |
| 2 | Composer chrome — top/bottom rule (no full box), decorative `> ` prefix on input, drop the density floor so the panel grows line-by-line, idle-state hint dropped | ✅ committed |
| 3 | Footer rebuild — absorbs the header chips (ctx, version, live marker, status indicator, receipts), 4-tier overflow cascade, transcript & reasoning rail switched from `▏ ` / `╎ ` to `  `, `wrap_card_rail` becomes a no-op, `compute_rail_prefix_width` Pattern C, ctx chip switched away from East-Asian ambiguous block glyphs | ✅ working tree |
| 4 | Persistent header removed — `body_area = size`, every chip moved to footer, `HeaderWidget` kept compiled with `#[allow(dead_code)]` for a future `/status` embed | ✅ working tree |
| 5 | Sidebar visual unification — `Borders::TOP` only, lowercase small-caps titles, padding budget tightened from −4/−3 to −2/−1, resize handle quiet by default and lights up on hover/drag, inline ASCII context bar removed | ✅ working tree |
| Polish | 🐳 Welcome whale, 🐳 footer brand prefix in front of `mode`, ctx chip rewritten as plain `64% context left` text (sympathetic colour escalation at 85% / 95%), retired the duplicated activity label that was repeating sidebar's recent-tools panel, restored the original `编写任务或使用 /` placeholder copy across all 7 locales | ✅ working tree |
| 6 | Tool / message cell rewrite — `⏺ <tool>(<arg>)` headers + `  ⎿ ` continuation, scoped behind `tui.cell_style = "claude" \| "classic"` feature flag, ~600 LOC + snapshot regen | ⏳ not started |

Test posture across phases 1–polish: **3986 passing**, 4 ignored, 1 known
baseline-flaky test (`mcp::tests::legacy_sse_closed_stream_reconnects_and_retries_tool_call`,
fails under parallel execution but passes in isolation; predates this fork).

## What changed visually

A short before/after by region:

- **Top of screen** — gone. The chat area now sits at row 0; the welcome block
  has 3 rows of breathing room above it.
- **Welcome block** — `>_ codewhale (v…)` heading replaced with `🐳 Welcome to
  CodeWhale`, followed by `/help` / `?` shortcut hints and a one-line
  `cwd: … model: … v…` row.
- **Composer** — top + bottom rule, no left/right sides, decorative `> ` prefix
  on every input row, no `Composer` / `Draft` title (history-search mode keeps
  its title since there's no other affordance signalling that state).
- **Sidebar** — single dim top rule per section, lowercase small-caps title
  (`work` / `tasks` / `agents` / `context`), panel body slightly deeper than
  the chat surface so it reads as a distinct rail.
- **Footer** — `🐳 agent · deepseek-v4-pro · ¥0.13 · saved ¥1.01    ●    Cache:
  75.0% hit  64% context left  v0.8.53`. The whale glyph is the row anchor
  (animates during in-flight turns, static when idle); right cluster drops
  chips from the tail under width pressure with a 4-tier cascade so the row
  never overflows.
- **Tool cells** — phase 3 dropped the `╭ │ ╰` card frame; the tool header
  glyph + indented body is the only frame. Phase 6 will rewrite the body
  itself.

## Repository layout

Same as upstream — see
[`AGENTS.md`](AGENTS.md) for the codebase tour and
[`docs/`](docs/) for migration notes:

- [`docs/TUI_CLAUDE_STYLE_PLAN.md`](docs/TUI_CLAUDE_STYLE_PLAN.md) — phase plan
  (Chinese, with the original baseline pinned by file path + line number)

## Building from source

Same as upstream. The fork has no external dependency changes.

```bash
cargo build -p codewhale-tui --release
cargo test  -p codewhale-tui --bin codewhale-tui
```

The release-tagged binaries on the upstream GitHub Releases page do **not**
include this fork's TUI changes — you must build from this branch to see
them.

## How to use

This fork isn't published to any registry — you build it from source and run
it directly.

### Prerequisites

- **Rust 1.88+** ([rustup.rs](https://rustup.rs))
- A **DeepSeek API key** ([platform.deepseek.com/api_keys](https://platform.deepseek.com/api_keys))

### 1. Clone and build

```bash
git clone https://github.com/ivorzhao/CodeWhale.git
cd CodeWhale
cargo build -p codewhale-tui --release
```

The dispatcher binary is `target/release/codewhale`, the TUI runtime is
`target/release/codewhale-tui`.

### 2. Set your API key

Either through environment variable:

```bash
export DEEPSEEK_API_KEY="sk-..."
```

Or by writing a config file at `~/.codewhale/config.toml`:

```toml
provider = "deepseek"
api_key = "sk-..."
```

See `config.example.toml` and [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md)
for every provider, model, and tuning knob.

### 3. Launch

```bash
./target/release/codewhale
```

CodeWhale starts in your current directory. Give it a task, and it reads files,
runs commands, and edits code — all visible in the transcript.

### 4. Basics

| What | How |
|---|---|
| Switch mode | `/mode agent`, `/mode plan`, `/mode yolo` |
| Change model | `/model deepseek-v4-flash` |
| Switch provider | `/provider deepseek` |
| See all commands | `/help` |
| Clear conversation | `/clear` or `Ctrl+L` |
| Open sidebar help | `?` |
| Exit | `/exit` or `Ctrl+C` |

For a deeper walkthrough, see [`docs/GUIDE.md`](docs/GUIDE.md).

### Caveat

Everything except the on-screen look is identical to upstream CodeWhale:
configuration, slash commands, agent loop, tool system, model routing — they
all work the same way. The upstream docs at
[`docs/`](docs/) and the [upstream
README](https://github.com/Hmbown/CodeWhale/blob/main/README.md) still apply.

## Contributing back

This fork's changes are intended to be upstreamed once phase 6 is reviewed.
Until then, the working tree on `tui-claude-style/phase-3-footer` is the
source of truth and PRs against this fork are welcome.
