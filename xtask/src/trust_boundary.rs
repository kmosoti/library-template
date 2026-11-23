use crate::util;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct CommitTouch {
    pub sha: String,
    pub message: String,
    pub files: Vec<String>,
}

pub fn check_trust_boundary(root: &Path, base: Option<&str>) -> Result<(), String> {
    let range = match base {
        Some(b) => format!("{b}..HEAD"),
        None => {
            let count_str = util::capture_cmd(root, "git", &["rev-list", "--count", "HEAD"])?;
            let count: usize = count_str
                .parse()
                .map_err(|e| format!("invalid rev-list count `{count_str}`: {e}"))?;
            if count <= 1 {
                return Ok(());
            }
            let rev_line =
                util::capture_cmd(root, "git", &["rev-list", "--parents", "-n", "1", "HEAD"])?;
            let parts: Vec<&str> = rev_line.split_whitespace().collect();
            if parts.len() >= 3 {
                verify_merge_commit_delta(root, parts[0], parts[1], parts[2])?;
                format!("{}..{}", parts[1], parts[2])
            } else {
                "HEAD~1..HEAD".to_string()
            }
        }
    };

    let shas_raw = util::capture_cmd(root, "git", &["rev-list", "--reverse", &range])?;
    if shas_raw.is_empty() {
        return Ok(());
    }

    let mut commits = Vec::new();
    for sha in shas_raw.lines().map(str::trim).filter(|s| !s.is_empty()) {
        let rev_line = util::capture_cmd(root, "git", &["rev-list", "--parents", "-n", "1", sha])?;
        let parts: Vec<&str> = rev_line.split_whitespace().collect();
        if parts.len() >= 3 {
            verify_merge_commit_delta(root, parts[0], parts[1], parts[2])?;
            continue;
        }
        let message = util::capture_cmd_raw(root, "git", &["show", "-s", "--format=%B", sha])?;
        let files_raw = util::capture_cmd(
            root,
            "git",
            &["diff-tree", "--no-commit-id", "--name-only", "-r", sha],
        )?;
        let files = files_raw
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        commits.push(CommitTouch {
            sha: sha.to_string(),
            message,
            files,
        });
    }
    evaluate_commit_range(&commits)
}

fn verify_merge_commit_delta(
    root: &Path,
    merge_sha: &str,
    p1: &str,
    p2: &str,
) -> Result<(), String> {
    let m_tree = util::capture_cmd(
        root,
        "git",
        &["rev-parse", &format!("{merge_sha}^{{tree}}")],
    )?;
    let p2_tree = util::capture_cmd(root, "git", &["rev-parse", &format!("{p2}^{{tree}}")])?;
    if m_tree == p2_tree {
        return Ok(());
    }
    let mut candidates = BTreeSet::new();
    for parent in [p1, p2] {
        let diff = util::capture_cmd(root, "git", &["diff", "--name-only", parent, merge_sha])?;
        for f in diff.lines().map(str::trim).filter(|s| !s.is_empty()) {
            candidates.insert(f.to_string());
        }
    }
    let mut novel = Vec::new();
    for path in candidates {
        let m_oid = blob_oid(root, merge_sha, &path);
        if m_oid != blob_oid(root, p1, &path) && m_oid != blob_oid(root, p2, &path) {
            novel.push(path);
        }
    }
    let has_impl = novel.iter().any(|p| is_impl_path(p));
    let has_ver = novel.iter().any(|p| is_verifier_path(p));
    if has_ver && (has_impl || !novel.is_empty()) {
        return Err(format!(
            "merge commit `{merge_sha}` introduced conflict-resolution edits touching verifier paths: {novel:?}"
        ));
    }
    Ok(())
}

fn blob_oid(root: &Path, rev: &str, path: &str) -> Option<String> {
    util::capture_cmd(
        root,
        "git",
        &["rev-parse", "--verify", &format!("{rev}:{path}")],
    )
    .ok()
}

pub fn evaluate_commit_range(commits: &[CommitTouch]) -> Result<(), String> {
    for commit in commits {
        reject_mixed_or_dishonest_trailer(commit)?;
    }

    let impl_present = commits
        .iter()
        .any(|c| c.files.iter().any(|f| is_impl_path(f)));
    if !impl_present {
        return Ok(());
    }

    // Verify per contiguous `(verifier+, impl+)` change group and forbid trailing verifier fixups after impl.
    let mut seen_impl_in_group = false;
    let mut group_has_verifier = false;
    let mut any_untrailered_verifier = None;

    for commit in commits {
        let touches_ver = commit.files.iter().any(|f| is_verifier_path(f));
        let touches_imp = commit.files.iter().any(|f| is_impl_path(f));
        if touches_ver && !has_git_trailer_verifier(&commit.message) {
            any_untrailered_verifier = Some(commit.sha.as_str());
        }
        if touches_ver {
            if seen_impl_in_group {
                // Starting a new verifier wave after an impl wave is valid only if followed by a matching impl commit.
                seen_impl_in_group = false;
                group_has_verifier = true;
            } else {
                group_has_verifier = true;
            }
        } else if touches_imp {
            seen_impl_in_group = true;
        }
    }

    if let Some(bad_sha) = any_untrailered_verifier {
        return Err(format!(
            "verifier commit `{bad_sha}` modified verifier/governance paths alongside implementation changes without `Trust-Boundary: verifier` trailer"
        ));
    }

    let last_ver_idx = commits
        .iter()
        .rposition(|c| c.files.iter().any(|f| is_verifier_path(f)));
    let last_impl_idx = commits
        .iter()
        .rposition(|c| c.files.iter().any(|f| is_impl_path(f)));
    let first_ver_idx = commits
        .iter()
        .position(|c| c.files.iter().any(|f| is_verifier_path(f)));
    let first_impl_idx = commits
        .iter()
        .position(|c| c.files.iter().any(|f| is_impl_path(f)));

    if group_has_verifier
        && let (Some(fv), Some(fi), Some(lv), Some(li)) =
            (first_ver_idx, first_impl_idx, last_ver_idx, last_impl_idx)
        && (fv > fi || lv > li)
    {
        return Err(
            "trust-boundary ordering violation: `Trust-Boundary: verifier` commits must strictly precede implementation commits"
                .to_string(),
        );
    }
    Ok(())
}

pub fn reject_mixed_or_dishonest_trailer(commit: &CommitTouch) -> Result<(), String> {
    let impl_files: Vec<&str> = commit
        .files
        .iter()
        .map(String::as_str)
        .filter(|p| is_impl_path(p))
        .collect();
    let ver_files: Vec<&str> = commit
        .files
        .iter()
        .map(String::as_str)
        .filter(|p| is_verifier_path(p))
        .collect();

    if !impl_files.is_empty() && !ver_files.is_empty() {
        return Err(format!(
            "mixed trust-boundary commit `{}` is strictly forbidden (modified `{}` and `{}` together)",
            commit.sha,
            impl_files.join(", "),
            ver_files.join(", ")
        ));
    }
    if has_git_trailer_verifier(&commit.message) && ver_files.is_empty() {
        return Err(format!(
            "commit `{}` carries `Trust-Boundary: verifier` trailer without modifying any verifier/governance path",
            commit.sha
        ));
    }
    Ok(())
}

pub fn is_impl_path(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() >= 4 && parts[0] == "crates" && parts[2] == "src"
}

pub fn is_verifier_path(path: &str) -> bool {
    if path.starts_with("xtask/") || path.starts_with(".github/") || path.starts_with("api/") {
        return true;
    }
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() >= 4 && parts[0] == "crates" && parts[2] == "tests"
}

pub fn has_git_trailer_verifier(message: &str) -> bool {
    let normalized = message.replace("\r\n", "\n");
    let Some((_, last_para)) = normalized.trim_end().rsplit_once("\n\n") else {
        return false;
    };
    let lines: Vec<&str> = last_para.lines().collect();
    !lines.is_empty()
        && lines.iter().all(|line| {
            line.starts_with([' ', '\t'])
                || line.split_once(':').is_some_and(|(tok, _)| {
                    !tok.is_empty() && tok.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
        })
        && lines
            .iter()
            .any(|line| line.trim() == "Trust-Boundary: verifier")
}

#[cfg(test)]
#[path = "trust_boundary_tests.rs"]
mod tests;
