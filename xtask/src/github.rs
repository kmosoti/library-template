use crate::gh_http::{gh_api_json, gh_api_status_and_body, gh_jq};
use crate::github_plan::{GithubSetupOpts, build_setup_plan};
use crate::{github_readback, util};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum BootstrapAction {
    AlreadyAligned,
    AlignOrphan { expected_old_sha: String },
    KeepRelatedHistory,
}

pub fn run_github_setup(root: &Path, opts: &GithubSetupOpts) -> Result<(), String> {
    let (reviewer_id, reviewer_source) = resolve_reviewer(root, opts)?;
    let plan = build_setup_plan(opts, reviewer_id, &reviewer_source);
    if opts.dry_run {
        println!(
            "{}",
            serde_json::to_string_pretty(&plan).map_err(|e| e.to_string())?
        );
        return Ok(());
    }

    let repo = &opts.repo;
    let default_branch = gh_jq(root, &format!("repos/{repo}"), ".default_branch")?;
    if default_branch != "main" {
        return Err(format!(
            "repository `{repo}` default_branch is `{default_branch}`, expected `main`"
        ));
    }

    reconcile_develop_agent_branch(root, repo)?;
    gh_api_json(root, "PATCH", &format!("repos/{repo}"), &plan.repo_patch)?;

    for env_plan in &plan.environments {
        let ep = format!("repos/{repo}/environments/{}", env_plan.name);
        gh_api_json(root, "PUT", &ep, &env_plan.payload)?;
        reconcile_env_policies(root, &ep, &env_plan.custom_branch_policies)?;
    }

    reconcile_rulesets(root, repo, &plan.rulesets)?;
    github_readback::fetch_and_verify_readback(root, repo, reviewer_id)?;
    println!("[ok] configured and verified GitHub governance for {repo}");
    Ok(())
}

fn resolve_reviewer(root: &Path, opts: &GithubSetupOpts) -> Result<(u64, String), String> {
    if let Some(id) = opts.reviewer_id {
        return Ok((id, "cli:--reviewer-id".to_string()));
    }
    if let Ok(raw) = gh_jq(root, "user", ".id")
        && let Ok(id) = raw.parse::<u64>()
    {
        return Ok((id, "gh-api-user".to_string()));
    }
    if opts.dry_run {
        return Ok((1, "dry-run-placeholder".to_string()));
    }
    Err("unable to resolve GitHub reviewer user ID; authenticate `gh` or pass `--reviewer-id <u64>`".to_string())
}

fn reconcile_develop_agent_branch(root: &Path, repo: &str) -> Result<(), String> {
    let main_sha = gh_jq(
        root,
        &format!("repos/{repo}/git/ref/heads/main"),
        ".object.sha",
    )?;
    let dev_ep = format!("repos/{repo}/git/ref/heads/develop-agent");
    let (status, body) = gh_api_status_and_body(root, &dev_ep)?;
    if status == 404 {
        return gh_api_json(
            root,
            "POST",
            &format!("repos/{repo}/git/refs"),
            &json!({ "ref": "refs/heads/develop-agent", "sha": main_sha }),
        );
    }
    if status != 200 {
        return Err(format!(
            "inspecting `develop-agent` ref failed with HTTP {status}: {body}"
        ));
    }
    let ref_val: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let dev_sha = ref_val
        .pointer("/object/sha")
        .and_then(Value::as_str)
        .ok_or("missing `object.sha` on `develop-agent` ref")?;
    if dev_sha == main_sha {
        return Ok(());
    }

    let m_tree = gh_jq(
        root,
        &format!("repos/{repo}/git/commits/{main_sha}"),
        ".tree.sha",
    )?;
    let d_raw = util::capture_cmd(
        root,
        "gh",
        &["api", &format!("repos/{repo}/git/commits/{dev_sha}")],
    )?;
    let d_commit: Value = serde_json::from_str(&d_raw).map_err(|e| e.to_string())?;
    let d_tree = d_commit
        .pointer("/tree/sha")
        .and_then(Value::as_str)
        .unwrap_or("");
    let parents_empty = d_commit
        .get("parents")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty);

    let related_status = if parents_empty {
        None
    } else {
        let (c_code, c_body) = gh_api_status_and_body(
            root,
            &format!("repos/{repo}/compare/{dev_sha}...{main_sha}"),
        )?;
        (c_code == 200)
            .then(|| serde_json::from_str::<Value>(&c_body).ok())
            .flatten()
            .and_then(|v| {
                v.get("status")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
    };

    match decide_bootstrap_action(
        &main_sha,
        dev_sha,
        &m_tree,
        d_tree,
        parents_empty,
        related_status.as_deref(),
    )? {
        BootstrapAction::AlreadyAligned | BootstrapAction::KeepRelatedHistory => Ok(()),
        BootstrapAction::AlignOrphan { expected_old_sha } => {
            let current_sha = gh_jq(root, &dev_ep, ".object.sha")?;
            if current_sha != expected_old_sha {
                return Err(format!(
                    "expected-old-SHA lease failed on `develop-agent`: `{current_sha}` != `{expected_old_sha}`"
                ));
            }
            gh_api_json(
                root,
                "PATCH",
                &dev_ep,
                &json!({ "sha": main_sha, "force": true }),
            )
        }
    }
}

pub fn decide_bootstrap_action(
    main_sha: &str,
    dev_sha: &str,
    main_tree: &str,
    dev_tree: &str,
    dev_parents_empty: bool,
    compare_status: Option<&str>,
) -> Result<BootstrapAction, String> {
    if main_sha == dev_sha {
        return Ok(BootstrapAction::AlreadyAligned);
    }
    if dev_parents_empty {
        return (!main_tree.is_empty() && main_tree == dev_tree)
            .then(|| BootstrapAction::AlignOrphan {
                expected_old_sha: dev_sha.to_string(),
            })
            .ok_or_else(|| {
                "orphan `develop-agent` root tree differs from `main`; refusing force alignment"
                    .to_string()
            });
    }
    match compare_status {
        Some("ahead" | "behind" | "identical" | "diverged") => {
            Ok(BootstrapAction::KeepRelatedHistory)
        }
        _ => Err(
            "`develop-agent` has unrelated history relative to `main`; refusing bootstrap"
                .to_string(),
        ),
    }
}

fn reconcile_env_policies(root: &Path, env_ep: &str, desired: &[Value]) -> Result<(), String> {
    if desired.is_empty() {
        return Ok(());
    }
    let list_ep = format!("{env_ep}/deployment-branch-policies");
    let raw = util::capture_cmd(
        root,
        "gh",
        &["api", "--paginate", &list_ep, "--jq", ".branch_policies[]"],
    )?;
    let existing: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    for ex in &existing {
        let keep = desired
            .iter()
            .any(|w| w.get("name") == ex.get("name") && w.get("type") == ex.get("type"));
        if !keep && let Some(id) = ex.get("id").and_then(Value::as_u64) {
            util::run_cmd(
                root,
                "gh",
                &["api", "-X", "DELETE", &format!("{list_ep}/{id}")],
            )?;
        }
    }
    for want in desired {
        if !existing
            .iter()
            .any(|e| e.get("name") == want.get("name") && e.get("type") == want.get("type"))
        {
            gh_api_json(root, "POST", &list_ep, want)?;
        }
    }
    Ok(())
}

fn reconcile_rulesets(root: &Path, repo: &str, desired: &[Value]) -> Result<(), String> {
    let ep = format!("repos/{repo}/rulesets");
    let raw = util::capture_cmd(root, "gh", &["api", "--paginate", &ep, "--jq", ".[]"])?;
    let existing: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for rs in desired {
        let name = rs.get("name").and_then(Value::as_str).unwrap_or("");
        if let Some(id) = existing
            .iter()
            .find(|e| e.get("name").and_then(Value::as_str) == Some(name))
            .and_then(|e| e.get("id"))
            .and_then(Value::as_u64)
        {
            gh_api_json(root, "PUT", &format!("{ep}/{id}"), rs)?;
        } else {
            gh_api_json(root, "POST", &ep, rs)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gh_http::parse_http_response;

    #[test]
    fn distinguishes_http_404_from_auth_error_and_guards_orphan_alignment() {
        assert_eq!(
            parse_http_response("HTTP/2.0 404 Not Found\n\n{\"message\":\"Not Found\"}"),
            Some((404, "{\"message\":\"Not Found\"}".to_string()))
        );
        assert_eq!(
            parse_http_response("HTTP/2.0 403 Forbidden\n\n{}"),
            Some((403, "{}".to_string()))
        );
        assert_eq!(
            decide_bootstrap_action("aaa", "bbb", "tree1", "tree1", true, None).unwrap(),
            BootstrapAction::AlignOrphan {
                expected_old_sha: "bbb".to_string()
            }
        );
        assert!(decide_bootstrap_action("aaa", "bbb", "tree1", "tree2", true, None).is_err());
        assert_eq!(
            decide_bootstrap_action("aaa", "bbb", "tree1", "tree1", false, Some("diverged"))
                .unwrap(),
            BootstrapAction::KeepRelatedHistory
        );
        assert!(decide_bootstrap_action("aaa", "bbb", "tree1", "tree1", false, None).is_err());
    }
}
