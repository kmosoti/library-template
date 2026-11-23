use crate::util;
use std::fs;
use std::path::Path;

const REQUIRED_WORKFLOWS: &[&str] = &[
    ".github/workflows/ci.yml",
    ".github/workflows/main-human-gate.yml",
    ".github/workflows/sync-develop-agent.yml",
    ".github/workflows/release.yml",
    ".github/workflows/cut-release-tag.yml",
];

pub fn check_workflow_pins(root: &Path) -> Result<(), String> {
    for rel in REQUIRED_WORKFLOWS {
        if !root.join(rel).is_file() {
            return Err(format!("missing required workflow `{rel}`"));
        }
    }
    let wf_dir = root.join(".github/workflows");
    let entries =
        fs::read_dir(&wf_dir).map_err(|e| format!("failed to read `{}`: {e}", wf_dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("dir entry error: {e}"))?.path();
        if !matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("yml" | "yaml")
        ) {
            continue;
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
        validate_workflow_yaml(&path.to_string_lossy(), &content)?;
    }
    Ok(())
}

pub fn validate_workflow_yaml(label: &str, content: &str) -> Result<(), String> {
    for (idx, raw) in content.lines().enumerate() {
        let line_no = idx + 1;
        let code = raw.split('#').next().unwrap_or("").trim();
        if code.contains(".sh") || code.contains(".py") {
            return Err(format!(
                "`{label}:{line_no}` references forbidden `.sh` or `.py` script"
            ));
        }
        let maybe_uses = code
            .strip_prefix("uses:")
            .or_else(|| code.strip_prefix("- uses:"));
        if let Some(rest) = maybe_uses {
            let target = rest.trim().trim_matches(['"', '\'']);
            if target.starts_with("./") {
                continue;
            }
            let Some((action, rev)) = target.split_once('@') else {
                return Err(format!("`{label}:{line_no}` invalid `uses: {target}`"));
            };
            let valid_sha = rev.len() == 40
                && rev
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if action.is_empty() || !valid_sha {
                return Err(format!(
                    "`{label}:{line_no}` action `{target}` must be pinned to a 40-char lowercase hex SHA"
                ));
            }
        }
    }
    Ok(())
}

pub fn check_privacy(root: &Path) -> Result<(), String> {
    for path in util::collect_repo_files(root)? {
        let rel = path.strip_prefix(root).unwrap_or(&path);
        if matches!(path.extension().and_then(|s| s.to_str()), Some("sh" | "py")) {
            return Err(format!(
                "forbidden script file `{}` (zero `.sh` or `.py` allowed)",
                rel.display()
            ));
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(finding) = scan_text_privacy(&text) {
            return Err(format!(
                "privacy/secret violation in `{}`: {finding}",
                rel.display()
            ));
        }
    }
    Ok(())
}

pub fn scan_text_privacy(text: &str) -> Option<&'static str> {
    let home_unix = "/home/";
    let home_mac = "/Users/";
    let pem_open = "-----BEGIN ";
    let pem_close = "PRIVATE KEY-----";
    for line in text.lines() {
        if has_host_home_path(line, home_unix) || has_host_home_path(line, home_mac) {
            return Some("contains absolute host home directory path");
        }
        if line.contains(pem_open) && line.contains(pem_close) {
            return Some("contains PEM private key header");
        }
        if has_prefixed_token(line, "ghp_", 20, |b| b.is_ascii_alphanumeric())
            || has_prefixed_token(line, "github_pat_", 20, |b| {
                b.is_ascii_alphanumeric() || b == b'_'
            })
            || has_prefixed_token(line, "AKIA", 16, |b| {
                b.is_ascii_uppercase() || b.is_ascii_digit()
            })
            || has_prefixed_token(line, "ya29.", 20, |b| {
                b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')
            })
            || has_prefixed_token(line, "sk-ant-", 16, |b| {
                b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')
            })
            || has_slack_token(line)
        {
            return Some("contains raw credential/API token pattern");
        }
    }
    None
}

fn has_host_home_path(line: &str, prefix: &str) -> bool {
    let mut rest = line;
    while let Some(idx) = rest.find(prefix) {
        let after = &rest[idx + prefix.len()..];
        if let Some(slash) = after.find('/') {
            let seg = &after[..slash];
            if !seg.is_empty()
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            {
                return true;
            }
        }
        rest = after;
    }
    false
}

fn has_prefixed_token(line: &str, prefix: &str, min_tail: usize, pred: fn(u8) -> bool) -> bool {
    let mut rest = line;
    while let Some(idx) = rest.find(prefix) {
        let after = &rest[idx + prefix.len()..];
        let count = after.bytes().take_while(|b| pred(*b)).count();
        if count >= min_tail {
            return true;
        }
        rest = after;
    }
    false
}

fn has_slack_token(line: &str) -> bool {
    for kind in ["xoxb-", "xoxa-", "xoxp-", "xoxr-", "xoxs-"] {
        if has_prefixed_token(line, kind, 10, |b| b.is_ascii_alphanumeric() || b == b'-') {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_structural_paths_and_tokens_without_false_positives_on_prose() {
        assert!(scan_text_privacy(&format!("/{}/alice/project", "home")).is_some());
        assert!(scan_text_privacy(&format!("/{}/dev_1/work", "Users")).is_some());
        assert!(scan_text_privacy(&format!("-----BEGIN {}-----", "OPENSSH PRIVATE KEY")).is_some());
        assert!(scan_text_privacy(&format!("token ghp_{}", "a".repeat(24))).is_some());
        assert!(scan_text_privacy(&format!("aws AKIA{}", "0".repeat(16))).is_some());
        assert!(scan_text_privacy("prose mentioning `ghp_` or `/home/<seg>/` rules").is_none());
    }
}
