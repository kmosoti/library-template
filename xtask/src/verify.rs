use crate::{agent_sync, api_surface, architecture, governance, knowledge, trust_boundary, util};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub const EXPECTED_GATES: &[&str] = &[
    "fmt",
    "clippy",
    "test",
    "thumbv7em-none-eabi",
    "check-architecture",
    "check-api-surface",
    "check-trust-boundary",
    "check-knowledge",
    "check-workflow-pins",
    "check-agent-configs",
    "check-line-caps",
    "check-privacy",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReceipt {
    pub schema_version: u32,
    pub template: String,
    pub version: String,
    pub status: String,
    pub worktree_dirty: bool,
    pub head_commit: String,
    pub tree_hash: String,
    pub git_tree: String,
    pub refs_digest: String,
    pub cargo_lock_sha256: String,
    pub toolchain_sha256: String,
    pub api_surface_sha256: String,
    pub rustc_version: String,
    pub max_rs_lines: usize,
    pub gates: Vec<String>,
}

pub fn receipt_path(root: &Path) -> PathBuf {
    root.join("evidence/verification.json")
}

pub fn load_clean_receipt_for_head(root: &Path) -> Result<VerificationReceipt, String> {
    let fp = util::compute_repo_fingerprint(root)?;
    if fp.worktree_dirty {
        return Err(
            "working tree is dirty; commit or discard changes and run `cargo xtask verify`"
                .to_string(),
        );
    }

    let path = receipt_path(root);
    let raw = fs::read_to_string(&path).map_err(|_| {
        "missing `evidence/verification.json` (run `cargo xtask verify` first)".to_string()
    })?;
    let receipt: VerificationReceipt = serde_json::from_str(&raw)
        .map_err(|e| format!("corrupt `evidence/verification.json`: {e}"))?;

    if receipt.status != "passed" {
        return Err("`evidence/verification.json` status is not `passed`".to_string());
    }
    if receipt.worktree_dirty {
        return Err(
            "`evidence/verification.json` was recorded on a dirty working tree (`worktree_dirty: true`)"
                .to_string(),
        );
    }
    if receipt.head_commit != fp.head_commit {
        return Err(format!(
            "stale verification receipt: commit `{}` != HEAD `{}`",
            receipt.head_commit, fp.head_commit
        ));
    }
    if receipt.tree_hash != fp.head_tree || receipt.git_tree != fp.head_tree {
        return Err(format!(
            "stale verification receipt: tree `{}` != HEAD^{{tree}} `{}`",
            receipt.tree_hash, fp.head_tree
        ));
    }
    if receipt.refs_digest != fp.refs_digest {
        return Err("stale verification receipt: git refs digest changed".to_string());
    }
    if receipt.cargo_lock_sha256 != util::sha256_file(&root.join("Cargo.lock"))? {
        return Err("stale verification receipt: `Cargo.lock` sha256 mismatch".to_string());
    }
    if receipt.toolchain_sha256 != util::sha256_file(&root.join("rust-toolchain.toml"))? {
        return Err(
            "stale verification receipt: `rust-toolchain.toml` sha256 mismatch".to_string(),
        );
    }
    if receipt.api_surface_sha256 != util::sha256_file(&root.join("api/quota-core.api"))? {
        return Err("stale verification receipt: `api/quota-core.api` sha256 mismatch".to_string());
    }
    let expected: Vec<String> = EXPECTED_GATES.iter().map(|s| (*s).to_string()).collect();
    if receipt.gates != expected {
        return Err("verification receipt does not cover the full required gate list".to_string());
    }
    Ok(receipt)
}

pub fn run_verify(root: &Path, force: bool) -> Result<(), String> {
    let pre_fp = util::compute_repo_fingerprint(root)?;
    let rustc_ver = util::rustc_version(root)?;

    if !force
        && !pre_fp.worktree_dirty
        && let Ok(existing) = load_clean_receipt_for_head(root)
        && existing.rustc_version == rustc_ver
    {
        println!(
            "[idempotent cache hit] evidence/verification.json matches (commit={}, tree={}, lock={}, rustc={})",
            existing.head_commit,
            existing.tree_hash,
            existing.cargo_lock_sha256,
            existing.rustc_version
        );
        return Ok(());
    }

    println!("[verify 1/12] cargo fmt --all -- --check");
    util::run_cmd(root, "cargo", &["fmt", "--all", "--", "--check"])?;

    println!("[verify 2/12] cargo clippy --workspace --all-targets --locked -- -D warnings");
    util::run_cmd(
        root,
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
    )?;

    println!("[verify 3/12] cargo test --workspace --locked");
    util::run_cmd(root, "cargo", &["test", "--workspace", "--locked"])?;

    println!("[verify 4/12] cargo check --locked -p quota-core --target thumbv7em-none-eabi");
    util::run_cmd(
        root,
        "cargo",
        &[
            "check",
            "--locked",
            "-p",
            "quota-core",
            "--target",
            "thumbv7em-none-eabi",
        ],
    )?;

    println!("[verify 5/12] check-architecture");
    architecture::check_architecture(root)?;

    println!("[verify 6/12] check-api-surface");
    api_surface::check_api_surface(root, None)?;

    println!("[verify 7/12] check-trust-boundary");
    trust_boundary::check_trust_boundary(root, None)?;

    println!("[verify 8/12] check-knowledge");
    knowledge::check_knowledge(root)?;

    println!("[verify 9/12] check-workflow-pins");
    governance::check_workflow_pins(root)?;

    println!("[verify 10/12] check-agent-configs");
    agent_sync::check_agent_configs(root)?;

    println!("[verify 11/12] check-line-caps");
    let max_rs_lines = architecture::check_line_caps(root)?;

    println!("[verify 12/12] check-privacy");
    governance::check_privacy(root)?;

    let post_fp = util::compute_repo_fingerprint(root)?;
    if pre_fp != post_fp {
        return Err(
            "repository fingerprint (HEAD, refs, or worktree files) changed mid-verification; failing closed"
                .to_string(),
        );
    }

    let receipt = VerificationReceipt {
        schema_version: 3,
        template: "library-template".to_string(),
        version: util::workspace_version(root)?,
        status: "passed".to_string(),
        worktree_dirty: pre_fp.worktree_dirty,
        head_commit: pre_fp.head_commit.clone(),
        git_tree: pre_fp.tree_hash.clone(),
        tree_hash: pre_fp.tree_hash.clone(),
        refs_digest: pre_fp.refs_digest.clone(),
        cargo_lock_sha256: util::sha256_file(&root.join("Cargo.lock"))?,
        toolchain_sha256: util::sha256_file(&root.join("rust-toolchain.toml"))?,
        api_surface_sha256: util::sha256_file(&root.join("api/quota-core.api"))?,
        rustc_version: rustc_ver,
        max_rs_lines,
        gates: EXPECTED_GATES.iter().map(|s| (*s).to_string()).collect(),
    };

    write_receipt_atomically(&receipt_path(root), &receipt)?;
    if util::compute_repo_fingerprint(root)? != pre_fp {
        let _ = fs::remove_file(receipt_path(root));
        return Err("writing verification receipt altered repository fingerprint".to_string());
    }

    println!(
        "[verified] all 12 gates passed (worktree_dirty={}, max_rs_lines={max_rs_lines}) -> evidence/verification.json",
        pre_fp.worktree_dirty
    );
    Ok(())
}

fn write_receipt_atomically(out_path: &Path, receipt: &VerificationReceipt) -> Result<(), String> {
    let parent = out_path
        .parent()
        .ok_or_else(|| "invalid receipt path".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("failed to create `{}`: {e}", parent.display()))?;
    let tmp_path = parent.join(format!(".verification.json.tmp.{}", std::process::id()));
    let json = serde_json::to_string_pretty(receipt)
        .map_err(|e| format!("failed to serialize verification receipt: {e}"))?;
    {
        let mut f = File::create(&tmp_path)
            .map_err(|e| format!("failed to create `{}`: {e}", tmp_path.display()))?;
        f.write_all(format!("{json}\n").as_bytes())
            .map_err(|e| format!("failed writing temp receipt: {e}"))?;
        f.sync_all()
            .map_err(|e| format!("failed fsyncing temp receipt: {e}"))?;
    }
    fs::rename(&tmp_path, out_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("atomic rename to `{}` failed: {e}", out_path.display())
    })
}
