#![forbid(unsafe_code)]

mod agent_sync;
mod api_parser;
mod api_surface;
mod architecture;
mod gh_http;
mod github;
mod github_plan;
mod github_readback;
mod governance;
mod hook_policy;
mod hooks;
mod knowledge;
mod package;
mod release;
mod rust_lex;
mod sync_branches;
mod trust_boundary;
mod util;
mod verify;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match dispatch(&args) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("xtask error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(args: &[String]) -> Result<ExitCode, String> {
    let root = util::workspace_root()?;
    let Some(cmd) = args.first().map(String::as_str) else {
        return Err(usage());
    };
    let rest = &args[1..];

    match cmd {
        "verify" => {
            let force = parse_verify_flags(rest)?;
            verify::run_verify(&root, force)?;
            Ok(ExitCode::SUCCESS)
        }
        "check-architecture" => {
            architecture::check_architecture(&root)?;
            println!("[ok] check-architecture");
            Ok(ExitCode::SUCCESS)
        }
        "check-api-surface" => {
            let base = parse_optional_flag(rest, "--base")?;
            api_surface::check_api_surface(&root, base.as_deref())?;
            println!("[ok] check-api-surface");
            Ok(ExitCode::SUCCESS)
        }
        "check-trust-boundary" => {
            let base = parse_optional_flag(rest, "--base")?;
            trust_boundary::check_trust_boundary(&root, base.as_deref())?;
            println!("[ok] check-trust-boundary");
            Ok(ExitCode::SUCCESS)
        }
        "check-knowledge" => {
            knowledge::check_knowledge(&root)?;
            println!("[ok] check-knowledge");
            Ok(ExitCode::SUCCESS)
        }
        "check-workflow-pins" => {
            governance::check_workflow_pins(&root)?;
            println!("[ok] check-workflow-pins");
            Ok(ExitCode::SUCCESS)
        }
        "check-agent-configs" => {
            agent_sync::check_agent_configs(&root)?;
            println!("[ok] check-agent-configs");
            Ok(ExitCode::SUCCESS)
        }
        "check-line-caps" => {
            let max_lines = architecture::check_line_caps(&root)?;
            println!("[ok] check-line-caps (max_rs_lines={max_lines})");
            Ok(ExitCode::SUCCESS)
        }
        "check-privacy" => {
            governance::check_privacy(&root)?;
            println!("[ok] check-privacy");
            Ok(ExitCode::SUCCESS)
        }
        "check-release-lineage" => {
            let tag = parse_optional_flag(rest, "--tag")?;
            release::check_release_lineage(&root, tag.as_deref())?;
            println!("[ok] check-release-lineage");
            Ok(ExitCode::SUCCESS)
        }
        "sync-branches" => {
            let apply = parse_sync_flags(rest)?;
            sync_branches::run_sync_branches(&root, apply)?;
            Ok(ExitCode::SUCCESS)
        }
        "preflight-tag" => {
            let [tag] = rest else {
                return Err("usage: cargo xtask preflight-tag <tag>".to_string());
            };
            release::preflight_tag(&root, tag)?;
            Ok(ExitCode::SUCCESS)
        }
        "package" => {
            let out = parse_optional_flag(rest, "--out")?.unwrap_or_else(|| "dist".to_string());
            package::package_release(&root, &PathBuf::from(out))?;
            Ok(ExitCode::SUCCESS)
        }
        "hook" => hooks::run_hook_cli(&root, rest),
        "github-setup" => {
            let opts = github_plan::parse_github_setup_args(rest)?;
            github::run_github_setup(&root, &opts)?;
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(usage()),
    }
}

fn parse_verify_flags(args: &[String]) -> Result<bool, String> {
    match args {
        [] => Ok(false),
        [flag] if flag == "--force" => Ok(true),
        _ => Err("usage: cargo xtask verify [--force]".to_string()),
    }
}

fn parse_sync_flags(args: &[String]) -> Result<bool, String> {
    match args {
        [] => Ok(false),
        [flag] if flag == "--check" => Ok(false),
        [flag] if flag == "--apply" => Ok(true),
        _ => Err("usage: cargo xtask sync-branches [--check | --apply]".to_string()),
    }
}

fn parse_optional_flag(args: &[String], flag: &str) -> Result<Option<String>, String> {
    match args {
        [] => Ok(None),
        [f, val] if f == flag => Ok(Some(val.clone())),
        _ => Err(format!("expected optional `{flag} <value>`")),
    }
}

fn usage() -> String {
    "usage: cargo xtask <verify [--force] | check-architecture | check-api-surface [--base <ref>] | \
check-trust-boundary [--base <ref>] | check-knowledge | check-workflow-pins | \
check-agent-configs | check-line-caps | check-privacy | check-release-lineage [--tag <tag>] | \
sync-branches [--check | --apply] | preflight-tag <tag> | package [--out <dir>] | \
hook pre-tool-use --format <antigravity|claude|codex|copilot> | \
github-setup <OWNER/REPO> [--dry-run] [--reviewer-id <u64>] [--prevent-self-review] [--main-pr-approvals <u32>] [--require-codeowners]>"
        .to_string()
}
