use crate::util;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncEvaluation {
    pub state: String,
    pub healthy: bool,
    pub main_sha: String,
    pub develop_sha: String,
}

pub fn run_sync_branches(root: &Path, _apply: bool) -> Result<(), String> {
    let eval = evaluate_sync_state(root)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&eval).map_err(|e| e.to_string())?
    );
    if eval.healthy {
        Ok(())
    } else {
        match eval.state.as_str() {
            "NonAncestorSameTree" => Err(
                "sync-branches failed (`NonAncestorSameTree`): `main` was updated via rebase/squash instead of a `--no-ff` merge commit from `develop-agent`. Reconcile via a linear sync PR onto `develop-agent`."
                    .to_string(),
            ),
            _ => Err(
                "sync-branches failed (`MainHasUnbackportedChanges`): `main` contains commits whose tree is not present in `develop-agent`'s merge-base. Backport `main` changes via a linear PR onto `develop-agent`."
                    .to_string(),
            ),
        }
    }
}

pub fn evaluate_sync_state(root: &Path) -> Result<SyncEvaluation, String> {
    let main_sha = resolve_first(
        root,
        &["refs/remotes/origin/main", "refs/heads/main", "HEAD"],
    )?;
    let dev_sha = resolve_first(
        root,
        &[
            "refs/remotes/origin/develop-agent",
            "refs/heads/develop-agent",
        ],
    )?;

    if main_sha == dev_sha {
        return Ok(SyncEvaluation {
            state: "ExactMatch".to_string(),
            healthy: true,
            main_sha,
            develop_sha: dev_sha,
        });
    }

    let main_tree = tree_of(root, &main_sha)?;
    let dev_tree = tree_of(root, &dev_sha)?;
    let dev_is_ancestor_of_main = util::run_cmd(
        root,
        "git",
        &["merge-base", "--is-ancestor", &dev_sha, &main_sha],
    )
    .is_ok();

    if dev_is_ancestor_of_main && main_tree == dev_tree {
        return Ok(SyncEvaluation {
            state: "PromotedAncestor".to_string(),
            healthy: true,
            main_sha,
            develop_sha: dev_sha,
        });
    }

    if !dev_is_ancestor_of_main
        && let Ok(base_sha) = util::capture_cmd(root, "git", &["merge-base", &main_sha, &dev_sha])
        && let Ok(base_tree) = tree_of(root, &base_sha)
        && base_tree == main_tree
    {
        return Ok(SyncEvaluation {
            state: "AgentAheadOfPromotedBase".to_string(),
            healthy: true,
            main_sha,
            develop_sha: dev_sha,
        });
    }

    let state = if !dev_is_ancestor_of_main && main_tree == dev_tree {
        "NonAncestorSameTree"
    } else {
        "MainHasUnbackportedChanges"
    };
    Ok(SyncEvaluation {
        state: state.to_string(),
        healthy: false,
        main_sha,
        develop_sha: dev_sha,
    })
}

fn resolve_first(root: &Path, candidates: &[&str]) -> Result<String, String> {
    for c in candidates {
        if let Ok(sha) = util::capture_cmd(root, "git", &["rev-parse", "--verify", c]) {
            return Ok(sha);
        }
    }
    Err(format!(
        "unable to resolve any git ref among: {}",
        candidates.join(", ")
    ))
}

fn tree_of(root: &Path, rev: &str) -> Result<String, String> {
    util::capture_cmd(root, "git", &["rev-parse", &format!("{rev}^{{tree}}")])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, process::Command};

    #[test]
    fn verifies_ancestry_model_across_consecutive_promotions_and_rejects_drift() {
        let work = env::temp_dir().join(format!("ancestry-sync-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&work);
        fs::create_dir_all(&work).unwrap();
        let git = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .current_dir(&work)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        git(&["init", "-b", "main"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "user.email", "test@example.invalid"]);
        fs::write(work.join("f.txt"), "0\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "root"]);
        git(&["branch", "develop-agent", "main"]);

        // 1. ExactMatch
        assert_eq!(evaluate_sync_state(&work).unwrap().state, "ExactMatch");
        assert!(run_sync_branches(&work, false).is_ok());

        // 2. Two consecutive promotions WITHOUT moving develop-agent onto merge commits!
        for round in ["1", "2"] {
            git(&["checkout", "develop-agent"]);
            fs::write(work.join("f.txt"), format!("{round}\n")).unwrap();
            git(&["commit", "-am", &format!("agent-{round}")]);
            // While agent is ahead of promoted base:
            assert_eq!(
                evaluate_sync_state(&work).unwrap().state,
                "AgentAheadOfPromotedBase"
            );
            assert!(run_sync_branches(&work, false).is_ok());

            git(&["checkout", "main"]);
            git(&[
                "merge",
                "--no-ff",
                "-m",
                &format!("promote-{round}"),
                "develop-agent",
            ]);
            let post = evaluate_sync_state(&work).unwrap();
            assert_eq!(post.state, "PromotedAncestor");
            assert!(run_sync_branches(&work, false).is_ok());
        }

        // 3. Direct unbackported commit on main -> MainHasUnbackportedChanges (fails non-zero!)
        git(&["checkout", "main"]);
        fs::write(work.join("hotfix.txt"), "urgent\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "hotfix"]);
        assert_eq!(
            evaluate_sync_state(&work).unwrap().state,
            "MainHasUnbackportedChanges"
        );
        assert!(run_sync_branches(&work, false).is_err());

        // 4. NonAncestorSameTree (agent cherry-picks same file content without ancestry) -> fails non-zero!
        let main_tip = util::git_head_commit(&work).unwrap();
        git(&["checkout", "develop-agent"]);
        git(&["cherry-pick", &main_tip]);
        assert_eq!(
            evaluate_sync_state(&work).unwrap().state,
            "NonAncestorSameTree"
        );
        assert!(run_sync_branches(&work, false).is_err());

        let _ = fs::remove_dir_all(&work);
    }
}
