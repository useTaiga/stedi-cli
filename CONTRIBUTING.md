# Contributing

Thanks for helping improve `stedi`.

## Development

```bash
cargo build            # debug build
cargo test             # run the integration test suite
cargo fmt              # format
cargo clippy --all-targets -- -D warnings   # lint (CI enforces this)
```

CI runs `fmt --check`, `clippy -D warnings`, and `test` on every push and PR.
Please make sure all three pass locally before opening a PR.

## Updating the embedded OpenAPI specs

The specs in [`specs/`](specs) are vendored from Stedi's public OpenAPI
repository and compiled into the binary. To refresh them:

```bash
./scripts/sync-specs.sh           # pulls the latest specs
cargo test                        # confirm nothing broke
```

Commit the updated `specs/*.json` together with a CHANGELOG entry.

## Releasing

Releases are tag-driven:

1. Bump `version` in `Cargo.toml` and add a `CHANGELOG.md` section.
2. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. The release workflow builds cross-platform binaries, attaches them (with
   `.sha256` checksums) to a GitHub Release, and updates the Homebrew formula.

Follow [Semantic Versioning](https://semver.org/).

## Code style

- Keep output JSON-only and stable — agents and scripts depend on the shape.
- Prefer the existing helpers (`emit`, `die`, `resolve_refs`, `parse_kv`).
- Add or update a test in `tests/cli.rs` for any behavior change.
