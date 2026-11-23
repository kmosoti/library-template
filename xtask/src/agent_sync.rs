use serde_json::Value;
use std::fs;
use std::path::Path;

const REQUIRED_AGENT_FILES: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "SECURITY.md",
    ".github/copilot-instructions.md",
    ".github/hooks/forge-guard.json",
    ".agents/plugin.json",
    ".agents/hooks.json",
    ".agents/rules/AGENTS.md",
    ".agents/skills/evidence-seeker/SKILL.md",
    ".agents/skills/idempotent-workflow/SKILL.md",
    ".agents/skills/semver-api-ratchet/SKILL.md",
    ".agents/plugins/library-forge/plugin.json",
    ".agents/plugins/library-forge/rules/AGENTS.md",
    ".agents/plugins/library-forge/skills/evidence-seeker/SKILL.md",
    ".agents/plugins/library-forge/skills/idempotent-workflow/SKILL.md",
    ".agents/plugins/library-forge/skills/semver-api-ratchet/SKILL.md",
    ".agents/plugins/library-forge/agents/adversarial-breaker.md",
    ".agents/plugins/library-forge/agents/stack-reviewer.md",
    ".claude/settings.json",
    ".claude/agents/adversarial-breaker.md",
    ".claude/agents/stack-reviewer.md",
    ".claude/skills/evidence-seeker/SKILL.md",
    ".claude/skills/idempotent-workflow/SKILL.md",
    ".claude/skills/semver-api-ratchet/SKILL.md",
    ".codex/hooks.json",
    ".codex/agents/adversarial-breaker.toml",
    ".codex/agents/stack-reviewer.toml",
];

const ALLOWED_PLUGIN_KEYS: &[&str] = &[
    "name",
    "displayName",
    "version",
    "description",
    "suggestedPrompts",
];

const ANTIGRAVITY_HOOK_CMD: &str = "cargo run --quiet --manifest-path ../Cargo.toml --package xtask -- hook pre-tool-use --format antigravity";
const CLAUDE_HOOK_CMD: &str = "cargo run --quiet --manifest-path \"${CLAUDE_PROJECT_DIR:-.}/Cargo.toml\" --package xtask -- hook pre-tool-use --format claude";
const CODEX_HOOK_CMD: &str =
    "cargo run --quiet --package xtask -- hook pre-tool-use --format codex";
const COPILOT_HOOK_CMD: &str =
    "cargo run --quiet --package xtask -- hook pre-tool-use --format copilot";

pub fn check_agent_configs(root: &Path) -> Result<(), String> {
    for rel in REQUIRED_AGENT_FILES {
        let body = fs::read_to_string(root.join(rel))
            .map_err(|e| format!("missing or unreadable agent config `{rel}`: {e}"))?;
        if body.trim().is_empty() {
            return Err(format!("agent config `{rel}` cannot be empty"));
        }
    }

    if root
        .join(".agents/plugins/library-forge/hooks.json")
        .exists()
    {
        return Err(
            "duplicate `.agents/plugins/library-forge/hooks.json` must not exist; keep workspace `.agents/hooks.json` only"
                .to_string(),
        );
    }

    for manifest in [
        ".agents/plugin.json",
        ".agents/plugins/library-forge/plugin.json",
    ] {
        let raw = fs::read_to_string(root.join(manifest)).map_err(|e| e.to_string())?;
        validate_plugin_manifest(manifest, &raw)?;
    }

    let ag_hooks =
        fs::read_to_string(root.join(".agents/hooks.json")).map_err(|e| e.to_string())?;
    validate_antigravity_hooks_json(".agents/hooks.json", &ag_hooks)?;

    let claude_raw =
        fs::read_to_string(root.join(".claude/settings.json")).map_err(|e| e.to_string())?;
    validate_host_json_hook(
        ".claude/settings.json",
        &claude_raw,
        "/hooks/PreToolUse",
        CLAUDE_HOOK_CMD,
    )?;

    let codex_raw =
        fs::read_to_string(root.join(".codex/hooks.json")).map_err(|e| e.to_string())?;
    validate_host_json_hook(
        ".codex/hooks.json",
        &codex_raw,
        "/hooks/PreToolUse",
        CODEX_HOOK_CMD,
    )?;

    let copilot_raw = fs::read_to_string(root.join(".github/hooks/forge-guard.json"))
        .map_err(|e| e.to_string())?;
    validate_copilot_hooks_json(".github/hooks/forge-guard.json", &copilot_raw)?;

    for skill in [
        "evidence-seeker",
        "idempotent-workflow",
        "semver-api-ratchet",
    ] {
        let ag = fs::read_to_string(root.join(format!(".agents/skills/{skill}/SKILL.md")))
            .map_err(|e| e.to_string())?;
        let pf = fs::read_to_string(root.join(format!(
            ".agents/plugins/library-forge/skills/{skill}/SKILL.md"
        )))
        .map_err(|e| e.to_string())?;
        let cl = fs::read_to_string(root.join(format!(".claude/skills/{skill}/SKILL.md")))
            .map_err(|e| e.to_string())?;
        if !ag.starts_with("---\n") || ag != cl || ag != pf {
            return Err(format!(
                "skill `{skill}` must have frontmatter and stay synchronized across `.agents/`, `.agents/plugins/library-forge/`, and `.claude/`"
            ));
        }
    }

    for agent in ["adversarial-breaker", "stack-reviewer"] {
        let ag = fs::read_to_string(
            root.join(format!(".agents/plugins/library-forge/agents/{agent}.md")),
        )
        .map_err(|e| e.to_string())?;
        let cl = fs::read_to_string(root.join(format!(".claude/agents/{agent}.md")))
            .map_err(|e| e.to_string())?;
        let cx = fs::read_to_string(root.join(format!(".codex/agents/{agent}.toml")))
            .map_err(|e| e.to_string())?;
        if !ag.starts_with("---\n") || ag != cl {
            return Err(format!(
                "agent `{agent}.md` must match across `.agents/plugins/library-forge/agents/` and `.claude/agents/`"
            ));
        }
        if !cx.contains("name =") || !cx.contains("description =") {
            return Err(format!(
                "`.codex/agents/{agent}.toml` must define `name` and `description`"
            ));
        }
    }
    Ok(())
}

pub fn validate_plugin_manifest(label: &str, raw: &str) -> Result<(), String> {
    let val: Value = serde_json::from_str(raw).map_err(|e| format!("invalid `{label}`: {e}"))?;
    let obj = val
        .as_object()
        .ok_or_else(|| format!("`{label}` root must be a JSON object"))?;
    for key in obj.keys() {
        if !ALLOWED_PLUGIN_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "`{label}` contains unsupported/out-of-root top-level key `{key}`"
            ));
        }
    }
    for req in ["name", "displayName", "version", "description"] {
        if obj
            .get(req)
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(format!("`{label}` missing non-empty string `{req}`"));
        }
    }
    let Some(prompts) = obj.get("suggestedPrompts").and_then(Value::as_array) else {
        return Err(format!("`{label}` must define `suggestedPrompts` array"));
    };
    if prompts.is_empty()
        || !prompts
            .iter()
            .all(|p| p.as_str().is_some_and(|s| !s.is_empty()))
    {
        return Err(format!(
            "`{label}` `suggestedPrompts` must contain non-empty strings"
        ));
    }
    Ok(())
}

fn validate_copilot_hooks_json(label: &str, raw: &str) -> Result<(), String> {
    let val: Value =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON in `{label}`: {e}"))?;
    let Some(entries) = val.pointer("/hooks/preToolUse").and_then(Value::as_array) else {
        return Err(format!("`{label}` missing `/hooks/preToolUse` array"));
    };
    let ok = entries.iter().any(|e| {
        e.get("cwd").and_then(Value::as_str) == Some(".")
            && e.get("bash").and_then(Value::as_str) == Some(COPILOT_HOOK_CMD)
            && e.get("powershell").and_then(Value::as_str) == Some(COPILOT_HOOK_CMD)
    });
    ok.then_some(()).ok_or_else(|| {
        format!("`{label}` must define `cwd: \".\"`, `bash`, and `powershell` invoking `{COPILOT_HOOK_CMD}`")
    })
}

fn validate_host_json_hook(
    label: &str,
    raw: &str,
    pointer: &str,
    expected_cmd: &str,
) -> Result<(), String> {
    let val: Value =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON in `{label}`: {e}"))?;
    let Some(sub) = val.pointer(pointer) else {
        return Err(format!("`{label}` missing `{pointer}`"));
    };
    if !value_contains_str(sub, expected_cmd) {
        return Err(format!(
            "`{label}` must configure `{pointer}` invoking `{expected_cmd}`"
        ));
    }
    Ok(())
}

fn value_contains_str(v: &Value, target: &str) -> bool {
    match v {
        Value::String(s) => s == target,
        Value::Array(arr) => arr.iter().any(|item| value_contains_str(item, target)),
        Value::Object(map) => map.values().any(|item| value_contains_str(item, target)),
        _ => false,
    }
}

pub fn validate_antigravity_hooks_json(label: &str, raw: &str) -> Result<(), String> {
    let val: Value =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON in `{label}`: {e}"))?;
    let root_obj = val
        .as_object()
        .ok_or_else(|| format!("`{label}` root must be a JSON object"))?;
    if root_obj.is_empty() || root_obj.contains_key("hooks") {
        return Err(format!(
            "`{label}` must map named hook group(s) rather than top-level `hooks`"
        ));
    }
    let mut found = false;
    for (group_name, events_val) in root_obj {
        let events = events_val
            .as_object()
            .ok_or_else(|| format!("`{label}` group `{group_name}` must be an object"))?;
        for ev in events.keys() {
            if !matches!(
                ev.as_str(),
                "SessionStart"
                    | "PreInvocation"
                    | "PostInvocation"
                    | "Stop"
                    | "PreToolUse"
                    | "PostToolUse"
            ) {
                return Err(format!("`{label}` invalid event `{ev}`"));
            }
        }
        if let Some(entries) = events.get("PreToolUse").and_then(Value::as_array) {
            for entry in entries {
                if entry.get("matcher").and_then(Value::as_str) == Some("run_command")
                    && let Some(list) = entry.get("hooks").and_then(Value::as_array)
                    && list.iter().any(|h| {
                        h.get("type").and_then(Value::as_str) == Some("command")
                            && h.get("command").and_then(Value::as_str)
                                == Some(ANTIGRAVITY_HOOK_CMD)
                    })
                {
                    found = true;
                }
            }
        }
    }
    found.then_some(()).ok_or_else(|| {
        format!(
            "`{label}` must register `PreToolUse` `run_command` invoking `{ANTIGRAVITY_HOOK_CMD}`"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_plugin_keys_and_ungrouped_hooks() {
        let bad_plugin = r#"{"name":"x","displayName":"X","version":"0.1.0","description":"d","suggestedPrompts":["a"],"hooks":"./hooks.json"}"#;
        assert!(validate_plugin_manifest("plugin.json", bad_plugin).is_err());

        let bad_hooks = r#"{"hooks":{"pre-tool-use":[{"matcher":"run_command","command":"x"}]}}"#;
        assert!(validate_antigravity_hooks_json("hooks.json", bad_hooks).is_err());
    }
}
