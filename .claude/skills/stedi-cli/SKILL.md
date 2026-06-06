---
name: stedi-cli
description: >-
  Drive the `stedi` command-line tool to work with Stedi's healthcare and EDI
  APIs — eligibility checks, claim status, claims submission (professional,
  institutional, dental), ERA/835 and 277 reports, insurance discovery,
  coordination of benefits, provider enrollment, the payer directory, and the
  raw EDI/X12 engine (executions, transactions, partnerships). Use this skill
  whenever the user wants to call a Stedi API, run an eligibility or claim-status
  check, submit or look up a claim, manage enrollments, search payers, work with
  X12/EDI transactions, or mentions `stedi`, Stedi, payer IDs, or 270/271/276/277/837/835
  transactions — even if they don't say "CLI". It explains how to discover
  operations, read their schemas, and build, preview, and execute requests safely.
---

# Using the Stedi CLI

`stedi` is a single binary that exposes every Stedi API operation, generated
from Stedi's official OpenAPI specs. Every command prints JSON, so you can parse
output directly. This skill is the playbook for using it effectively.

The golden rule: **discover before you call, and preview before you send.** The
CLI is self-describing — never guess an operation's name, parameters, or body
shape. Ask the CLI, then use `--dry-run` to confirm the exact request before it
hits the network.

## 0. Make sure it's available and authenticated

Check the binary exists: `stedi --version`. If `command not found`:

- Install via Homebrew: `brew tap useTaiga/stedi https://github.com/useTaiga/stedi-cli && brew install useTaiga/stedi/stedi`
- Or build from a clone: `cargo build --release` (binary at `target/release/stedi`).

Auth resolves in this order: `--api-key` flag → `$STEDI_API_KEY` → config file
(`~/.config/stedi/config.toml`). To persist a key: `stedi configure --api-key "<key>"`
(written `0600`). You do **not** need a key for `apis`, `ops`, `describe`,
`schema`, or any `--dry-run` — only for real `call`s. If a user wants to explore
without credentials, lean on `--dry-run`.

## 1. Discover the API surface

Start broad, then narrow. There are seven API groups; `apis` lists them with
their server and operation count:

```bash
stedi apis
```

Search operations by keyword across every group (matches id, path, summary,
description). This is your primary lookup tool — use it instead of guessing:

```bash
stedi ops --search eligibility        # find eligibility-related operations
stedi ops --api healthcare            # list everything in one group
```

See `references/apis.md` for what each group covers and the operations you'll
reach for most — read it when you need orientation on which API does what.

## 2. Read an operation's contract

Before building a call, get its exact parameters, request-body schema, and
responses. `$ref`s are inlined so the schema is self-contained:

```bash
stedi describe EligibilityCheck
```

Operations are addressed by `operationId`. A few ids exist in more than one
group (e.g. `GetPayerRecord` is in both `healthcare` and `payers`). When that
happens the CLI refuses and tells you to qualify it — use `api:OperationId`:

```bash
stedi describe payers:GetPayerRecord
```

A request/response body is often defined inline in `describe` output. When it
references a *named* component type, dump that type directly — for example the
`BenefitsInformation` block in a 271 eligibility response:

```bash
stedi schema BenefitsInformation --api healthcare
```

## 3. Build and preview the call

`stedi call <operationId>` runs an operation. Supply inputs with `-p KEY=VALUE`
(repeatable). The CLI auto-routes each one to the **path**, **query**, or
**body** based on the spec, so you usually don't think about where it goes.
Values are parsed as JSON when possible — `-p pageSize=50` sends a number,
`-p codes='["30"]'` sends an array, bare text stays a string.

For anything with a non-trivial request body (most POSTs — eligibility, claims,
enrollment), pass the whole body as JSON instead of many `-p` flags:

```bash
stedi call EligibilityCheck --body @request.json     # from a file
stedi call EligibilityCheck --body '{"controlNumber":"...","tradingPartnerServiceId":"..."}'
```

**Always dry-run first.** It prints the exact method, URL, headers (API key
redacted), and body without sending anything — your chance to catch a wrong
payer id, missing field, or mis-routed param:

```bash
stedi call EligibilityCheck --body @request.json --dry-run
```

When the preview looks right, drop `--dry-run` to send it. Treat dry-run as
mandatory for any state-changing call (POST/PUT/DELETE: submissions, enrollment
creation/updates, deletions).

Other flags: `--query`/`--path`/`--header KEY=VALUE` force a value into a
specific location; `--body @-` reads the body from stdin; `--api-key` overrides
the resolved key; `--verbose` echoes the request alongside the response;
`--timeout SECONDS` (default 60). Transient failures (429/5xx) are retried
automatically with backoff — add `--debug` to watch the full HTTP exchange
(auth redacted) when something goes wrong.

### Global flags worth knowing

- `--jq 'EXPR'` filters output through a built-in jq (no `jq` install needed).
  Prefer this over piping to `jq` — it's self-contained: `... --jq '.body.x'`.
- `-o compact` emits one-line JSON (nice for NDJSON-style processing); `-o table`
  is a human view. **Leave output as the default JSON when you'll parse it.**
- `--debug` dumps request+response to stderr for troubleshooting.

## 4. Read the response

`call` prints `{"status", "ok", "body"}`. Parse it; don't eyeball it:

- `ok` is `true` for a 2xx response. **Exit codes:** `0` success, `2` a non-2xx
  HTTP response (the error detail is in `body`), `1` a usage/client error
  (printed as `{"error": "..."}` on stderr). Branch on the exit code in scripts.
- On failure, read `body` for the API's error — Stedi returns structured codes
  (e.g. `access_denied`, validation messages). A `403`/`access_denied` almost
  always means a bad or missing API key.

## 5. Pagination — use `--paginate`

List endpoints (e.g. `ListEnrollments`, `ListExecutions`, `ListPayerRecords`)
page with `nextPageToken`. When a user asks for "all" of something, **don't loop
by hand** — pass `--paginate` and the CLI follows the token to exhaustion,
streaming every item as NDJSON (one compact JSON object per line):

```bash
stedi call ListEnrollments --paginate
stedi call ListPayerRecords --paginate --jq '.stediId'   # just the ids, all pages
```

`--paginate` is GET-only. (You can still page manually with `-p pageToken=…` if
you need a single specific page.)

## 6. Async transactions — use `--watch`

Much of Stedi is asynchronous: batch eligibility, executions, and transactions
move through statuses over time. Instead of polling in a loop, add `--watch` to a
GET that returns a status — the CLI re-issues it until the watched field reaches
a terminal value, then prints the final result:

```bash
stedi call GetBatch -p batchId=ba_123 --watch
stedi call GetExecution -p executionId=ex_1 --watch \
  --watch-field status --watch-until COMPLETED --watch-until FAILED --watch-interval 5
```

Defaults: polls `status` every 3s up to 300s, stopping on common terminal values
(completed/succeeded/failed/…). Use `--watch-field` if the status lives elsewhere
in the body, and `--watch-until` to set explicit terminal values.

## 7. Raw requests — the escape hatch

For an endpoint not yet in the embedded spec, or quick exploration, use the raw
commands. Give a full URL, or a path plus `--api` to resolve the base server:

```bash
stedi get /payers --api payers -p query=aetna
stedi post /some/new/endpoint --api healthcare --body @payload.json
```

Prefer `call <operationId>` when the operation exists — it validates path/params
from the spec. Reach for `get`/`post`/`delete` only when `call` can't express it.

## Worked example: an eligibility check

```bash
# 1. find it          2. learn its body
stedi ops --search eligibility
stedi describe EligibilityCheck

# 3. assemble the body from the schema (write request.json), then preview
stedi call EligibilityCheck --body @request.json --dry-run

# 4. send, and inspect body for the 271 result
stedi call EligibilityCheck --body @request.json
```

## Common pitfalls

- **Guessing operation names or fields.** Always `ops --search` / `describe`
  first; the specs are the source of truth and they change over time.
- **Skipping dry-run on submissions.** Claims and enrollments have rich,
  easy-to-get-wrong payloads — preview catches mistakes before they're real.
- **Ambiguous ids.** If `call`/`describe` complains an id spans APIs, qualify it
  with `api:OperationId`.
- **Forgetting pagination.** "All enrollments" means `--paginate`, not just the
  first page.
- **Hand-rolling polling.** For async/batch status, use `--watch` instead of a
  manual sleep loop.
- **Mistaking a `403` for a code bug.** It's auth — check `$STEDI_API_KEY` or
  run `stedi configure`.
