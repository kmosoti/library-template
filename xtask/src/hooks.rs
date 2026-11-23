use crate::hook_policy::{TagPushVerifier, WorkspaceTagVerifier, check_shell_command};
use serde_json::{Value, json};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookFormat {
    Antigravity,
    Claude,
    Codex,
    Copilot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookOutcome {
    pub allowed: bool,
    pub stdout_json: String,
}

pub fn run_hook_cli(fallback_root: &Path, args: &[String]) -> Result<ExitCode, String> {
    let format = match args {
        [event, flag, fmt] if event == "pre-tool-use" && flag == "--format" => match fmt.as_str() {
            "antigravity" => HookFormat::Antigravity,
            "claude" => HookFormat::Claude,
            "codex" => HookFormat::Codex,
            "copilot" => HookFormat::Copilot,
            _ => return Err(format!("unsupported hook format `{fmt}`")),
        },
        _ => {
            return Err(
                "usage: cargo xtask hook pre-tool-use --format <antigravity|claude|codex|copilot>"
                    .to_string(),
            );
        }
    };

    let mut payload = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut payload) {
        let out = render_outcome(format, Some(format!("failed reading hook stdin: {e}")));
        println!("{}", out.stdout_json);
        return Ok(ExitCode::SUCCESS);
    }

    let Ok(val) = serde_json::from_str::<Value>(&payload) else {
        let out = render_outcome(
            format,
            Some("malformed PreToolUse hook JSON payload; failing closed".to_string()),
        );
        println!("{}", out.stdout_json);
        return Ok(ExitCode::SUCCESS);
    };

    let resolved_root = resolve_hook_workspace_root(&val, fallback_root);
    let verifier = WorkspaceTagVerifier {
        base_root: resolved_root,
    };
    let outcome = evaluate_parsed_payload(format, &val, &verifier);
    println!("{}", outcome.stdout_json);
    Ok(ExitCode::SUCCESS)
}

pub fn resolve_hook_workspace_root(val: &Value, fallback: &Path) -> PathBuf {
    let candidate = val
        .get("cwd")
        .or_else(|| val.pointer("/workspacePaths/0"))
        .or_else(|| val.pointer("/toolCall/args/Directory"))
        .or_else(|| val.pointer("/tool_input/cwd"))
        .and_then(Value::as_str);

    if let Some(dir_str) = candidate {
        let mut cur = PathBuf::from(dir_str);
        loop {
            if cur.join("Cargo.toml").is_file() && cur.join("xtask/Cargo.toml").is_file() {
                return cur;
            }
            if !cur.pop() {
                break;
            }
        }
    }
    fallback.to_path_buf()
}

#[cfg(test)]
pub fn evaluate_hook_payload(
    format: HookFormat,
    raw_json: &str,
    verifier: &dyn TagPushVerifier,
) -> HookOutcome {
    let Ok(val) = serde_json::from_str::<Value>(raw_json) else {
        return render_outcome(
            format,
            Some("malformed PreToolUse hook JSON payload; failing closed".to_string()),
        );
    };
    evaluate_parsed_payload(format, &val, verifier)
}

fn evaluate_parsed_payload(
    format: HookFormat,
    val: &Value,
    verifier: &dyn TagPushVerifier,
) -> HookOutcome {
    let Some(command) = extract_command(val) else {
        return render_outcome(format, None);
    };
    match check_shell_command(&command, verifier) {
        Ok(()) => render_outcome(format, None),
        Err(reason) => render_outcome(format, Some(reason)),
    }
}

fn extract_command(v: &Value) -> Option<String> {
    if let Some(s) = v
        .pointer("/toolCall/args/CommandLine")
        .or_else(|| v.pointer("/toolCall/args/command"))
        .or_else(|| v.pointer("/tool_input/command"))
        .or_else(|| v.pointer("/tool_input/CommandLine"))
        .or_else(|| v.pointer("/arguments/CommandLine"))
        .or_else(|| v.pointer("/arguments/command"))
        .or_else(|| v.pointer("/input/command"))
        .or_else(|| v.pointer("/toolArgs/command"))
        .or_else(|| v.get("command"))
        .and_then(Value::as_str)
    {
        return Some(s.to_string());
    }
    if let Some(arr) = v
        .pointer("/tool_input/command")
        .or_else(|| v.get("command"))
        .and_then(Value::as_array)
    {
        let joined: Vec<&str> = arr.iter().filter_map(Value::as_str).collect();
        if !joined.is_empty() {
            return Some(joined.join(" "));
        }
    }
    if let Some(args_str) = v.get("toolArgs").and_then(Value::as_str)
        && let Ok(nested) = serde_json::from_str::<Value>(args_str)
        && let Some(cmd) = nested.get("command").and_then(Value::as_str)
    {
        return Some(cmd.to_string());
    }
    None
}

fn render_outcome(format: HookFormat, deny_reason: Option<String>) -> HookOutcome {
    let allowed = deny_reason.is_none();
    let stdout_json = match (format, deny_reason) {
        (HookFormat::Antigravity, None) => json!({ "decision": "allow" }).to_string(),
        (HookFormat::Antigravity, Some(reason)) => {
            json!({ "decision": "deny", "reason": reason }).to_string()
        }
        (HookFormat::Claude | HookFormat::Codex | HookFormat::Copilot, None) => {
            json!({}).to_string()
        }
        (HookFormat::Claude | HookFormat::Codex, Some(reason)) => json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason
            }
        })
        .to_string(),
        (HookFormat::Copilot, Some(reason)) => json!({
            "permissionDecision": "deny",
            "permissionDecisionReason": reason
        })
        .to_string(),
    };
    HookOutcome {
        allowed,
        stdout_json,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release;

    struct MockVerifier;

    impl TagPushVerifier for MockVerifier {
        fn verify_pushed_tag(
            &self,
            _repo_override: Option<&str>,
            tag: &str,
            _src_ref: Option<&str>,
        ) -> Result<(), String> {
            let parsed = release::parse_release_tag(tag)?;
            (parsed.version == "0.1.0")
                .then_some(())
                .ok_or_else(|| "version mismatch".to_string())
        }
    }

    #[test]
    fn enforces_four_formats_neutral_empty_json_and_fail_closed_malformed_stdin() {
        for fmt in [
            HookFormat::Antigravity,
            HookFormat::Claude,
            HookFormat::Codex,
            HookFormat::Copilot,
        ] {
            let bad = evaluate_hook_payload(fmt, "{not-valid-json", &MockVerifier);
            assert!(!bad.allowed);

            let neutral =
                evaluate_hook_payload(fmt, r#"{"tool_input":{"command":"pwd"}}"#, &MockVerifier);
            assert!(neutral.allowed);
            if fmt != HookFormat::Antigravity {
                assert_eq!(neutral.stdout_json, "{}");
            }
        }
        let compound = r#"{"toolCall":{"name":"run_command","args":{"CommandLine":"(env FOO=1 git -C . push origin main)"}}}"#;
        assert!(!evaluate_hook_payload(HookFormat::Antigravity, compound, &MockVerifier).allowed);
    }
}
