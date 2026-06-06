//! stedi — a single-binary, agent-friendly CLI for the Stedi APIs.
//!
//! The official Stedi OpenAPI documents are embedded at compile time
//! (`include_str!`) and reflected at runtime, so the binary never drifts from
//! the specs and needs no files on disk. Designed to be driven by LLM agents:
//! every command emits machine-readable JSON, operations are fully
//! discoverable, and `call --dry-run` previews a request before it is sent.
//!
//! Operations are referenced by operationId. When the same id exists in more
//! than one API, qualify it as `api:OperationId` or pass `--api`.
//!
//! Auth precedence: `--api-key` > `$STEDI_API_KEY` > config file
//! (`$XDG_CONFIG_HOME/stedi/config.toml`, default `~/.config/stedi/config.toml`).

use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;

use clap::{Parser, Subcommand};
use serde_json::{json, Map, Value};

const HTTP_METHODS: [&str; 7] = ["get", "post", "put", "delete", "patch", "head", "options"];

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

// --------------------------------------------------------------------------- //
// CLI definition
// --------------------------------------------------------------------------- //
#[derive(Parser)]
#[command(
    name = "stedi",
    version,
    about = "Agent-friendly CLI for the Stedi APIs. All output is JSON.",
    long_about = "Agent-friendly CLI for the Stedi APIs, driven by the official \
OpenAPI specs (embedded at build time). All output is JSON.\n\n\
Auth precedence: --api-key > $STEDI_API_KEY > ~/.config/stedi/config.toml"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
        /// Disambiguate when the id spans APIs.
        #[arg(long)]
        api: Option<String>,
    },
    /// Dump a named component schema (refs inlined).
    Schema {
        /// Schema name under components.schemas
        name: String,
        /// Which API the schema belongs to.
        #[arg(long)]
        api: Option<String>,
    },
    /// Execute an operation against the live API.
    Call {
        /// operationId or api:operationId
        operation: String,
        /// Disambiguate when the id spans APIs.
        #[arg(long)]
        api: Option<String>,
        /// Parameter, auto-routed to path/query/body per the spec. Repeatable.
        /// Values are parsed as JSON when possible.
        #[arg(short = 'p', long = "param", value_name = "KEY=VALUE")]
        param: Vec<String>,
        /// Force a path parameter. Repeatable.
        #[arg(long = "path", value_name = "KEY=VALUE")]
        path_param: Vec<String>,
        /// Force a query parameter. Repeatable.
        #[arg(long, value_name = "KEY=VALUE")]
        query: Vec<String>,
        /// Extra request header. Repeatable.
        #[arg(long, value_name = "KEY=VALUE")]
        header: Vec<String>,
        /// Full JSON request body, or @path to a JSON file.
        #[arg(long, value_name = "JSON|@file")]
        body: Option<String>,
        /// Override the resolved API key.
        #[arg(long = "api-key")]
        api_key: Option<String>,
        /// Print the request that would be sent; do not send it.
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// Include the request in the response output.
        #[arg(long)]
        verbose: bool,
        /// Seconds (default 60).
        #[arg(long, default_value_t = 60.0)]
        timeout: f64,
    },
    /// Save an API key to the config file for future use.
    Configure {
        /// The API key to store. If omitted, read from stdin (non-interactive).
        #[arg(long = "api-key")]
        api_key: Option<String>,
        /// Print the resolved config path and exit without writing.
        #[arg(long)]
        show_path: bool,
    },
}

// --------------------------------------------------------------------------- //
// Helpers
// --------------------------------------------------------------------------- //
fn die(msg: impl AsRef<str>) -> ! {
    eprintln!("{}", json!({ "error": msg.as_ref() }));
    exit(1);
}

fn emit(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap());
}

/// Path to the config file: $XDG_CONFIG_HOME/stedi/config.toml, else
/// ~/.config/stedi/config.toml. Honors $STEDI_CONFIG for an explicit override.
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

/// Read `api_key` from the config file, if present and parseable.
fn config_api_key() -> Option<String> {
    let path = config_path()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let parsed: toml::Value = text.parse().ok()?;
    parsed
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Resolve the API key by precedence: explicit flag > env > config file.
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

/// Load and parse every embedded spec, preserving declaration order.
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

/// Yield (api, method, path, operation) for every operation in every spec.
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

/// Resolve an operationId (optionally `api:OperationId`) to its definition.
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
            api_filter.as_ref().map_or(true, |f| f == api)
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

/// Recursively inline local `$ref`s so schemas are self-contained and readable.
/// Guards against cycles and runaway depth.
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

/// Parse repeated `key=value` flags. Values are JSON-parsed when possible.
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

fn server_url(doc: &Value) -> String {
    doc.get("servers")
        .and_then(Value::as_array)
        .and_then(|s| s.first())
        .and_then(|s| s.get("url"))
        .and_then(Value::as_str)
        .map(|u| u.trim_end_matches('/').to_string())
        .unwrap_or_else(|| die("Spec has no servers[].url to build a request from"))
}

/// A scalar Value rendered for use in a URL or header.
fn scalar_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Percent-encode a string (everything but RFC 3986 unreserved chars).
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

// --------------------------------------------------------------------------- //
// Commands
// --------------------------------------------------------------------------- //
fn cmd_apis(specs: &[(String, Value)]) {
    let out: Vec<Value> = specs
        .iter()
        .map(|(api, doc)| {
            let info = doc.get("info").cloned().unwrap_or_default();
            let ops = doc
                .get("paths")
                .and_then(Value::as_object)
                .map(|p| {
                    p.values()
                        .filter_map(Value::as_object)
                        .map(|item| {
                            item.keys()
                                .filter(|m| HTTP_METHODS.contains(&m.to_lowercase().as_str()))
                                .count()
                        })
                        .sum::<usize>()
                })
                .unwrap_or(0);
            json!({
                "api": api,
                "title": info.get("title"),
                "version": info.get("version"),
                "server": doc.get("servers").and_then(Value::as_array)
                    .and_then(|s| s.first()).and_then(|s| s.get("url")),
                "operations": ops,
            })
        })
        .collect();
    emit(&Value::Array(out));
}

fn cmd_ops(specs: &[(String, Value)], api: Option<&str>, search: Option<&str>) {
    let term = search.unwrap_or("").to_lowercase();
    let mut out: Vec<Value> = Vec::new();
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
        out.push(json!({
            "api": a,
            "operationId": opid,
            "method": method.to_uppercase(),
            "path": path,
            "summary": summary,
        }));
    }
    out.sort_by(|x, y| {
        let key = |v: &Value| {
            (
                v["api"].as_str().unwrap_or("").to_string(),
                v["path"].as_str().unwrap_or("").to_string(),
            )
        };
        key(x).cmp(&key(y))
    });
    emit(&Value::Array(out));
}

fn cmd_describe(specs: &[(String, Value)], operation: &str, api: Option<&str>) {
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
                        "required": p.get("required").and_then(Value::as_bool)
                            .unwrap_or(in_ == "path"),
                        "description": p.get("description"),
                        "schema": p.get("schema"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let body = op.get("requestBody").map(|rb| {
        let rb = resolve_refs(rb, doc, 0, &[]);
        let content = rb
            .get("content")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let ctype = if content.contains_key("application/json") {
            Some("application/json".to_string())
        } else {
            content.keys().next().cloned()
        };
        let schema = ctype
            .as_ref()
            .and_then(|c| content.get(c))
            .and_then(|c| c.get("schema"))
            .cloned();
        json!({
            "required": rb.get("required").and_then(Value::as_bool).unwrap_or(false),
            "contentType": ctype,
            "schema": schema,
        })
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

    emit(&json!({
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
    }));
}

fn cmd_schema(specs: &[(String, Value)], name: &str, api: Option<&str>) {
    let schemas_of = |doc: &Value| -> Option<Value> {
        doc.pointer(&format!(
            "/components/schemas/{}",
            name.replace('~', "~0").replace('/', "~1")
        ))
        .cloned()
    };

    let api = match api {
        Some(a) => a.to_string(),
        None => {
            let hits: Vec<&String> = specs
                .iter()
                .filter(|(_, doc)| schemas_of(doc).is_some())
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
        Some(s) => emit(&resolve_refs(&s, doc, 0, &[])),
        None => die(format!("No schema '{name}' in api '{api}'")),
    }
}

fn cmd_configure(api_key: &Option<String>, show_path: bool) {
    let path = config_path()
        .unwrap_or_else(|| die("Cannot determine config path (set $HOME or $STEDI_CONFIG)"));
    if show_path {
        emit(&json!({ "configPath": path.to_string_lossy() }));
        return;
    }

    // Resolve the key: flag, else stdin (supports `echo $KEY | stedi configure`).
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
    // Escape the key for TOML basic-string syntax.
    let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
    let contents = format!("api_key = \"{escaped}\"\n");
    if let Err(e) = std::fs::write(&path, contents) {
        die(format!("Cannot write config '{}': {e}", path.display()));
    }
    // Best-effort tighten permissions on Unix (file holds a secret).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    emit(&json!({ "ok": true, "configPath": path.to_string_lossy() }));
}

fn cmd_call(specs: &[(String, Value)], c: &Command) {
    let Command::Call {
        operation,
        api,
        param,
        path_param,
        query,
        header,
        body,
        api_key,
        dry_run,
        verbose,
        timeout,
    } = c
    else {
        unreachable!()
    };

    let (api_name, method, path, op) = find_operation(specs, operation, api.as_deref());
    let doc = &specs.iter().find(|(a, _)| *a == api_name).unwrap().1;

    // Classify the spec's declared parameters.
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

    let mut explicit_path = parse_kv(path_param);
    let mut explicit_query = parse_kv(query);
    let mut headers = parse_kv(header);

    // Auto-route generic -p/--param values. An unrecognized param goes to the
    // body only when the op declares one; otherwise it is a query param.
    let has_body = op.get("requestBody").is_some();
    let mut body_params = Map::new();
    for (key, val) in parse_kv(param) {
        if path_names.contains(&key) {
            explicit_path.insert(key, val);
        } else if query_names.contains(&key) || !has_body {
            explicit_query.insert(key, val);
        } else {
            body_params.insert(key, val);
        }
    }

    // Build the request body: explicit --body wins, else assembled -p values.
    let body_value: Option<Value> = if let Some(raw) = body {
        let text = if let Some(fp) = raw.strip_prefix('@') {
            std::fs::read_to_string(fp)
                .unwrap_or_else(|e| die(format!("Cannot read body file '{fp}': {e}")))
        } else {
            raw.clone()
        };
        Some(
            serde_json::from_str(&text)
                .unwrap_or_else(|e| die(format!("--body is not valid JSON: {e}"))),
        )
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

    let mut url = server_url(doc) + &url_path;
    // Append query params (list values become repeated keys).
    let mut qs: Vec<(String, String)> = Vec::new();
    for (k, v) in &explicit_query {
        match v {
            Value::Array(items) => {
                for item in items {
                    qs.push((k.clone(), scalar_to_string(item)));
                }
            }
            other => qs.push((k.clone(), scalar_to_string(other))),
        }
    }
    if !qs.is_empty() {
        let encoded: Vec<String> = qs
            .iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect();
        url.push('?');
        url.push_str(&encoded.join("&"));
    }

    // Auth + content headers.
    let key = resolve_api_key(api_key);
    if !headers.contains_key("Authorization") {
        if let Some(k) = &key {
            headers.insert("Authorization".to_string(), Value::String(k.clone()));
        }
    }
    let data = body_value
        .as_ref()
        .map(|b| serde_json::to_string(b).unwrap());
    if data.is_some() {
        headers
            .entry("Content-Type".to_string())
            .or_insert(Value::String("application/json".into()));
    }
    headers
        .entry("Accept".to_string())
        .or_insert(Value::String("application/json".into()));

    let header_preview: Map<String, Value> = headers
        .iter()
        .map(|(k, v)| {
            let shown = if k == "Authorization" {
                Value::String("<redacted>".into())
            } else {
                v.clone()
            };
            (k.clone(), shown)
        })
        .collect();
    let request_preview = json!({
        "method": method.to_uppercase(),
        "url": url,
        "headers": Value::Object(header_preview),
        "body": body_value,
    });

    if *dry_run {
        emit(&json!({ "dryRun": true, "request": request_preview }));
        return;
    }

    if !headers.contains_key("Authorization") {
        die("No API key. Run `stedi configure`, set $STEDI_API_KEY, or pass --api-key (or use --dry-run to preview).");
    }

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs_f64(*timeout))
        .build();
    let mut req = agent.request(&method.to_uppercase(), &url);
    for (k, v) in &headers {
        req = req.set(k, &scalar_to_string(v));
    }
    let result = match &data {
        Some(d) => req.send_string(d),
        None => req.call(),
    };

    let (status, raw): (u16, String) = match result {
        Ok(resp) => {
            let s = resp.status();
            (s, resp.into_string().unwrap_or_default())
        }
        Err(ureq::Error::Status(code, resp)) => (code, resp.into_string().unwrap_or_default()),
        Err(ureq::Error::Transport(e)) => die(format!("Request failed: {e}")),
    };

    let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|_| Value::String(raw.clone()));
    let ok = (200..300).contains(&status);
    let mut result_obj = json!({ "status": status, "ok": ok, "body": parsed });
    if *verbose {
        result_obj["request"] = request_preview;
    }
    emit(&result_obj);
    if !ok {
        exit(2);
    }
}

fn main() {
    let cli = Cli::parse();
    if let Command::Configure { api_key, show_path } = &cli.command {
        cmd_configure(api_key, *show_path);
        return;
    }
    let specs = load_specs();
    if specs.is_empty() {
        die("No OpenAPI specs embedded in this binary.");
    }
    match &cli.command {
        Command::Apis => cmd_apis(&specs),
        Command::Ops { api, search } => cmd_ops(&specs, api.as_deref(), search.as_deref()),
        Command::Describe { operation, api } => cmd_describe(&specs, operation, api.as_deref()),
        Command::Schema { name, api } => cmd_schema(&specs, name, api.as_deref()),
        Command::Call { .. } => cmd_call(&specs, &cli.command),
        Command::Configure { .. } => unreachable!(),
    }
}
