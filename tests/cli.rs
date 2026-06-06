//! Integration tests driving the compiled `stedi` binary.
//! These exercise spec reflection, routing, and config behavior without
//! making any network calls (network paths are covered by `--dry-run`).

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

fn stedi() -> Command {
    Command::cargo_bin("stedi").unwrap()
}

#[test]
fn version_and_help() {
    stedi().arg("--version").assert().success();
    stedi().arg("--help").assert().success();
}

#[test]
fn apis_lists_all_seven_groups() {
    let out = stedi().arg("apis").output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 7, "expected 7 embedded specs");
    // Every entry has the discovery fields agents rely on.
    for api in arr {
        assert!(api.get("api").and_then(Value::as_str).is_some());
        assert!(api.get("server").and_then(Value::as_str).is_some());
        assert!(api.get("operations").and_then(Value::as_u64).unwrap() > 0);
    }
}

#[test]
fn ops_search_filters() {
    let out = stedi()
        .args(["ops", "--search", "eligibility"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v.as_array().unwrap();
    assert!(!arr.is_empty());
    for op in arr {
        let hay = format!("{op}").to_lowercase();
        assert!(hay.contains("eligibility"));
    }
}

#[test]
fn describe_reports_params() {
    let out = stedi().args(["describe", "GetBatch"]).output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["method"], "GET");
    let params = v["parameters"].as_array().unwrap();
    assert!(params
        .iter()
        .any(|p| p["name"] == "batchId" && p["in"] == "path"));
}

#[test]
fn ambiguous_operation_is_rejected() {
    // GetPayerRecord exists in both `healthcare` and `payers`.
    stedi()
        .args(["describe", "GetPayerRecord"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ambiguous"));
}

#[test]
fn api_qualifier_disambiguates() {
    stedi()
        .args(["describe", "payers:GetPayerRecord"])
        .assert()
        .success();
}

#[test]
fn dry_run_routes_path_and_query() {
    let out = stedi()
        .args([
            "call",
            "GetBatch",
            "-p",
            "batchId=abc123",
            "-p",
            "pageSize=50",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let req = &v["request"];
    assert_eq!(req["method"], "GET");
    let url = req["url"].as_str().unwrap();
    assert!(
        url.contains("/eligibility-manager/batch/abc123"),
        "path param substituted: {url}"
    );
    assert!(
        url.contains("pageSize=50"),
        "unknown param on GET -> query: {url}"
    );
    // A GET must not carry a body.
    assert!(req["body"].is_null());
}

#[test]
fn dry_run_post_body_from_inline_json() {
    let out = stedi()
        .args([
            "call",
            "EligibilityCheck",
            "--body",
            r#"{"hello":"world"}"#,
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["request"]["method"], "POST");
    assert_eq!(v["request"]["body"]["hello"], "world");
}

#[test]
fn missing_path_param_errors() {
    stedi()
        .args(["call", "GetBatch", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Missing required path parameter"));
}

#[test]
fn auth_header_redacted_in_preview() {
    let out = stedi()
        .args(["call", "payers:ListPayerRecords", "--dry-run"])
        .env("STEDI_API_KEY", "supersecret")
        .output()
        .unwrap();
    let body = String::from_utf8(out.stdout).unwrap();
    assert!(body.contains("<redacted>"));
    assert!(
        !body.contains("supersecret"),
        "secret must never appear in output"
    );
}

#[test]
fn configure_writes_and_resolves_key() {
    let dir = std::env::temp_dir().join(format!("stedi-test-{}", std::process::id()));
    let cfg = dir.join("config.toml");
    let _ = std::fs::remove_dir_all(&dir);

    stedi()
        .args(["configure", "--api-key", "key-from-config"])
        .env("STEDI_CONFIG", &cfg)
        .assert()
        .success();
    assert!(cfg.exists());

    // With no env var, the call should fall back to the config file's key.
    // We can't see the key (redacted), but resolution succeeding means the
    // request is attempted rather than the "No API key" error.
    let out = stedi()
        .args(["call", "payers:ListPayerRecords", "--dry-run"])
        .env_remove("STEDI_API_KEY")
        .env("STEDI_CONFIG", &cfg)
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    // Authorization header present (redacted) => key resolved from config.
    assert!(v["request"]["headers"].get("Authorization").is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn version_includes_build_metadata() {
    let out = stedi().arg("--version").output().unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    // e.g. "stedi 0.2.0 (abc1234, 2026-06-06)" — has a parenthesized build stamp.
    assert!(
        s.contains('(') && s.contains(')'),
        "version lacks build metadata: {s}"
    );
}

#[test]
fn jq_filters_output() {
    let out = stedi().args(["apis", "--jq", ".[].api"]).output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("\"healthcare\""));
    assert!(s.contains("\"payers\""));
    // jq projected to scalars, so the full object keys shouldn't appear.
    assert!(!s.contains("\"operations\""));
}

#[test]
fn table_output_renders_columns() {
    let out = stedi().args(["apis", "-o", "table"]).output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    // comfy-table draws box-drawing borders and includes our column headers.
    assert!(s.contains("api") && s.contains("operations"));
    assert!(s.contains('┌') || s.contains('|'));
}

#[test]
fn compact_output_is_single_line() {
    let out = stedi()
        .args(["apis", "-o", "compact", "--jq", ".[0].api"])
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    assert_eq!(s.trim(), "\"claims\"");
}

#[test]
fn completions_generate() {
    for shell in ["bash", "zsh", "fish", "powershell"] {
        stedi().args(["completions", shell]).assert().success();
    }
}

#[test]
fn man_page_generates() {
    let out = stedi().arg("man").output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains(".TH stedi"));
}

#[test]
fn paginate_rejects_non_get() {
    stedi()
        .args(["call", "EligibilityCheck", "--paginate", "--body", "{}"])
        .env("STEDI_API_KEY", "x")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--paginate only applies to GET"));
}

#[test]
fn raw_get_requires_api_for_relative_path() {
    stedi()
        .args(["get", "/payers"])
        .env("STEDI_API_KEY", "x")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--api"));
}

#[test]
fn body_from_stdin() {
    let out = stedi()
        .args(["call", "EligibilityCheck", "--body", "@-", "--dry-run"])
        .env("STEDI_API_KEY", "x")
        .write_stdin(r#"{"fromStdin":true}"#)
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["request"]["body"]["fromStdin"], true);
}
