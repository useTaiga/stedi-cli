//! stedi — a single-binary, agent-friendly CLI for the Stedi APIs.
//!
//! The official Stedi OpenAPI documents are embedded at compile time
//! (`include_str!`) and reflected at runtime, so the binary never drifts from
//! the specs and needs no files on disk. Designed to be driven by LLM agents:
//! output is JSON by default, operations are fully discoverable, and
//! `call --dry-run` previews a request before it is sent.
//!
//! Operations are referenced by operationId. When the same id exists in more
//! than one API, qualify it as `api:OperationId` or pass `--api`.
//!
//! Auth precedence: `--api-key` > `$STEDI_API_KEY` > config file
//! (`$XDG_CONFIG_HOME/stedi/config.toml`, default `~/.config/stedi/config.toml`).

use std::path::PathBuf;
use std::process::exit;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use serde_json::{json, Map, Value};

mod jq;

const HTTP_METHODS: [&str; 7] = ["get", "post", "put", "delete", "patch", "head", "options"];

/// Status codes worth retrying. 429/503 mean "try later" (safe for any method);
/// 500/502/504 are retried only for idempotent methods (see `is_idempotent`).
const RETRY_ALWAYS: [u16; 2] = [429, 503];
const RETRY_IDEMPOTENT: [u16; 5] = [429, 500, 502, 503, 504];

/// Default terminal statuses for `--watch` — when the watched field reaches one
/// of these, polling stops. Case-insensitive; override with `--watch-until`.
const TERMINAL_STATUSES: [&str; 11] = [
    "completed",
    "succeeded",
    "success",
    "failed",
    "failure",
    "errored",
    "error",
    "delivered",
    "cancelled",
    "canceled",
    "rejected",
];

/// Specs embedded at compile time: (api_name, raw_json). Paths are relative to
/// this source file (src/main.rs -> ../specs).
const SPECS: [(&str, &str); 7] = [
    ("claims", include_str!("../specs/claims.json")),
    ("core", include_str!("../specs/core.json")),
    ("enrollment", include_str!("../specs/enrollment.json")),
    (
        "event-destinations",
        include_str!("../specs/event-destinations.json"),
    ),
    ("healthcare", include_str!("../specs/healthcare.json")),
    ("manager", include_str!("../specs/manager.json")),
    ("payers", include_str!("../specs/payers.json")),
];

const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("STEDI_GIT_SHA"),
    ", ",
    env!("STEDI_BUILD_DATE"),
    ")"
);

const EXAMPLES: &str = "\
EXAMPLES:
  stedi apis                                  # list the API groups
  stedi ops --search eligibility              # find operations
  stedi describe EligibilityCheck             # see params + body schema
  stedi call EligibilityCheck --body @req.json --dry-run   # preview a request
  stedi call ListEnrollments --paginate --jq '.[].id'      # every id, all pages
  stedi call GetBatch -p batchId=ba_1 --watch # poll until the batch finishes
  stedi get /payers -p query=aetna --api payers            # raw request

Output is JSON by default. Use -o table for a human view, or --jq to filter.";

// --------------------------------------------------------------------------- //
// CLI definition
// --------------------------------------------------------------------------- //
#[derive(Parser)]
#[command(
    name = "stedi",
    version = VERSION,
    about = "Agent-friendly CLI for the Stedi APIs. JSON output by default.",
    long_about = "Agent-friendly CLI for the Stedi APIs, driven by the official \
OpenAPI specs (embedded at build time).\n\n\
Auth precedence: --api-key > $STEDI_API_KEY > ~/.config/stedi/config.toml",
    after_help = EXAMPLES
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Output format for results.
    #[arg(long, short = 'o', global = true, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,

    /// Filter output through a jq expression (built-in; jq need not be installed).
    #[arg(long, global = true, value_name = "EXPR")]
    jq: Option<String>,

    /// Print the full HTTP exchange (request + response) to stderr; auth redacted.
    #[arg(long, global = true)]
    debug: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    /// Pretty-printed JSON (the stable, machine-readable default).
    Json,
    /// Compact one-line JSON.
    Compact,
    /// Human-readable table (best for `apis`/`ops` and arrays of flat objects).
    Table,
}

/// Options that flow into every result-producing command.
struct Out {
    format: OutputFormat,
    jq: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// List the available API groups (specs).
    Apis,
    /// List/search operations.
    Ops {
        /// Restrict to one API group.
        #[arg(long)]
        api: Option<String>,
        /// Filter by substring across id/path/summary.
        #[arg(long)]
        search: Option<String>,
    },
    /// Show one operation's params, body, and responses.
    Describe {
        /// operationId or api:operationId
        operation: String,
        #[arg(long)]
        api: Option<String>,
    },
    /// Dump a named component schema (refs inlined).
    Schema {
        name: String,
        #[arg(long)]
        api: Option<String>,
    },
    /// Execute an operation against the live API.
    Call(CallArgs),
    /// Raw GET to an arbitrary path/URL.
    Get(RawArgs),
    /// Raw POST to an arbitrary path/URL.
    Post(RawArgs),
    /// Raw DELETE to an arbitrary path/URL.
    Delete(RawArgs),
    /// Save an API key to the config file for future use.
    Configure {
        #[arg(long = "api-key")]
        api_key: Option<String>,
        #[arg(long)]
        show_path: bool,
    },
    /// Print a shell completion script (bash, zsh, fish, powershell, elvish).
    Completions { shell: clap_complete::Shell },
    /// Generate man pages (to a directory, or the main page to stdout).
    Man {
        /// Directory to write one .1 page per command into.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

#[derive(clap::Args)]
struct CallArgs {
    /// operationId or api:operationId
    operation: String,
    #[arg(long)]
    api: Option<String>,
    /// Parameter, auto-routed to path/query/body per the spec. Repeatable.
    #[arg(short = 'p', long = "param", value_name = "KEY=VALUE")]
    param: Vec<String>,
    #[arg(long = "path", value_name = "KEY=VALUE")]
    path_param: Vec<String>,
    #[arg(long, value_name = "KEY=VALUE")]
    query: Vec<String>,
    #[arg(long, value_name = "KEY=VALUE")]
    header: Vec<String>,
    /// Full JSON request body, `@file`, or `@-` for stdin.
    #[arg(long, value_name = "JSON|@file")]
    body: Option<String>,
    #[arg(long = "api-key")]
    api_key: Option<String>,
    /// Print the request that would be sent; do not send it.
    #[arg(long = "dry-run")]
    dry_run: bool,
    /// Include the request in the response output.
    #[arg(long)]
    verbose: bool,
    /// Follow `nextPageToken` and stream every item as NDJSON (GET only).
    #[arg(long)]
    paginate: bool,
    /// Re-issue the request until a terminal status, then print the result (GET only).
    #[arg(long)]
    watch: bool,
    /// Field in the response body to watch (dot-path supported). Default: status.
    #[arg(long, default_value = "status")]
    watch_field: String,
    /// Terminal value(s) that stop a watch (repeatable). Defaults to common statuses.
    #[arg(long = "watch-until", value_name = "VALUE")]
    watch_until: Vec<String>,
    /// Seconds between watch polls.
    #[arg(long, default_value_t = 3.0)]
    watch_interval: f64,
    /// Give up watching after this many seconds (0 = no limit).
    #[arg(long, default_value_t = 300.0)]
    watch_timeout: f64,
    /// Max retry attempts on transient errors (429/5xx).
    #[arg(long, default_value_t = 3)]
    max_retries: u32,
    #[arg(long, default_value_t = 60.0)]
    timeout: f64,
}

#[derive(clap::Args)]
struct RawArgs {
    /// Absolute URL, or a path resolved against the chosen API's server.
    path: String,
    /// Which API group's server to resolve a path against (required for non-URL paths).
    #[arg(long)]
    api: Option<String>,
    #[arg(short = 'p', long = "param", value_name = "KEY=VALUE")]
    param: Vec<String>,
    #[arg(long, value_name = "KEY=VALUE")]
    header: Vec<String>,
    /// Request body: JSON, `@file`, or `@-` for stdin (POST).
    #[arg(long, value_name = "JSON|@file")]
    body: Option<String>,
    #[arg(long = "api-key")]
    api_key: Option<String>,
    #[arg(long)]
    paginate: bool,
    #[arg(long, default_value_t = 3)]
    max_retries: u32,
    #[arg(long, default_value_t = 60.0)]
    timeout: f64,
}

// --------------------------------------------------------------------------- //
// Output helpers
// --------------------------------------------------------------------------- //
fn die(msg: impl AsRef<str>) -> ! {
    eprintln!("{}", json!({ "error": msg.as_ref() }));
    exit(1);
}

/// Render and print a single result value, applying `--jq` then the format.
fn emit(value: &Value, out: &Out) {
    if let Some(prog) = &out.jq {
        match jq::run(value, prog) {
            Ok(results) => {
                for r in &results {
                    print_value(r, out.format);
                }
            }
            Err(e) => die(format!("jq error: {e}")),
        }
        return;
    }
    print_value(value, out.format);
}

fn print_value(value: &Value, format: OutputFormat) {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(value).unwrap()),
        OutputFormat::Compact => println!("{}", serde_json::to_string(value).unwrap()),
        OutputFormat::Table => print!("{}", render_table(value)),
    }
}

/// Render a JSON array of flat objects as a table; fall back to pretty JSON for
/// anything that isn't tabular (the discovery commands produce ideal input).
fn render_table(value: &Value) -> String {
    use comfy_table::{presets::UTF8_FULL, Cell, Table};
    let rows = match value.as_array() {
        Some(a) if a.iter().all(|v| v.is_object()) && !a.is_empty() => a,
        _ => return serde_json::to_string_pretty(value).unwrap() + "\n",
    };
    // Column set = union of keys, in first-seen order.
    let mut cols: Vec<String> = Vec::new();
    for row in rows {
        for k in row.as_object().unwrap().keys() {
            if !cols.contains(k) {
                cols.push(k.clone());
            }
        }
    }
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(cols.iter().map(Cell::new));
    for row in rows {
        let obj = row.as_object().unwrap();
        table.add_row(cols.iter().map(|c| Cell::new(cell_text(obj.get(c)))));
    }
    table.to_string() + "\n"
}

fn cell_text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

// --------------------------------------------------------------------------- //
// Spec loading
// --------------------------------------------------------------------------- //
fn load_specs() -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for (name, raw) in SPECS {
        match serde_json::from_str::<Value>(raw) {
            Ok(doc) if doc.get("openapi").is_some() => out.push((name.to_string(), doc)),
            Ok(_) => {}
            Err(e) => die(format!("Failed to parse embedded spec '{name}': {e}")),
        }
    }
    out
}

fn iter_operations(specs: &[(String, Value)]) -> Vec<(String, String, String, Value)> {
    let mut out = Vec::new();
    for (api, doc) in specs {
        let Some(paths) = doc.get("paths").and_then(Value::as_object) else {
            continue;
        };
        for (path, item) in paths {
            let Some(item) = item.as_object() else {
                continue;
            };
            for (method, op) in item {
                if HTTP_METHODS.contains(&method.to_lowercase().as_str()) && op.is_object() {
                    out.push((api.clone(), method.to_lowercase(), path.clone(), op.clone()));
                }
            }
        }
    }
    out
}

fn find_operation(
    specs: &[(String, Value)],
    op_ref: &str,
    api_filter: Option<&str>,
) -> (String, String, String, Value) {
    let known: Vec<&str> = specs.iter().map(|(a, _)| a.as_str()).collect();
    let (api_filter, op_ref): (Option<String>, String) = match op_ref.split_once(':') {
        Some((a, rest)) if known.contains(&a) => (Some(a.to_string()), rest.to_string()),
        _ => (api_filter.map(String::from), op_ref.to_string()),
    };

    let mut matches: Vec<(String, String, String, Value)> = iter_operations(specs)
        .into_iter()
        .filter(|(api, _, _, op)| {
            api_filter.as_ref().is_none_or(|f| f == api)
                && op.get("operationId").and_then(Value::as_str) == Some(op_ref.as_str())
        })
        .collect();

    if matches.is_empty() {
        let scope = api_filter
            .map(|a| format!(" in api '{a}'"))
            .unwrap_or_default();
        die(format!(
            "No operation '{op_ref}'{scope}. Try: stedi ops --search {op_ref}"
        ));
    }
    if matches.len() > 1 {
        let opts: Vec<String> = matches
            .iter()
            .map(|m| format!("{}:{op_ref}", m.0))
            .collect();
        die(format!(
            "Operation '{op_ref}' is ambiguous across APIs. Qualify it: {}",
            opts.join(", ")
        ));
    }
    matches.remove(0)
}

fn resolve_refs(node: &Value, doc: &Value, depth: usize, seen: &[String]) -> Value {
    if depth > 12 {
        return json!({ "$ref_truncated": "max depth reached" });
    }
    match node {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get("$ref") {
                if seen.contains(r) {
                    return json!({ "$ref_cycle": r });
                }
                if let Some(ptr) = r.strip_prefix("#/") {
                    let mut target = doc;
                    for part in ptr.split('/') {
                        let part = part.replace("~1", "/").replace("~0", "~");
                        match target.get(&part) {
                            Some(next) => target = next,
                            None => return json!({ "$ref_unresolved": r }),
                        }
                    }
                    let mut next_seen = seen.to_vec();
                    next_seen.push(r.clone());
                    return resolve_refs(target, doc, depth + 1, &next_seen);
                }
                return node.clone();
            }
            let mut out = Map::new();
            for (k, v) in map {
                out.insert(k.clone(), resolve_refs(v, doc, depth + 1, seen));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| resolve_refs(v, doc, depth + 1, seen))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn server_url(doc: &Value) -> String {
    doc.get("servers")
        .and_then(Value::as_array)
        .and_then(|s| s.first())
        .and_then(|s| s.get("url"))
        .and_then(Value::as_str)
        .map(|u| u.trim_end_matches('/').to_string())
        .unwrap_or_else(|| die("Spec has no servers[].url to build a request from"))
}

// --------------------------------------------------------------------------- //
// Small utilities
// --------------------------------------------------------------------------- //
fn parse_kv(items: &[String]) -> Map<String, Value> {
    let mut out = Map::new();
    for item in items {
        let Some((k, v)) = item.split_once('=') else {
            die(format!("Expected key=value, got '{item}'"));
        };
        let val = serde_json::from_str::<Value>(v).unwrap_or_else(|_| Value::String(v.to_string()));
        out.insert(k.to_string(), val);
    }
    out
}

fn scalar_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn build_query(map: &Map<String, Value>) -> String {
    let mut pairs: Vec<String> = Vec::new();
    for (k, v) in map {
        let items = match v {
            Value::Array(a) => a.clone(),
            other => vec![other.clone()],
        };
        for item in items {
            pairs.push(format!(
                "{}={}",
                percent_encode(k),
                percent_encode(&scalar_to_string(&item))
            ));
        }
    }
    pairs.join("&")
}

/// Read a `--body` argument: inline JSON, `@file`, or `@-` (stdin).
fn read_body(raw: &str) -> Value {
    let text = if raw == "@-" {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .unwrap_or_else(|e| die(format!("Cannot read stdin: {e}")));
        s
    } else if let Some(fp) = raw.strip_prefix('@') {
        std::fs::read_to_string(fp)
            .unwrap_or_else(|e| die(format!("Cannot read body file '{fp}': {e}")))
    } else {
        raw.to_string()
    };
    serde_json::from_str(&text).unwrap_or_else(|e| die(format!("--body is not valid JSON: {e}")))
}

/// Cheap pseudo-random in [0, cap_ms], seeded from the clock — for backoff jitter.
fn jitter_ms(cap_ms: u64) -> u64 {
    if cap_ms == 0 {
        return 0;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos as u64).wrapping_mul(2654435761) % (cap_ms + 1)
}

fn is_idempotent(method: &str) -> bool {
    matches!(method, "GET" | "HEAD" | "PUT" | "DELETE" | "OPTIONS")
}

fn resolve_api_key(explicit: &Option<String>) -> Option<String> {
    explicit
        .clone()
        .or_else(|| {
            std::env::var("STEDI_API_KEY")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .or_else(config_api_key)
}

fn config_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("STEDI_CONFIG") {
        return Some(PathBuf::from(explicit));
    }
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("stedi").join("config.toml"))
}

fn config_api_key() -> Option<String> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    text.parse::<toml::Value>()
        .ok()?
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Look up a possibly-dotted path in a JSON value (for `--watch-field`).
fn get_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = value;
    for part in path.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur)
}

// --------------------------------------------------------------------------- //
// HTTP execution (with retries + optional --debug)
// --------------------------------------------------------------------------- //
struct Exec {
    max_retries: u32,
    timeout: f64,
    debug: bool,
}

/// Send one request, retrying transient failures with backoff + jitter, honoring
/// `Retry-After`. Returns (status, body-text).
fn send(
    method: &str,
    url: &str,
    headers: &Map<String, Value>,
    data: Option<&str>,
    ex: &Exec,
) -> (u16, String) {
    let retryable: &[u16] = if is_idempotent(method) {
        &RETRY_IDEMPOTENT
    } else {
        &RETRY_ALWAYS
    };
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs_f64(ex.timeout))
        .build();

    let mut attempt = 0u32;
    loop {
        if ex.debug {
            debug_request(method, url, headers, data, attempt);
        }
        let started = Instant::now();
        let mut req = agent.request(method, url);
        for (k, v) in headers {
            req = req.set(k, &scalar_to_string(v));
        }
        let result = match data {
            Some(d) => req.send_string(d),
            None => req.call(),
        };

        let (status, body, retry_after) = match result {
            Ok(resp) => {
                let s = resp.status();
                (s, resp.into_string().unwrap_or_default(), None)
            }
            Err(ureq::Error::Status(code, resp)) => {
                let ra = resp
                    .header("retry-after")
                    .and_then(|v| v.parse::<u64>().ok());
                (code, resp.into_string().unwrap_or_default(), ra)
            }
            Err(ureq::Error::Transport(e)) => {
                // Only retry transport errors for idempotent methods (a POST may
                // have been received), and only if attempts remain.
                if is_idempotent(method) && attempt < ex.max_retries {
                    let delay = backoff_ms(attempt, None);
                    if ex.debug {
                        eprintln!("[stedi] transport error: {e}; retry in {delay}ms");
                    }
                    sleep(Duration::from_millis(delay));
                    attempt += 1;
                    continue;
                }
                die(format!("Request failed: {e}"));
            }
        };

        if ex.debug {
            eprintln!(
                "[stedi] <- {status} in {}ms{}",
                started.elapsed().as_millis(),
                if attempt > 0 {
                    format!(" (attempt {})", attempt + 1)
                } else {
                    String::new()
                }
            );
        }

        if retryable.contains(&status) && attempt < ex.max_retries {
            let delay = backoff_ms(attempt, retry_after);
            if ex.debug {
                eprintln!("[stedi] retrying after {status} in {delay}ms");
            }
            sleep(Duration::from_millis(delay));
            attempt += 1;
            continue;
        }
        return (status, body);
    }
}

/// Backoff: honor Retry-After if present (clamped to 60s); else exponential
/// (base 500ms, doubling, capped 20s) with full jitter.
fn backoff_ms(attempt: u32, retry_after_secs: Option<u64>) -> u64 {
    if let Some(secs) = retry_after_secs {
        return secs.min(60) * 1000;
    }
    let cap = 20_000u64;
    let base = 500u64.saturating_mul(1u64 << attempt.min(6));
    jitter_ms(base.min(cap))
}

fn debug_request(
    method: &str,
    url: &str,
    headers: &Map<String, Value>,
    data: Option<&str>,
    attempt: u32,
) {
    let n = if attempt > 0 {
        format!(" (attempt {})", attempt + 1)
    } else {
        String::new()
    };
    eprintln!("[stedi] -> {method} {url}{n}");
    for (k, v) in headers {
        let shown = if k.eq_ignore_ascii_case("authorization") {
            "<redacted>"
        } else {
            &scalar_to_string(v)
        };
        eprintln!("[stedi]    {k}: {shown}");
    }
    if let Some(d) = data {
        eprintln!("[stedi]    body: {d}");
    }
}

/// Headers common to every call: auth + content negotiation.
fn base_headers(
    api_key: Option<&str>,
    has_body: bool,
    extra: Map<String, Value>,
) -> Map<String, Value> {
    let mut headers = extra;
    if !headers.contains_key("Authorization") {
        if let Some(k) = api_key {
            headers.insert("Authorization".into(), Value::String(k.to_string()));
        }
    }
    if has_body {
        headers
            .entry("Content-Type".to_string())
            .or_insert(Value::String("application/json".into()));
    }
    headers
        .entry("Accept".to_string())
        .or_insert(Value::String("application/json".into()));
    headers
}

fn parse_response(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

// --------------------------------------------------------------------------- //
// Commands: discovery
// --------------------------------------------------------------------------- //
fn cmd_apis(specs: &[(String, Value)], out: &Out) {
    let list: Vec<Value> = specs
        .iter()
        .map(|(api, doc)| {
            let info = doc.get("info").cloned().unwrap_or_default();
            let ops = doc
                .get("paths")
                .and_then(Value::as_object)
                .map(|p| {
                    p.values()
                        .filter_map(Value::as_object)
                        .map(|item| item.keys().filter(|m| HTTP_METHODS.contains(&m.to_lowercase().as_str())).count())
                        .sum::<usize>()
                })
                .unwrap_or(0);
            json!({
                "api": api,
                "title": info.get("title"),
                "version": info.get("version"),
                "server": doc.get("servers").and_then(Value::as_array).and_then(|s| s.first()).and_then(|s| s.get("url")),
                "operations": ops,
            })
        })
        .collect();
    emit(&Value::Array(list), out);
}

fn cmd_ops(specs: &[(String, Value)], api: Option<&str>, search: Option<&str>, out: &Out) {
    let term = search.unwrap_or("").to_lowercase();
    let mut list: Vec<Value> = Vec::new();
    for (a, method, path, op) in iter_operations(specs) {
        if api.is_some_and(|f| f != a) {
            continue;
        }
        let opid = op.get("operationId").and_then(Value::as_str).unwrap_or("");
        let summary = op.get("summary").and_then(Value::as_str).unwrap_or("");
        let desc = op.get("description").and_then(Value::as_str).unwrap_or("");
        let hay = format!("{a} {opid} {method} {path} {summary} {desc}").to_lowercase();
        if !term.is_empty() && !hay.contains(&term) {
            continue;
        }
        list.push(json!({
            "api": a,
            "operationId": opid,
            "method": method.to_uppercase(),
            "path": path,
            "summary": summary,
        }));
    }
    list.sort_by(|x, y| {
        let key = |v: &Value| {
            (
                v["api"].as_str().unwrap_or("").to_string(),
                v["path"].as_str().unwrap_or("").to_string(),
            )
        };
        key(x).cmp(&key(y))
    });
    emit(&Value::Array(list), out);
}

fn cmd_describe(specs: &[(String, Value)], operation: &str, api: Option<&str>, out: &Out) {
    let (api, method, path, op) = find_operation(specs, operation, api);
    let doc = &specs.iter().find(|(a, _)| *a == api).unwrap().1;

    let params: Vec<Value> = op
        .get("parameters")
        .and_then(Value::as_array)
        .map(|ps| {
            ps.iter()
                .map(|p| {
                    let p = resolve_refs(p, doc, 0, &[]);
                    let in_ = p.get("in").and_then(Value::as_str).unwrap_or("");
                    json!({
                        "name": p.get("name"),
                        "in": p.get("in"),
                        "required": p.get("required").and_then(Value::as_bool).unwrap_or(in_ == "path"),
                        "description": p.get("description"),
                        "schema": p.get("schema"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let body = op.get("requestBody").map(|rb| {
        let rb = resolve_refs(rb, doc, 0, &[]);
        let content = rb.get("content").and_then(Value::as_object).cloned().unwrap_or_default();
        let ctype = if content.contains_key("application/json") {
            Some("application/json".to_string())
        } else {
            content.keys().next().cloned()
        };
        let schema = ctype.as_ref().and_then(|c| content.get(c)).and_then(|c| c.get("schema")).cloned();
        json!({ "required": rb.get("required").and_then(Value::as_bool).unwrap_or(false), "contentType": ctype, "schema": schema })
    });

    let mut responses = Map::new();
    if let Some(rs) = op.get("responses").and_then(Value::as_object) {
        for (code, resp) in rs {
            let resp = resolve_refs(resp, doc, 0, &[]);
            responses.insert(
                code.clone(),
                resp.get("description").cloned().unwrap_or(Value::Null),
            );
        }
    }

    emit(
        &json!({
            "api": api,
            "operationId": op.get("operationId"),
            "method": method.to_uppercase(),
            "path": path,
            "server": server_url(doc),
            "summary": op.get("summary"),
            "description": op.get("description"),
            "parameters": params,
            "requestBody": body,
            "responses": Value::Object(responses),
        }),
        out,
    );
}

fn cmd_schema(specs: &[(String, Value)], name: &str, api: Option<&str>, out: &Out) {
    let ptr = format!(
        "/components/schemas/{}",
        name.replace('~', "~0").replace('/', "~1")
    );
    let schemas_of = |doc: &Value| doc.pointer(&ptr).cloned();

    let api = match api {
        Some(a) => a.to_string(),
        None => {
            let hits: Vec<&String> = specs
                .iter()
                .filter(|(_, d)| schemas_of(d).is_some())
                .map(|(a, _)| a)
                .collect();
            match hits.len() {
                0 => die(format!("No schema '{name}' in any spec")),
                1 => hits[0].clone(),
                _ => die(format!(
                    "Schema '{name}' in multiple APIs; pass --api: {}",
                    hits.iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            }
        }
    };
    let doc = match specs.iter().find(|(a, _)| *a == api) {
        Some((_, doc)) => doc,
        None => die(format!("Unknown api '{api}'")),
    };
    match schemas_of(doc) {
        Some(s) => emit(&resolve_refs(&s, doc, 0, &[]), out),
        None => die(format!("No schema '{name}' in api '{api}'")),
    }
}

// --------------------------------------------------------------------------- //
// Commands: call
// --------------------------------------------------------------------------- //
fn cmd_call(specs: &[(String, Value)], a: &CallArgs, out: &Out, debug: bool) {
    let (api_name, method, path, op) = find_operation(specs, &a.operation, a.api.as_deref());
    let method = method.to_uppercase();
    let doc = &specs.iter().find(|(n, _)| *n == api_name).unwrap().1;

    // Classify declared parameters.
    let mut path_names: Vec<String> = Vec::new();
    let mut query_names: Vec<String> = Vec::new();
    if let Some(ps) = op.get("parameters").and_then(Value::as_array) {
        for p in ps {
            let p = resolve_refs(p, doc, 0, &[]);
            let name = p.get("name").and_then(Value::as_str).map(String::from);
            match (p.get("in").and_then(Value::as_str), name) {
                (Some("path"), Some(n)) => path_names.push(n),
                (Some("query"), Some(n)) => query_names.push(n),
                _ => {}
            }
        }
    }

    let mut explicit_path = parse_kv(&a.path_param);
    let mut explicit_query = parse_kv(&a.query);
    let headers_extra = parse_kv(&a.header);
    let has_body_decl = op.get("requestBody").is_some();

    let mut body_params = Map::new();
    for (key, val) in parse_kv(&a.param) {
        if path_names.contains(&key) {
            explicit_path.insert(key, val);
        } else if query_names.contains(&key) || !has_body_decl {
            explicit_query.insert(key, val);
        } else {
            body_params.insert(key, val);
        }
    }

    let body_value: Option<Value> = if let Some(raw) = &a.body {
        Some(read_body(raw))
    } else if !body_params.is_empty() {
        Some(Value::Object(body_params))
    } else {
        None
    };

    // Substitute path params.
    let mut url_path = path.clone();
    for name in &path_names {
        let token = format!("{{{name}}}");
        if url_path.contains(&token) {
            let val = explicit_path.get(name).unwrap_or_else(|| {
                die(format!(
                    "Missing required path parameter '{name}'. Pass -p {name}=VALUE"
                ))
            });
            url_path = url_path.replace(&token, &percent_encode(&scalar_to_string(val)));
        }
    }
    let base_url = server_url(doc) + &url_path;

    let api_key = resolve_api_key(&a.api_key);
    let data = body_value
        .as_ref()
        .map(|b| serde_json::to_string(b).unwrap());
    let headers = base_headers(api_key.as_deref(), data.is_some(), headers_extra);

    // Dry run: show the request, send nothing.
    if a.dry_run {
        let url = with_query(&base_url, &explicit_query);
        emit(
            &json!({ "dryRun": true, "request": preview(&method, &url, &headers, &body_value) }),
            out,
        );
        return;
    }
    if !headers.contains_key("Authorization") {
        die("No API key. Run `stedi configure`, set $STEDI_API_KEY, or pass --api-key (or use --dry-run to preview).");
    }

    let ex = Exec {
        max_retries: a.max_retries,
        timeout: a.timeout,
        debug,
    };

    if a.paginate {
        if method != "GET" {
            die("--paginate only applies to GET operations");
        }
        paginate(&base_url, &explicit_query, &headers, &ex, out);
        return;
    }
    if a.watch {
        if method != "GET" {
            die("--watch only applies to GET operations");
        }
        watch(&base_url, &explicit_query, &headers, &ex, a, out);
        return;
    }

    let url = with_query(&base_url, &explicit_query);
    let (status, raw) = send(&method, &url, &headers, data.as_deref(), &ex);
    finish_call(
        status,
        &raw,
        a.verbose
            .then(|| preview(&method, &url, &headers, &body_value)),
        out,
    );
}

fn with_query(base: &str, query: &Map<String, Value>) -> String {
    let qs = build_query(query);
    if qs.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{qs}")
    }
}

fn preview(method: &str, url: &str, headers: &Map<String, Value>, body: &Option<Value>) -> Value {
    let shown: Map<String, Value> = headers
        .iter()
        .map(|(k, v)| {
            let val = if k.eq_ignore_ascii_case("authorization") {
                Value::String("<redacted>".into())
            } else {
                v.clone()
            };
            (k.clone(), val)
        })
        .collect();
    json!({ "method": method, "url": url, "headers": Value::Object(shown), "body": body })
}

fn finish_call(status: u16, raw: &str, request: Option<Value>, out: &Out) {
    let parsed = parse_response(raw);
    let ok = (200..300).contains(&status);
    let mut result = json!({ "status": status, "ok": ok, "body": parsed });
    if let Some(req) = request {
        result["request"] = req;
    }
    emit(&result, out);
    if !ok {
        exit(2);
    }
}

/// Follow `nextPageToken`, streaming each item of the first array field as NDJSON.
fn paginate(
    base: &str,
    query: &Map<String, Value>,
    headers: &Map<String, Value>,
    ex: &Exec,
    out: &Out,
) {
    let mut q = query.clone();
    let mut total = 0usize;
    loop {
        let url = with_query(base, &q);
        let (status, raw) = send("GET", &url, headers, None, ex);
        if !(200..300).contains(&status) {
            finish_call(status, &raw, None, out); // prints error + exits 2
        }
        let body = parse_response(&raw);
        let items = body
            .as_object()
            .and_then(|o| o.values().find(|v| v.is_array()))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| die("--paginate: response has no array field to stream"));
        for item in &items {
            emit(
                item,
                &Out {
                    format: OutputFormat::Compact,
                    jq: out.jq.clone(),
                },
            );
            total += 1;
        }
        match body
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
        {
            Some(token) => {
                q.insert("pageToken".into(), Value::String(token.to_string()));
            }
            None => break,
        }
    }
    eprintln!("[stedi] paginated {total} item(s)");
}

/// Re-issue a GET until the watched field reaches a terminal status (or timeout).
fn watch(
    base: &str,
    query: &Map<String, Value>,
    headers: &Map<String, Value>,
    ex: &Exec,
    a: &CallArgs,
    out: &Out,
) {
    let url = with_query(base, query);
    let terminals: Vec<String> = if a.watch_until.is_empty() {
        TERMINAL_STATUSES.iter().map(|s| s.to_string()).collect()
    } else {
        a.watch_until.iter().map(|s| s.to_lowercase()).collect()
    };
    let deadline =
        (a.watch_timeout > 0.0).then(|| Instant::now() + Duration::from_secs_f64(a.watch_timeout));

    loop {
        let (status, raw) = send("GET", &url, headers, None, ex);
        if !(200..300).contains(&status) {
            finish_call(status, &raw, None, out);
        }
        let body = parse_response(&raw);
        let current = get_path(&body, &a.watch_field).map(scalar_to_string);
        match &current {
            Some(s) if terminals.contains(&s.to_lowercase()) => {
                finish_call(status, &raw, None, out);
                return;
            }
            Some(s) => eprintln!("[stedi] {}={s}; waiting…", a.watch_field),
            None => die(format!(
                "--watch: field '{}' not found in response. Use --watch-field to point at the status field.",
                a.watch_field
            )),
        }
        if let Some(d) = deadline {
            if Instant::now() >= d {
                eprintln!(
                    "[stedi] watch timed out after {}s; printing last result",
                    a.watch_timeout
                );
                finish_call(status, &raw, None, out);
                return;
            }
        }
        sleep(Duration::from_secs_f64(a.watch_interval));
    }
}

// --------------------------------------------------------------------------- //
// Commands: raw get/post/delete
// --------------------------------------------------------------------------- //
fn cmd_raw(specs: &[(String, Value)], method: &str, a: &RawArgs, out: &Out, debug: bool) {
    let base_url = if a.path.starts_with("http://") || a.path.starts_with("https://") {
        a.path.clone()
    } else {
        let api = a.api.as_deref().unwrap_or_else(|| {
            die("Provide --api <group> to resolve a path, or pass a full URL. See `stedi apis`.")
        });
        let doc = &specs
            .iter()
            .find(|(n, _)| n == api)
            .unwrap_or_else(|| die(format!("Unknown api '{api}'. See `stedi apis`.")))
            .1;
        let path = if a.path.starts_with('/') {
            a.path.clone()
        } else {
            format!("/{}", a.path)
        };
        server_url(doc) + &path
    };

    let query = parse_kv(&a.param);
    let body_value = a.body.as_ref().map(|raw| read_body(raw));
    let data = body_value
        .as_ref()
        .map(|b| serde_json::to_string(b).unwrap());
    let api_key = resolve_api_key(&a.api_key);
    let headers = base_headers(api_key.as_deref(), data.is_some(), parse_kv(&a.header));
    if !headers.contains_key("Authorization") {
        die("No API key. Run `stedi configure`, set $STEDI_API_KEY, or pass --api-key.");
    }
    let ex = Exec {
        max_retries: a.max_retries,
        timeout: a.timeout,
        debug,
    };

    if a.paginate {
        if method != "GET" {
            die("--paginate only applies to GET");
        }
        paginate(&base_url, &query, &headers, &ex, out);
        return;
    }
    let url = with_query(&base_url, &query);
    let (status, raw) = send(method, &url, &headers, data.as_deref(), &ex);
    finish_call(status, &raw, None, out);
}

// --------------------------------------------------------------------------- //
// Commands: configure, completions, man
// --------------------------------------------------------------------------- //
fn cmd_configure(api_key: &Option<String>, show_path: bool, out: &Out) {
    let path = config_path()
        .unwrap_or_else(|| die("Cannot determine config path (set $HOME or $STEDI_CONFIG)"));
    if show_path {
        emit(&json!({ "configPath": path.to_string_lossy() }), out);
        return;
    }
    let key = match api_key {
        Some(k) => k.clone(),
        None => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).ok();
            let k = buf.trim().to_string();
            if k.is_empty() {
                die("No API key provided. Pass --api-key or pipe it on stdin.");
            }
            k
        }
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            die(format!(
                "Cannot create config dir '{}': {e}",
                parent.display()
            ));
        }
    }
    let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
    if let Err(e) = std::fs::write(&path, format!("api_key = \"{escaped}\"\n")) {
        die(format!("Cannot write config '{}': {e}", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    emit(
        &json!({ "ok": true, "configPath": path.to_string_lossy() }),
        out,
    );
}

fn cmd_completions(shell: clap_complete::Shell) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "stedi", &mut std::io::stdout());
}

fn cmd_man(dir: Option<PathBuf>) {
    let cmd = Cli::command();
    match dir {
        None => {
            let man = clap_mangen::Man::new(cmd);
            man.render(&mut std::io::stdout())
                .unwrap_or_else(|e| die(format!("man render failed: {e}")));
        }
        Some(dir) => {
            std::fs::create_dir_all(&dir)
                .unwrap_or_else(|e| die(format!("cannot create {}: {e}", dir.display())));
            clap_mangen::generate_to(cmd, &dir)
                .unwrap_or_else(|e| die(format!("man generation failed: {e}")));
            eprintln!("[stedi] wrote man pages to {}", dir.display());
        }
    }
}

// --------------------------------------------------------------------------- //
// main
// --------------------------------------------------------------------------- //
fn main() {
    let cli = Cli::parse();
    let out = Out {
        format: cli.output,
        jq: cli.jq.clone(),
    };

    // Commands that don't need specs.
    match &cli.command {
        Command::Configure { api_key, show_path } => {
            return cmd_configure(api_key, *show_path, &out)
        }
        Command::Completions { shell } => return cmd_completions(*shell),
        Command::Man { dir } => return cmd_man(dir.clone()),
        _ => {}
    }

    let specs = load_specs();
    if specs.is_empty() {
        die("No OpenAPI specs embedded in this binary.");
    }
    match &cli.command {
        Command::Apis => cmd_apis(&specs, &out),
        Command::Ops { api, search } => cmd_ops(&specs, api.as_deref(), search.as_deref(), &out),
        Command::Describe { operation, api } => {
            cmd_describe(&specs, operation, api.as_deref(), &out)
        }
        Command::Schema { name, api } => cmd_schema(&specs, name, api.as_deref(), &out),
        Command::Call(a) => cmd_call(&specs, a, &out, cli.debug),
        Command::Get(a) => cmd_raw(&specs, "GET", a, &out, cli.debug),
        Command::Post(a) => cmd_raw(&specs, "POST", a, &out, cli.debug),
        Command::Delete(a) => cmd_raw(&specs, "DELETE", a, &out, cli.debug),
        Command::Configure { .. } | Command::Completions { .. } | Command::Man { .. } => {
            unreachable!()
        }
    }
}
