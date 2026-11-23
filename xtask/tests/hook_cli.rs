#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::env;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn run_hook_in_dir(cwd: &Path, format: &str, stdin_raw: &str) -> (i32, Value) {
    let bin = env!("CARGO_BIN_EXE_xtask");
    let mut child = Command::new(bin)
        .current_dir(cwd)
        .args(["hook", "pre-tool-use", "--format", format])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn xtask hook");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin_raw.as_bytes())
        .expect("failed writing stdin");

    let out = child.wait_with_output().expect("failed waiting on xtask");
    let code = out.status.code().unwrap_or(-1);
    let stdout_str = String::from_utf8(out.stdout).expect("invalid utf-8 stdout");
    let parsed: Value =
        serde_json::from_str(stdout_str.trim()).expect("hook stdout must be valid JSON");
    (code, parsed)
}

#[test]
fn hooks_work_from_tmp_and_docs_across_all_four_formats() {
    let ws_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let ws_root = ws_path.to_string_lossy().to_string();
    let docs_dir = ws_path.join("docs");

    for fmt in ["antigravity", "claude", "codex", "copilot"] {
        let neutral = json!({
            "cwd": ws_root,
            "toolCall": { "name": "run_command", "args": { "CommandLine": "pwd" } }
        })
        .to_string();
        let (code, allow_body) = run_hook_in_dir(&env::temp_dir(), fmt, &neutral);
        assert_eq!(code, 0);
        if fmt == "antigravity" {
            assert_eq!(allow_body, json!({ "decision": "allow" }));
        } else {
            assert_eq!(allow_body, json!({}));
        }

        for cmd in [
            "git push origin v0.2.0",
            "git push origin main",
            "gh pr merge 1 --squash",
        ] {
            let deny_input = json!({
                "cwd": docs_dir.to_string_lossy(),
                "toolCall": { "name": "run_command", "args": { "CommandLine": cmd } }
            })
            .to_string();
            let (d_code, deny_body) = run_hook_in_dir(&docs_dir, fmt, &deny_input);
            assert_eq!(d_code, 0, "format {fmt} must exit 0 on deny");
            assert_denied(fmt, &deny_body);
        }

        let (m_code, mal_body) = run_hook_in_dir(&env::temp_dir(), fmt, "{not-valid-json");
        assert_eq!(m_code, 0, "format {fmt} must exit 0 on malformed JSON");
        assert_denied(fmt, &mal_body);
    }
}

fn assert_denied(fmt: &str, body: &Value) {
    match fmt {
        "antigravity" => assert_eq!(body["decision"], json!("deny")),
        "claude" | "codex" => {
            assert!(
                body.get("decision").is_none(),
                "{fmt} schema must not emit top-level `decision`"
            );
            assert_eq!(
                body.pointer("/hookSpecificOutput/permissionDecision"),
                Some(&json!("deny"))
            );
        }
        "copilot" => assert_eq!(body["permissionDecision"], json!("deny")),
        _ => unreachable!(),
    }
}
