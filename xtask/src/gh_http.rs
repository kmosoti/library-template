use crate::util;
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn gh_jq(root: &Path, endpoint: &str, jq_expr: &str) -> Result<String, String> {
    util::capture_cmd(root, "gh", &["api", endpoint, "--jq", jq_expr])
}

pub fn gh_api_status_and_body(cwd: &Path, endpoint: &str) -> Result<(u16, String), String> {
    let out = Command::new("gh")
        .current_dir(cwd)
        .args(["api", "-i", endpoint])
        .output()
        .map_err(|e| format!("failed to execute `gh api -i {endpoint}`: {e}"))?;
    parse_http_response(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| format!("unable to parse HTTP status from `gh api -i {endpoint}`"))
}

pub fn parse_http_response(raw: &str) -> Option<(u16, String)> {
    let normalized = raw.replace("\r\n", "\n");
    let (headers, body) = normalized.split_once("\n\n")?;
    let status_line = headers.lines().find(|l| l.starts_with("HTTP/"))?;
    let code: u16 = status_line.split_whitespace().nth(1)?.parse().ok()?;
    Some((code, body.trim().to_string()))
}

pub fn gh_api_json(
    cwd: &Path,
    method: &str,
    endpoint: &str,
    payload: &Value,
) -> Result<(), String> {
    let body = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    let mut child = Command::new("gh")
        .current_dir(cwd)
        .args(["api", "--method", method, endpoint, "--input", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn `gh api`: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write as _;
        stdin.write_all(&body).map_err(|e| e.to_string())?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("`gh api -X {method} {endpoint}` failed with {status}"))
}
