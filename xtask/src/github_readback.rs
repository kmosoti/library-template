use crate::util;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

pub struct EnvReadback {
    pub env_json: Value,
    pub branch_policies: Vec<Value>,
}

pub fn fetch_and_verify_readback(
    root: &Path,
    repo: &str,
    expected_reviewer_id: u64,
) -> Result<(), String> {
    let ep = format!("repos/{repo}/rulesets");
    let list_raw = util::capture_cmd(root, "gh", &["api", "--paginate", &ep, "--jq", ".[]"])?;
    let summaries: Vec<Value> = list_raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut detailed_rulesets = Vec::new();
    for item in summaries {
        if let Some(id) = item.get("id").and_then(Value::as_u64) {
            let full_raw = util::capture_cmd(root, "gh", &["api", &format!("{ep}/{id}")])?;
            detailed_rulesets.push(serde_json::from_str(&full_raw).map_err(|e| e.to_string())?);
        }
    }

    let mut envs = BTreeMap::new();
    for (name, has_custom) in [
        ("main-release", false),
        ("stable-publish", true),
        ("tag-release", true),
    ] {
        let env_raw = util::capture_cmd(
            root,
            "gh",
            &["api", &format!("repos/{repo}/environments/{name}")],
        )?;
        let env_json: Value = serde_json::from_str(&env_raw).map_err(|e| e.to_string())?;
        let branch_policies = if has_custom {
            let pol_ep = format!("repos/{repo}/environments/{name}/deployment-branch-policies");
            let pol_raw = util::capture_cmd(
                root,
                "gh",
                &["api", "--paginate", &pol_ep, "--jq", ".branch_policies[]"],
            )?;
            pol_raw
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(serde_json::from_str)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        } else {
            Vec::new()
        };
        envs.insert(
            name.to_string(),
            EnvReadback {
                env_json,
                branch_policies,
            },
        );
    }

    verify_typed_readback(&detailed_rulesets, &envs, expected_reviewer_id)
}

pub fn verify_typed_readback(
    rulesets: &[Value],
    envs: &BTreeMap<String, EnvReadback>,
    expected_reviewer_id: u64,
) -> Result<(), String> {
    let find_rs = |name: &str| -> Result<&Value, String> {
        let rs = rulesets
            .iter()
            .find(|r| r.get("name").and_then(Value::as_str) == Some(name))
            .ok_or_else(|| format!("readback missing ruleset `{name}`"))?;
        if rs.get("enforcement").and_then(Value::as_str) != Some("active") {
            return Err(format!("ruleset `{name}` is not `active`"));
        }
        Ok(rs)
    };

    let main_rs = find_rs("library-template-main-protection")?;
    require_empty_bypass(main_rs, "library-template-main-protection")?;
    require_merge_methods(main_rs, &["merge"])?;
    require_deployments(main_rs, &["main-release"])?;

    let agent_rs = find_rs("library-template-agent-branch-protection")?;
    require_empty_bypass(agent_rs, "library-template-agent-branch-protection")?;
    require_rule_type(agent_rs, "required_linear_history")?;
    require_merge_methods(agent_rs, &["rebase"])?;

    let maint_rs = find_rs("library-template-maintenance-protection")?;
    require_empty_bypass(maint_rs, "library-template-maintenance-protection")?;
    require_rule_type(maint_rs, "required_linear_history")?;
    require_merge_methods(maint_rs, &["rebase"])?;
    require_deployments(maint_rs, &["main-release"])?;

    let immut_rs = find_rs("library-template-tag-immutability")?;
    require_empty_bypass(immut_rs, "library-template-tag-immutability")?;
    for rt in ["update", "deletion", "non_fast_forward"] {
        require_rule_type(immut_rs, rt)?;
    }

    let create_rs = find_rs("library-template-tag-creation")?;
    require_rule_type(create_rs, "creation")?;
    let bypass = create_rs
        .get("bypass_actors")
        .and_then(Value::as_array)
        .ok_or("missing `bypass_actors` on tag-creation")?;
    if bypass.len() != 1 || bypass[0].get("actor_id") != Some(&json!(5)) {
        return Err(
            "`library-template-tag-creation` must restrict bypass strictly to actor_id=5"
                .to_string(),
        );
    }

    verify_env(envs, "main-release", expected_reviewer_id, None)?;
    verify_env(
        envs,
        "stable-publish",
        expected_reviewer_id,
        Some(("v*", "tag")),
    )?;
    verify_env(
        envs,
        "tag-release",
        expected_reviewer_id,
        Some(("main", "branch")),
    )?;
    Ok(())
}

fn require_empty_bypass(rs: &Value, name: &str) -> Result<(), String> {
    let empty = rs
        .get("bypass_actors")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty);
    empty
        .then_some(())
        .ok_or_else(|| format!("ruleset `{name}` must have empty `bypass_actors`"))
}

fn require_rule_type(rs: &Value, rule_type: &str) -> Result<(), String> {
    let ok = rs
        .get("rules")
        .and_then(Value::as_array)
        .is_some_and(|arr| {
            arr.iter()
                .any(|r| r.get("type").and_then(Value::as_str) == Some(rule_type))
        });
    ok.then_some(())
        .ok_or_else(|| format!("ruleset missing required rule `{rule_type}`"))
}

fn require_merge_methods(rs: &Value, expected: &[&str]) -> Result<(), String> {
    let methods = find_rule_param(rs, "pull_request", "allowed_merge_methods")
        .and_then(Value::as_array)
        .ok_or("missing `allowed_merge_methods`")?;
    let actual: Vec<&str> = methods.iter().filter_map(Value::as_str).collect();
    (actual == expected)
        .then_some(())
        .ok_or_else(|| format!("expected merge methods {expected:?}, got {actual:?}"))
}

fn require_deployments(rs: &Value, expected: &[&str]) -> Result<(), String> {
    let envs = find_rule_param(
        rs,
        "required_deployments",
        "required_deployment_environments",
    )
    .and_then(Value::as_array)
    .ok_or("missing `required_deployment_environments`")?;
    let actual: Vec<&str> = envs.iter().filter_map(Value::as_str).collect();
    (actual == expected)
        .then_some(())
        .ok_or_else(|| format!("expected required_deployments {expected:?}, got {actual:?}"))
}

fn find_rule_param<'a>(rs: &'a Value, rtype: &str, param: &str) -> Option<&'a Value> {
    rs.get("rules")?
        .as_array()?
        .iter()
        .find(|r| r.get("type").and_then(Value::as_str) == Some(rtype))?
        .get("parameters")?
        .get(param)
}

fn verify_env(
    envs: &BTreeMap<String, EnvReadback>,
    name: &str,
    reviewer_id: u64,
    expected_policy: Option<(&str, &str)>,
) -> Result<(), String> {
    let entry = envs
        .get(name)
        .ok_or_else(|| format!("readback missing environment `{name}`"))?;
    let has_reviewer = entry
        .env_json
        .get("protection_rules")
        .and_then(Value::as_array)
        .is_some_and(|rules| {
            rules.iter().any(|r| {
                r.get("reviewers")
                    .and_then(Value::as_array)
                    .is_some_and(|revs| {
                        revs.iter().any(|rv| {
                            rv.pointer("/reviewer/id") == Some(&json!(reviewer_id))
                                || rv.get("id") == Some(&json!(reviewer_id))
                        })
                    })
            })
        });
    if !has_reviewer {
        return Err(format!(
            "environment `{name}` readback missing expected reviewer_id={reviewer_id}"
        ));
    }
    match expected_policy {
        None => {
            if !entry.env_json["deployment_branch_policy"].is_null() {
                return Err(format!(
                    "`{name}` must have `deployment_branch_policy: null`"
                ));
            }
        }
        Some((pat, kind)) => {
            if entry.branch_policies.len() != 1
                || entry.branch_policies[0].get("name").and_then(Value::as_str) != Some(pat)
                || entry.branch_policies[0].get("type").and_then(Value::as_str) != Some(kind)
            {
                return Err(format!(
                    "`{name}` branch_policies mismatch (expected sole `{kind}:{pat}`)"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github_plan::{GithubSetupOpts, build_setup_plan};

    #[test]
    fn accepts_valid_readback_and_rejects_mismatched_bypass_or_stale_policies() {
        let opts = GithubSetupOpts {
            repo: "example-org/library-template".to_string(),
            dry_run: true,
            reviewer_id: Some(77),
            prevent_self_review: false,
            main_pr_approvals: 1,
            require_codeowners: false,
        };
        let plan = build_setup_plan(&opts, 77, "cli");
        let mk_env = |dp: Value, policies: Vec<Value>| EnvReadback {
            env_json: json!({
                "deployment_branch_policy": dp,
                "protection_rules": [{ "type": "required_reviewers", "reviewers": [{ "type": "User", "reviewer": { "id": 77 } }] }]
            }),
            branch_policies: policies,
        };
        let mut envs = BTreeMap::new();
        envs.insert("main-release".to_string(), mk_env(Value::Null, vec![]));
        envs.insert(
            "stable-publish".to_string(),
            mk_env(
                json!({"custom_branch_policies": true}),
                vec![json!({"name":"v*","type":"tag"})],
            ),
        );
        envs.insert(
            "tag-release".to_string(),
            mk_env(
                json!({"custom_branch_policies": true}),
                vec![json!({"name":"main","type":"branch"})],
            ),
        );
        assert!(verify_typed_readback(&plan.rulesets, &envs, 77).is_ok());
        assert!(verify_typed_readback(&plan.rulesets, &envs, 999).is_err());

        // Stale unpruned extra policy on tag-release must fail readback!
        envs.get_mut("tag-release")
            .unwrap()
            .branch_policies
            .push(json!({"name": "develop-agent", "type": "branch"}));
        assert!(verify_typed_readback(&plan.rulesets, &envs, 77).is_err());
    }
}
