# codewhale-unicornt

CodeWhale TUI fork by @unicornt — a Claude Code‑style TUI rewrite of
[Hmbown/CodeWhale](https://github.com/Hmbown/CodeWhale) v0.8.53.
This package is independent from the upstream npm distribution.

The npm wrapper downloads the matching `codewhale` and `codewhale-tui` binaries
from the fork's GitHub Releases and exposes the `codewhale` command.

## Install

```bash
npm install -g codewhale-unicornt
```

For project-local usage:

```bash
npm install codewhale-unicornt
npx codewhale --help
```

## First run

```bash
codewhale login --api-key "YOUR_DEEPSEEK_API_KEY"
codewhale doctor
codewhale
```

Config lives at `~/.codewhale/config.toml`.

## Supported platforms

Prebuilt binaries from the fork's GitHub Release:

- Linux x64 / arm64 / riscv64
- macOS x64 / arm64
- Windows x64

Unsupported platforms should build from source:

```bash
git clone https://github.com/unicornt/CodeWhale.git
cd CodeWhale
cargo build --release
cp target/release/codewhale target/release/codewhale-tui ~/.local/bin/
```

## Wrapper configuration

| Setting | Purpose |
|---|---|
| `DEEPSEEK_TUI_VERSION` | Override the version to download |
| `DEEPSEEK_TUI_GITHUB_REPO` | Override the source repo (default: `unicornt/CodeWhale`) |
| `DEEPSEEK_TUI_RELEASE_BASE_URL` | Use a mirror release-asset directory |
| `DEEPSEEK_TUI_FORCE_DOWNLOAD=1` | Force re-download |

## Upstream

Forked from [Hmbown/CodeWhale](https://github.com/Hmbown/CodeWhale) v0.8.53.
