use crate::{release, util};
use std::path::{Path, PathBuf};

pub trait TagPushVerifier {
    fn verify_pushed_tag(
        &self,
        repo_override: Option<&str>,
        tag: &str,
        src_ref: Option<&str>,
    ) -> Result<(), String>;
}

pub struct WorkspaceTagVerifier {
    pub base_root: PathBuf,
}

impl TagPushVerifier for WorkspaceTagVerifier {
    fn verify_pushed_tag(
        &self,
        repo_override: Option<&str>,
        tag: &str,
        src_ref: Option<&str>,
    ) -> Result<(), String> {
        let root = match repo_override {
            Some(dir) => {
                let p = Path::new(dir);
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    self.base_root.join(p)
                }
            }
            None => self.base_root.clone(),
        };
        release::verify_tag_preflight(&root, tag)?;

        let head_sha = util::git_head_commit(&root)?;
        let target_expr = match src_ref {
            Some(src) => format!("{src}^{{commit}}"),
            None => format!("{tag}^{{commit}}"),
        };
        let resolved = util::capture_cmd(&root, "git", &["rev-parse", "--verify", &target_expr])
            .map_err(|_| format!("local tag/ref `{target_expr}` does not exist locally"))?;
        if resolved != head_sha {
            return Err(format!(
                "pushed tag target `{resolved}` does not match current HEAD `{head_sha}`"
            ));
        }
        Ok(())
    }
}

pub fn check_shell_command(command: &str, verifier: &dyn TagPushVerifier) -> Result<(), String> {
    for segment in command.split([';', '\n', '|', '&']) {
        let seg = segment.trim().trim_matches(['(', ')', '{', '}']).trim();
        if !seg.is_empty() {
            check_segment(seg, verifier)?;
        }
    }
    Ok(())
}

fn check_segment(raw_seg: &str, verifier: &dyn TagPushVerifier) -> Result<(), String> {
    let raw_tokens: Vec<&str> = raw_seg.split_whitespace().collect();
    let tokens = strip_env_and_wrappers(&raw_tokens);
    if tokens.is_empty() {
        return Ok(());
    }
    let prog = Path::new(tokens[0])
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(tokens[0]);

    if matches!(prog, "gh" | "curl" | "wget" | "http" | "xh")
        && (raw_seg.contains("pending_deployments")
            || raw_seg.contains("deployment-protection-rule")
            || ((raw_seg.contains("main-release")
                || raw_seg.contains("stable-publish")
                || raw_seg.contains("tag-release"))
                && (raw_seg.contains("approve") || raw_seg.contains("deployments"))))
    {
        return Err(
            "agents are forbidden from approving `main-release`, `stable-publish`, or `tag-release` human gates"
                .to_string(),
        );
    }

    if matches!(prog, "sh" | "bash" | "zsh")
        && let Some(idx) = tokens.iter().position(|t| *t == "-c" || *t == "-lc")
        && idx + 1 < tokens.len()
    {
        let inner = tokens[idx + 1..].join(" ");
        return check_shell_command(inner.trim_matches(['"', '\'']), verifier);
    }

    if prog == "gh" && tokens.get(1) == Some(&"pr") && tokens.get(2) == Some(&"merge") {
        if tokens.contains(&"--admin") {
            return Err("`gh pr merge --admin` is forbidden".to_string());
        }
        if !tokens.contains(&"--rebase") {
            return Err("`gh pr merge` must explicitly pass `--rebase`".to_string());
        }
        for tok in &tokens[3..] {
            let c = tok.trim_matches(['"', '\'']);
            if c == "main" || c.starts_with("release/") {
                return Err(format!(
                    "agents cannot merge PRs targeting protected branch `{c}`"
                ));
            }
        }
    }

    if prog == "git" {
        let (repo_override, sub_tokens) = strip_git_global_flags(&tokens[1..]);
        if sub_tokens.first() == Some(&"push") {
            check_git_push(&sub_tokens[1..], repo_override, verifier)?;
        }
    }
    Ok(())
}

fn strip_env_and_wrappers<'a>(tokens: &'a [&'a str]) -> &'a [&'a str] {
    let mut idx = 0;
    while idx < tokens.len() {
        let t = tokens[idx];
        if t == "env" || t == "command" || (t.contains('=') && !t.starts_with('-')) {
            idx += 1;
        } else {
            break;
        }
    }
    &tokens[idx..]
}

fn strip_git_global_flags<'a>(tokens: &'a [&'a str]) -> (Option<&'a str>, &'a [&'a str]) {
    let mut repo_dir = None;
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "-C" if i + 1 < tokens.len() => {
                repo_dir = Some(tokens[i + 1].trim_matches(['"', '\'']));
                i += 2;
            }
            "-c" | "--git-dir" | "--work-tree" if i + 1 < tokens.len() => i += 2,
            t if t.starts_with('-') => i += 1,
            _ => break,
        }
    }
    (repo_dir, &tokens[i..])
}

fn check_git_push(
    push_args: &[&str],
    repo_override: Option<&str>,
    verifier: &dyn TagPushVerifier,
) -> Result<(), String> {
    for tok in push_args {
        let cleaned = tok.trim_matches(['"', '\'']);
        if cleaned == "--tags" {
            return Err(
                "`git push --tags` is forbidden; push an explicit verified `vX.Y.Z` tag"
                    .to_string(),
            );
        }
        if cleaned.starts_with('-') {
            continue;
        }
        let (src_opt, dst_raw) = match cleaned.split_once(':') {
            Some((s, d)) => (Some(s.trim_start_matches('+')), d),
            None => (None, cleaned.trim_start_matches('+')),
        };
        if dst_raw == "main"
            || dst_raw == "refs/heads/main"
            || dst_raw.starts_with("release/")
            || dst_raw.starts_with("refs/heads/release/")
        {
            return Err(format!(
                "direct `git push` to protected branch `{cleaned}` is forbidden"
            ));
        }
        let maybe_tag = dst_raw.strip_prefix("refs/tags/").or_else(|| {
            (dst_raw.starts_with('v') && dst_raw.as_bytes().get(1).is_some_and(u8::is_ascii_digit))
                .then_some(dst_raw)
        });
        if let Some(tag_name) = maybe_tag {
            if cleaned.starts_with('+') || src_opt == Some("") {
                return Err(format!(
                    "force-pushing or deleting release tag `{tag_name}` is forbidden"
                ));
            }
            verifier
                .verify_pushed_tag(repo_override, tag_name, src_opt)
                .map_err(|e| format!("blocked `git push` for tag `{tag_name}`: {e}"))?;
        }
    }
    Ok(())
}
