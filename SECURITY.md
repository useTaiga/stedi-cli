# Security Policy

## Reporting a vulnerability

Please report security issues privately to **security@usetaiga.com** (or open a
GitHub [security advisory](https://github.com/useTaiga/stedi-cli/security/advisories/new)).
Do not file public issues for vulnerabilities.

We aim to acknowledge reports within 3 business days and to provide a
remediation timeline after triage.

## Handling of secrets

- `stedi` never logs or prints your API key. It is redacted as `<redacted>` in
  all `--dry-run` and `--verbose` output.
- The config file (`~/.config/stedi/config.toml`) is written with `0600`
  permissions and contains only the API key.
- The key is transmitted only to the official Stedi API hosts declared in the
  embedded OpenAPI `servers` blocks, over HTTPS.

## Supported versions

Security fixes are released for the latest published version.
