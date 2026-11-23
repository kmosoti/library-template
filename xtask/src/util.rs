use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFingerprint {
    pub head_commit: String,
    pub head_tree: String,
    pub refs_digest: String,
    pub status_snapshot: String,
    pub worktree_dirty: bool,
    pub tree_hash: String,
}

pub fn workspace_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "unable to locate workspace root parent of xtask".to_string())
}

pub fn run_cmd(cwd: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .status()
        .map_err(|e| format!("failed to spawn `{program} {}`: {e}", args.join(" ")))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "command failed ({status}): `{program} {}`",
            args.join(" ")
        ))
    }
}

pub fn capture_cmd(cwd: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    capture_cmd_raw(cwd, program, args).map(|s| s.trim().to_string())
}

pub fn capture_cmd_raw(cwd: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| format!("failed to execute `{program} {}`: {e}", args.join(" ")))?;
    if out.status.success() {
        String::from_utf8(out.stdout).map_err(|e| format!("invalid utf-8 from `{program}`: {e}"))
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(format!(
            "`{program} {}` exited with {}: {}",
            args.join(" "),
            out.status,
            stderr.trim()
        ))
    }
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize() {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
    Ok(sha256_bytes(&data))
}

pub fn git_head_commit(root: &Path) -> Result<String, String> {
    capture_cmd(root, "git", &["rev-parse", "HEAD"])
}

pub fn git_head_tree(root: &Path) -> Result<String, String> {
    capture_cmd(root, "git", &["rev-parse", "HEAD^{tree}"])
}

pub fn git_status_snapshot(root: &Path) -> Result<String, String> {
    let raw = capture_cmd_raw(
        root,
        "git",
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    Ok(raw.trim_end_matches(['\r', '\n']).to_string())
}

pub fn compute_repo_fingerprint(root: &Path) -> Result<RepoFingerprint, String> {
    let head_commit = git_head_commit(root)?;
    let head_tree = git_head_tree(root)?;
    let refs_raw = capture_cmd_raw(
        root,
        "git",
        &[
            "for-each-ref",
            "--sort=refname",
            "--format=%(refname) %(objectname)",
        ],
    )?;
    let refs_digest = sha256_bytes(refs_raw.as_bytes());
    let status_snapshot = git_status_snapshot(root)?;
    let worktree_dirty = !status_snapshot.is_empty();

    let tree_hash = if !worktree_dirty {
        head_tree.clone()
    } else {
        let mut hasher = Sha256::new();
        hasher.update(head_tree.as_bytes());
        hasher.update(b"\n");
        hasher.update(status_snapshot.as_bytes());
        let mut paths: Vec<String> = status_snapshot
            .lines()
            .filter(|line| line.len() > 3)
            .map(|line| {
                line[3..]
                    .rsplit(" -> ")
                    .next()
                    .unwrap_or(&line[3..])
                    .trim_matches('"')
                    .to_string()
            })
            .collect();
        paths.sort();
        paths.dedup();
        for rel in paths {
            hasher.update(b"\0");
            hasher.update(rel.as_bytes());
            if let Ok(bytes) = fs::read(root.join(&rel)) {
                hasher.update(sha256_bytes(&bytes).as_bytes());
            }
        }
        let mut hex = String::with_capacity(64);
        for byte in hasher.finalize() {
            use std::fmt::Write as _;
            let _ = write!(&mut hex, "{byte:02x}");
        }
        format!("dirty:{hex}")
    };

    Ok(RepoFingerprint {
        head_commit,
        head_tree,
        refs_digest,
        status_snapshot,
        worktree_dirty,
        tree_hash,
    })
}

pub fn rustc_version(root: &Path) -> Result<String, String> {
    capture_cmd(root, "rustc", &["--version"])
}

pub fn workspace_version(root: &Path) -> Result<String, String> {
    let cargo_toml = fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|e| format!("failed to read workspace Cargo.toml: {e}"))?;
    extract_workspace_version(&cargo_toml)
}

pub fn extract_workspace_version(cargo_toml: &str) -> Result<String, String> {
    let mut in_workspace_pkg = false;
    for raw_line in cargo_toml.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_workspace_pkg = line == "[workspace.package]";
            continue;
        }
        if in_workspace_pkg
            && let Some(rest) = line.strip_prefix("version")
            && let Some(val) = rest.trim_start().strip_prefix('=')
        {
            let unquoted = val.trim().trim_matches('"');
            if !unquoted.is_empty() {
                return Ok(unquoted.to_string());
            }
        }
    }
    Err("missing `[workspace.package] version` in Cargo.toml".to_string())
}

pub fn collect_repo_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    walk_dir(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_dir(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        fs::read_dir(dir).map_err(|e| format!("failed to read dir `{}`: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry error: {e}"))?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|e| format!("strip_prefix error: {e}"))?;
        if let Some(first) = rel.components().next() {
            let name = first.as_os_str().to_string_lossy();
            if matches!(name.as_ref(), ".git" | "target" | "evidence" | "dist") {
                continue;
            }
        }
        let ftype = entry
            .file_type()
            .map_err(|e| format!("file_type error on `{}`: {e}", path.display()))?;
        if ftype.is_dir() {
            walk_dir(root, &path, out)?;
        } else if ftype.is_file() {
            out.push(path);
        }
    }
    Ok(())
}
