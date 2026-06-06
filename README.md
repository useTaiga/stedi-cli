# stedi

[![CI](https://github.com/useTaiga/stedi-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/useTaiga/stedi-cli/actions/workflows/ci.yml)
[![Release](https://github.com/useTaiga/stedi-cli/actions/workflows/release.yml/badge.svg)](https://github.com/useTaiga/stedi-cli/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A single-binary, **agent-friendly** CLI for the [Stedi](https://www.stedi.com)
APIs. The official Stedi OpenAPI specs are embedded into the binary at build
time and reflected at runtime, so every operation is callable and discoverable
without the binary ever drifting from the published APIs.

Built for LLM agents and scripts:

- **Everything is JSON** — stdout is always machine-readable; errors are
  `{"error": "..."}` on stderr.
- **Fully discoverable** — `apis`, `ops --search`, `describe`, and `schema`
  let an agent learn the API surface with no docs.
- **Safe by default** — `call --dry-run` previews the exact request (with the
  API key redacted) before anything is sent.
- **No runtime** — one static binary, nothing to install alongside it.

---

## Install

### Homebrew (recommended)

```bash
brew tap useTaiga/stedi https://github.com/useTaiga/stedi-cli
brew install useTaiga/stedi/stedi
```

### Prebuilt binaries

Download the archive for your platform from the
[latest release](https://github.com/useTaiga/stedi-cli/releases/latest),
extract it, and put `stedi` on your `PATH`. Each archive ships with a
`.sha256` checksum.

### From source

```bash
cargo install --git https://github.com/useTaiga/stedi-cli
# or, in a clone:
cargo build --release   # -> target/release/stedi
```

---

## Quick start

```bash
# 1. Save your API key once (writes ~/.config/stedi/config.toml, mode 0600)
stedi configure --api-key "$STEDI_API_KEY"

# 2. Discover the APIs
stedi apis

# 3. Find an operation
stedi ops --search eligibility

# 4. Read its contract
stedi describe EligibilityCheck

# 5. Preview, then send
stedi call EligibilityCheck --body @request.json --dry-run
stedi call EligibilityCheck --body @request.json
```

---

## Commands

| Command | Purpose |
|---|---|
| `stedi apis` | List API groups (title, version, server, op count). |
| `stedi ops [--api A] [--search TERM]` | List/search operations across all APIs. |
| `stedi describe OP [--api A]` | Full contract for one operation (`$ref`s inlined). |
| `stedi schema NAME [--api A]` | Dump a component schema with `$ref`s resolved. |
| `stedi call OP [opts]` | Execute an operation against the live API. |
| `stedi configure [--api-key K]` | Store an API key in the config file. |
| `stedi --version` / `stedi --help` | Version and help. |

### Referencing operations

Operations are addressed by `operationId`. When the same id exists in more than
one API (e.g. `GetPayerRecord` is in both `healthcare` and `payers`), qualify it
as `api:OperationId` or pass `--api`:

```bash
stedi describe payers:GetPayerRecord
stedi call healthcare:GetPayerRecord -p stediId=AETNA --dry-run
```

### Passing parameters to `call`

- `-p KEY=VALUE` (repeatable) — auto-routed to **path**, **query**, or **body**
  based on the spec. Values are parsed as JSON when possible, so
  `-p pageSize=50` sends a number and `-p ids='["a","b"]'` sends an array.
- `--body JSON` or `--body @file.json` — supply the entire request body at once
  (wins over assembled `-p` values).
- `--path KEY=VALUE`, `--query KEY=VALUE`, `--header KEY=VALUE` — force a value
  into a specific location.
- `--api-key KEY` — override the resolved key for one call.
- `--dry-run` — print the exact request (auth redacted) without sending it.
- `--verbose` — include the request alongside the response.
- `--timeout SECONDS` — request timeout (default 60).

---

## Authentication

The API key is resolved in this order:

1. `--api-key` flag
2. `$STEDI_API_KEY` environment variable
3. config file — `$XDG_CONFIG_HOME/stedi/config.toml` (default
   `~/.config/stedi/config.toml`), written by `stedi configure`

The config file contains a single line, `api_key = "..."`, and is created with
`0600` permissions. Set `$STEDI_CONFIG` to point at an alternate path.

The key is sent as the `Authorization` header and is **always redacted** in
`--dry-run` / `--verbose` output.

---

## Output & exit codes

- All output is JSON on stdout; errors are `{"error": "..."}` on stderr.
- `call` prints `{"status", "ok", "body"}`.
- Exit codes: `0` success, `1` usage/client error, `2` a non-2xx HTTP response.

---

## How it stays in sync with the specs

`src/main.rs` embeds the vendored specs in [`specs/`](specs) via `include_str!`.
Refresh them from the upstream Stedi OpenAPI repository with
[`scripts/sync-specs.sh`](scripts/sync-specs.sh), then rebuild — there is no
codegen step.

## Using with AI agents

This repo ships a [Claude Code skill](.claude/skills/stedi-cli/SKILL.md) that
teaches an agent how to drive `stedi` — discovering operations, reading their
schemas, and building/previewing/executing requests safely. It activates
automatically in Claude Code when you work in this repo, or copy
`.claude/skills/stedi-cli` into `~/.claude/skills/` to use it everywhere.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Security issues: see
[SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE) © Taiga
