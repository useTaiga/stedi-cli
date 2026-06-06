# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-06-06

### Added
- **Reliability:** automatic retries on transient failures (429/5xx) with
  exponential backoff + jitter, honoring `Retry-After`. Tune with `--max-retries`.
  Non-idempotent methods only retry on 429/503.
- **`--debug`:** print the full HTTP exchange (request + response, auth redacted)
  to stderr.
- **`--paginate`:** follow `nextPageToken` and stream every item as NDJSON (GET).
- **`--watch`:** re-issue a GET until a terminal status, then print the result;
  configurable via `--watch-field`, `--watch-until`, `--watch-interval`,
  `--watch-timeout`. Great for async transactions/batches.
- **Built-in `--jq`:** filter output with a jq expression — no `jq` install needed.
- **Output formats:** `-o/--output json|compact|table` (JSON stays the default).
- **Raw requests:** `stedi get|post|delete <path|url>` escape hatch for endpoints
  not yet in the spec or quick exploration.
- **`--body @-`:** read a request body from stdin.
- **`stedi completions <shell>`** and **`stedi man`** generation; the Homebrew
  formula now installs completions and a man page automatically.
- **`--version`** now embeds the git short SHA and build date.

## [0.1.1] - 2026-06-06

### Fixed
- Release workflow now finds the correct checksum assets (`stedi-<target>.sha256`)
  and generates the Homebrew formula via a tested `scripts/gen-formula.sh`, so
  the tagged release pipeline (binaries + formula auto-update) completes green.

## [0.1.0] - 2026-06-05

### Added
- Initial release.
- Spec-driven CLI over the bundled Stedi OpenAPI specs (`claims`, `core`,
  `enrollment`, `event-destinations`, `healthcare`, `manager`, `payers`),
  embedded at build time.
- Commands: `apis`, `ops`, `describe`, `schema`, `call`, `configure`.
- `call` with auto-routed `-p` parameters, `--body`/`@file`, `--dry-run`,
  `--verbose`, and `--timeout`.
- API-key resolution via `--api-key`, `$STEDI_API_KEY`, or a `0600` config file
  at `~/.config/stedi/config.toml`.
- Cross-platform release binaries and a Homebrew formula.

[Unreleased]: https://github.com/useTaiga/stedi-cli/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/useTaiga/stedi-cli/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/useTaiga/stedi-cli/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/useTaiga/stedi-cli/releases/tag/v0.1.0
