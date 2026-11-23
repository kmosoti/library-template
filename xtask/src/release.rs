use crate::{util, verify};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParsedTag {
    pub tag: String,
    pub version: String,
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub channel: String,
    pub prerelease: bool,
    pub required_environment: Option<String>,
}

#[derive(Debug, Serialize)]
struct PreflightReport {
    tag: String,
    version: String,
    channel: String,
    prerelease: bool,
    required_environment: Option<String>,
    release_notes: String,
    head_commit: String,
    tree_hash: String,
}

pub fn parse_release_tag(tag: &str) -> Result<ParsedTag, String> {
    let raw_ver = tag
        .strip_prefix('v')
        .ok_or_else(|| format!("release tag `{tag}` must start with `v`"))?;

    let (core_part, pre_part) = match raw_ver.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (raw_ver, None),
    };

    let nums: Vec<&str> = core_part.split('.').collect();
    if nums.len() != 3 || !nums.iter().all(|n| is_canonical_uint(n, true)) {
        return Err(format!(
            "invalid SemVer core in tag `{tag}` (expected `vX.Y.Z`)"
        ));
    }
    let major: u64 = nums[0].parse().map_err(|_| "invalid major")?;
    let minor: u64 = nums[1].parse().map_err(|_| "invalid minor")?;
    let patch: u64 = nums[2].parse().map_err(|_| "invalid patch")?;

    match pre_part {
        None => Ok(ParsedTag {
            tag: tag.to_string(),
            version: raw_ver.to_string(),
            major,
            minor,
            patch,
            channel: "stable".to_string(),
            prerelease: false,
            required_environment: Some("stable-publish".to_string()),
        }),
        Some(pre) => {
            let Some((chan, seq)) = pre.split_once('.') else {
                return Err(format!(
                    "invalid prerelease suffix in `{tag}` (expected `-(alpha|beta|rc).N`)"
                ));
            };
            if !matches!(chan, "alpha" | "beta" | "rc") || !is_canonical_uint(seq, false) {
                return Err(format!(
                    "invalid prerelease channel/number in `{tag}` (expected `-(alpha|beta|rc).N` with N >= 1)"
                ));
            }
            Ok(ParsedTag {
                tag: tag.to_string(),
                version: raw_ver.to_string(),
                major,
                minor,
                patch,
                channel: chan.to_string(),
                prerelease: true,
                required_environment: None,
            })
        }
    }
}

pub fn is_valid_release_branch_suffix(suffix: &str) -> bool {
    let parts: Vec<&str> = suffix.split('.').collect();
    parts.len() == 2 && parts.iter().all(|p| is_canonical_uint(p, true))
}

fn is_canonical_uint(s: &str, allow_zero: bool) -> bool {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if s == "0" {
        return allow_zero;
    }
    !s.starts_with('0')
}

pub fn verify_tag_preflight(
    root: &Path,
    tag: &str,
) -> Result<(ParsedTag, String, verify::VerificationReceipt), String> {
    let parsed = parse_release_tag(tag)?;
    let receipt = verify::load_clean_receipt_for_head(root)?;
    let ws_ver = util::workspace_version(root)?;
    if ws_ver != parsed.version {
        return Err(format!(
            "Cargo.toml `[workspace.package] version` (`{ws_ver}`) does not match tag `{tag}` (`{}`)",
            parsed.version
        ));
    }

    let notes_rel = format!("docs/releases/{tag}.md");
    let notes = fs::read_to_string(root.join(&notes_rel))
        .map_err(|_| format!("missing required release notes `{notes_rel}`"))?;
    if notes.trim().is_empty() {
        return Err(format!("release notes `{notes_rel}` cannot be empty"));
    }

    Ok((parsed, notes_rel, receipt))
}

pub fn preflight_tag(root: &Path, tag: &str) -> Result<(), String> {
    let (parsed, notes_rel, receipt) = verify_tag_preflight(root, tag)?;
    let report = PreflightReport {
        tag: parsed.tag,
        version: parsed.version,
        channel: parsed.channel,
        prerelease: parsed.prerelease,
        required_environment: parsed.required_environment,
        release_notes: notes_rel,
        head_commit: receipt.head_commit,
        tree_hash: receipt.tree_hash,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}

pub fn check_release_lineage(root: &Path, explicit_tag: Option<&str>) -> Result<(), String> {
    let resolved_tag = match explicit_tag {
        Some(t) => t.to_string(),
        None => env::var("GITHUB_REF_NAME")
            .ok()
            .filter(|r| r.starts_with('v'))
            .unwrap_or(format!("v{}", util::workspace_version(root)?)),
    };
    let parsed = parse_release_tag(&resolved_tag)?;

    let refs = util::capture_cmd(
        root,
        "git",
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/remotes/origin/release/",
        ],
    )?;
    for branch in refs.lines().map(str::trim).filter(|s| !s.is_empty()) {
        let suffix = branch.strip_prefix("origin/release/").unwrap_or("");
        if !is_valid_release_branch_suffix(suffix) {
            return Err(format!(
                "malformed remote maintenance branch `{branch}` (expected `origin/release/X.Y`)"
            ));
        }
    }

    let expected_rel = format!("origin/release/{}.{}", parsed.major, parsed.minor);
    let on_matching_rel = is_ancestor(root, "HEAD", &expected_rel);
    let on_main = is_ancestor(root, "HEAD", "origin/main");

    if parsed.patch == 0 {
        if on_main || on_matching_rel {
            return Ok(());
        }
        return Err(format!(
            "tag `{resolved_tag}` (`Z == 0`) must be reachable on `origin/main` or `{expected_rel}`"
        ));
    }

    if on_matching_rel {
        return Ok(());
    }
    if on_main
        && let Ok(main_cargo) = util::capture_cmd(root, "git", &["show", "origin/main:Cargo.toml"])
        && let Ok(main_ver) = util::extract_workspace_version(&main_cargo)
    {
        let main_core = main_ver.split('-').next().unwrap_or(&main_ver);
        let tag_core = format!("{}.{}.{}", parsed.major, parsed.minor, parsed.patch);
        if main_core == tag_core {
            return Ok(());
        }
    }
    Err(format!(
        "patch tag `{resolved_tag}` (`Z > 0`) must be reachable on `{expected_rel}` (or `origin/main` while `origin/main` Cargo.toml is still on `{}.{}.{}`)",
        parsed.major, parsed.minor, parsed.patch
    ))
}

fn is_ancestor(root: &Path, child: &str, ancestor_target: &str) -> bool {
    util::capture_cmd(root, "git", &["rev-parse", "--verify", ancestor_target]).is_ok()
        && util::run_cmd(
            root,
            "git",
            &["merge-base", "--is-ancestor", child, ancestor_target],
        )
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn rejects_patch_tag_on_mismatched_release_branch_and_malformed_branch() {
        assert!(is_valid_release_branch_suffix("1.2"));
        assert!(!is_valid_release_branch_suffix("v1.2"));
        assert!(!is_valid_release_branch_suffix("1.2.0"));

        let tmp = env::temp_dir().join(format!("lineage-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let git = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .current_dir(&tmp)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        git(&["init", "-b", "main"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "user.email", "test@example.invalid"]);
        fs::write(
            tmp.join("Cargo.toml"),
            "[workspace.package]\nversion = \"1.1.0\"\n",
        )
        .unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "v1.1.0"]);
        git(&["update-ref", "refs/remotes/origin/release/1.1", "HEAD"]);

        fs::write(
            tmp.join("Cargo.toml"),
            "[workspace.package]\nversion = \"1.3.0\"\n",
        )
        .unwrap();
        git(&["commit", "-am", "advance main to 1.3.0"]);
        git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);

        // Checkout release/1.1 commit and verify v1.2.1 is REJECTED on release/1.1!
        git(&["checkout", "--detach", "refs/remotes/origin/release/1.1"]);
        assert!(check_release_lineage(&tmp, Some("v1.2.1")).is_err());
        assert!(check_release_lineage(&tmp, Some("v1.1.1")).is_ok());

        // Matching release/1.2 accepts v1.2.1.
        git(&["update-ref", "refs/remotes/origin/release/1.2", "HEAD"]);
        assert!(check_release_lineage(&tmp, Some("v1.2.1")).is_ok());
        let _ = fs::remove_dir_all(&tmp);
    }
}
