use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct GithubSetupOpts {
    pub repo: String,
    pub dry_run: bool,
    pub reviewer_id: Option<u64>,
    pub prevent_self_review: bool,
    pub main_pr_approvals: u32,
    pub require_codeowners: bool,
}

#[derive(Debug, Serialize)]
pub struct GithubSetupPlan {
    pub repository: String,
    pub reviewer_id: u64,
    pub reviewer_source: String,
    pub repo_patch: Value,
    pub environments: Vec<EnvironmentPlan>,
    pub integration_branch: String,
    pub rulesets: Vec<Value>,
}

#[derive(Debug, Serialize)]
pub struct EnvironmentPlan {
    pub name: String,
    pub payload: Value,
    pub custom_branch_policies: Vec<Value>,
}

pub fn parse_github_setup_args(args: &[String]) -> Result<GithubSetupOpts, String> {
    let mut repo = None;
    let mut dry_run = false;
    let mut reviewer_id = None;
    let mut prevent_self_review = false;
    let mut main_pr_approvals = 1_u32;
    let mut require_codeowners = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dry-run" => dry_run = true,
            "--prevent-self-review" => prevent_self_review = true,
            "--require-codeowners" => require_codeowners = true,
            "--reviewer-id" => {
                i += 1;
                let raw = args.get(i).ok_or("missing value for `--reviewer-id`")?;
                reviewer_id = Some(raw.parse().map_err(|_| "invalid `--reviewer-id` u64")?);
            }
            "--main-pr-approvals" => {
                i += 1;
                let raw = args
                    .get(i)
                    .ok_or("missing value for `--main-pr-approvals`")?;
                main_pr_approvals = raw
                    .parse()
                    .map_err(|_| "invalid `--main-pr-approvals` u32")?;
            }
            arg if !arg.starts_with('-') && repo.is_none() => {
                validate_repo_slug(arg)?;
                repo = Some(arg.to_string());
            }
            other => return Err(format!("unexpected `github-setup` argument `{other}`")),
        }
        i += 1;
    }
    let repo = repo.ok_or_else(|| {
        "usage: cargo xtask github-setup <OWNER/REPO> [--dry-run] [--reviewer-id <u64>] [--prevent-self-review] [--main-pr-approvals <u32>] [--require-codeowners]".to_string()
    })?;
    Ok(GithubSetupOpts {
        repo,
        dry_run,
        reviewer_id,
        prevent_self_review,
        main_pr_approvals,
        require_codeowners,
    })
}

pub fn validate_repo_slug(slug: &str) -> Result<(), String> {
    let Some((owner, repo)) = slug.split_once('/') else {
        return Err(format!("invalid `<OWNER/REPO>` `{slug}`"));
    };
    let ok = |s: &str| {
        !s.is_empty()
            && !s.contains('/')
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    (ok(owner) && ok(repo))
        .then_some(())
        .ok_or_else(|| format!("invalid `<OWNER/REPO>` `{slug}`"))
}

pub fn build_setup_plan(
    opts: &GithubSetupOpts,
    reviewer_id: u64,
    reviewer_source: &str,
) -> GithubSetupPlan {
    let repo_patch = json!({
        "allow_merge_commit": true,
        "allow_squash_merge": false,
        "allow_rebase_merge": true,
        "delete_branch_on_merge": true,
        "allow_auto_merge": true
    });

    let reviewers = json!([{ "type": "User", "id": reviewer_id }]);
    let custom_policy = json!({ "protected_branches": false, "custom_branch_policies": true });
    let environments = vec![
        EnvironmentPlan {
            name: "main-release".to_string(),
            payload: json!({
                "wait_timer": 0,
                "prevent_self_review": opts.prevent_self_review,
                "reviewers": reviewers,
                "deployment_branch_policy": null
            }),
            custom_branch_policies: vec![],
        },
        EnvironmentPlan {
            name: "stable-publish".to_string(),
            payload: json!({
                "wait_timer": 0,
                "prevent_self_review": opts.prevent_self_review,
                "reviewers": reviewers,
                "deployment_branch_policy": custom_policy
            }),
            custom_branch_policies: vec![json!({ "name": "v*", "type": "tag" })],
        },
        EnvironmentPlan {
            name: "tag-release".to_string(),
            payload: json!({
                "wait_timer": 0,
                "prevent_self_review": opts.prevent_self_review,
                "reviewers": reviewers,
                "deployment_branch_policy": custom_policy
            }),
            custom_branch_policies: vec![json!({ "name": "main", "type": "branch" })],
        },
    ];

    let rulesets = vec![
        main_ruleset(opts.main_pr_approvals, opts.require_codeowners),
        agent_branch_ruleset(),
        maintenance_ruleset(opts.main_pr_approvals, opts.require_codeowners),
        tag_immutability_ruleset(),
        tag_creation_ruleset(),
    ];

    GithubSetupPlan {
        repository: opts.repo.clone(),
        reviewer_id,
        reviewer_source: reviewer_source.to_string(),
        repo_patch,
        environments,
        integration_branch: "develop-agent".to_string(),
        rulesets,
    }
}

fn main_ruleset(approvals: u32, require_codeowners: bool) -> Value {
    json!({
        "name": "library-template-main-protection",
        "target": "branch",
        "enforcement": "active",
        "bypass_actors": [],
        "conditions": { "ref_name": { "include": ["refs/heads/main"], "exclude": [] } },
        "rules": [
            { "type": "deletion" },
            { "type": "non_fast_forward" },
            {
                "type": "pull_request",
                "parameters": {
                    "required_approving_review_count": approvals,
                    "dismiss_stale_reviews_on_push": true,
                    "require_code_owner_review": require_codeowners,
                    "require_last_push_approval": false,
                    "required_review_thread_resolution": true,
                    "allowed_merge_methods": ["merge"]
                }
            },
            {
                "type": "required_status_checks",
                "parameters": {
                    "strict_required_status_checks_policy": false,
                    "required_status_checks": [{ "context": "gates" }, { "context": "main-release-gate" }]
                }
            },
            {
                "type": "required_deployments",
                "parameters": { "required_deployment_environments": ["main-release"] }
            }
        ]
    })
}

fn agent_branch_ruleset() -> Value {
    json!({
        "name": "library-template-agent-branch-protection",
        "target": "branch",
        "enforcement": "active",
        "bypass_actors": [],
        "conditions": { "ref_name": { "include": ["refs/heads/develop-agent"], "exclude": [] } },
        "rules": [
            { "type": "deletion" },
            { "type": "non_fast_forward" },
            { "type": "required_linear_history" },
            {
                "type": "pull_request",
                "parameters": {
                    "required_approving_review_count": 0,
                    "dismiss_stale_reviews_on_push": false,
                    "require_code_owner_review": false,
                    "require_last_push_approval": false,
                    "required_review_thread_resolution": true,
                    "allowed_merge_methods": ["rebase"]
                }
            },
            {
                "type": "required_status_checks",
                "parameters": {
                    "strict_required_status_checks_policy": true,
                    "required_status_checks": [{ "context": "gates" }]
                }
            }
        ]
    })
}

fn maintenance_ruleset(approvals: u32, require_codeowners: bool) -> Value {
    json!({
        "name": "library-template-maintenance-protection",
        "target": "branch",
        "enforcement": "active",
        "bypass_actors": [],
        "conditions": { "ref_name": { "include": ["refs/heads/release/*"], "exclude": [] } },
        "rules": [
            { "type": "deletion" },
            { "type": "non_fast_forward" },
            { "type": "required_linear_history" },
            {
                "type": "pull_request",
                "parameters": {
                    "required_approving_review_count": approvals,
                    "dismiss_stale_reviews_on_push": true,
                    "require_code_owner_review": require_codeowners,
                    "require_last_push_approval": false,
                    "required_review_thread_resolution": true,
                    "allowed_merge_methods": ["rebase"]
                }
            },
            {
                "type": "required_status_checks",
                "parameters": {
                    "strict_required_status_checks_policy": false,
                    "required_status_checks": [{ "context": "gates" }, { "context": "main-release-gate" }]
                }
            },
            {
                "type": "required_deployments",
                "parameters": { "required_deployment_environments": ["main-release"] }
            }
        ]
    })
}

fn tag_immutability_ruleset() -> Value {
    json!({
        "name": "library-template-tag-immutability",
        "target": "tag",
        "enforcement": "active",
        "bypass_actors": [],
        "conditions": { "ref_name": { "include": ["refs/tags/v*"], "exclude": [] } },
        "rules": [{ "type": "update" }, { "type": "deletion" }, { "type": "non_fast_forward" }]
    })
}

fn tag_creation_ruleset() -> Value {
    json!({
        "name": "library-template-tag-creation",
        "target": "tag",
        "enforcement": "active",
        "bypass_actors": [{ "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always" }],
        "conditions": { "ref_name": { "include": ["refs/tags/v*"], "exclude": [] } },
        "rules": [{ "type": "creation" }]
    })
}
