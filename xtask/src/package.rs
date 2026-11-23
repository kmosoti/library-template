use crate::{util, verify};
use flate2::read::GzDecoder;
use flate2::{Compression, GzBuilder};
use serde::Serialize;
use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Serialize)]
struct ReleaseEvidence {
    schema_version: u32,
    version: String,
    archive_name: String,
    archive_sha256: String,
    archive_size_bytes: u64,
    api_surface_sha256: String,
    unpacked_consumer_gates: Vec<String>,
    verification: verify::VerificationReceipt,
}

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn package_release(root: &Path, out_arg: &Path) -> Result<(), String> {
    let receipt = verify::load_clean_receipt_for_head(root)?;
    let version = util::workspace_version(root)?;
    let out_dir = if out_arg.is_absolute() {
        out_arg.to_path_buf()
    } else {
        root.join(out_arg)
    };
    fs::create_dir_all(&out_dir)
        .map_err(|e| format!("failed to create `{}`: {e}", out_dir.display()))?;

    let archive_name = format!("quota-core-v{version}.tar.gz");
    let archive_path = out_dir.join(&archive_name);
    let prefix = format!("quota-core-v{version}");

    build_deterministic_tar_gz(root, &archive_path, &prefix)?;
    verify_unpacked_consumer_archive(&archive_path, &prefix)?;

    let archive_sha256 = util::sha256_file(&archive_path)?;
    let archive_size_bytes = fs::metadata(&archive_path)
        .map_err(|e| format!("stat failed on `{}`: {e}", archive_path.display()))?
        .len();
    let api_surface_sha256 = util::sha256_file(&root.join("api/quota-core.api"))?;

    let evidence = ReleaseEvidence {
        schema_version: 2,
        version,
        archive_name: archive_name.clone(),
        archive_sha256: archive_sha256.clone(),
        archive_size_bytes,
        api_surface_sha256,
        unpacked_consumer_gates: vec![
            "cargo metadata --offline --no-deps --format-version 1".to_string(),
            "cargo test --offline --workspace".to_string(),
            "cargo check --offline -p quota-core --target thumbv7em-none-eabi".to_string(),
        ],
        verification: receipt,
    };
    let evidence_path = out_dir.join("RELEASE-EVIDENCE.json");
    let evidence_json =
        serde_json::to_string_pretty(&evidence).map_err(|e| format!("json error: {e}"))?;
    fs::write(&evidence_path, format!("{evidence_json}\n"))
        .map_err(|e| format!("failed to write `{}`: {e}", evidence_path.display()))?;

    let evidence_sha256 = util::sha256_file(&evidence_path)?;
    let sums =
        format!("{archive_sha256}  {archive_name}\n{evidence_sha256}  RELEASE-EVIDENCE.json\n");
    fs::write(out_dir.join("SHA256SUMS"), sums)
        .map_err(|e| format!("failed to write SHA256SUMS: {e}"))?;

    println!(
        "[packaged] {archive_name} (verified unpacked consumer metadata + test + thumbv7em-none-eabi), RELEASE-EVIDENCE.json, SHA256SUMS"
    );
    Ok(())
}

fn build_deterministic_tar_gz(
    root: &Path,
    archive_path: &Path,
    prefix: &str,
) -> Result<(), String> {
    let raw_files = util::capture_cmd_raw(root, "git", &["ls-files", "-z"])?;
    let mut tracked: Vec<&str> = raw_files.split('\0').filter(|s| !s.is_empty()).collect();
    tracked.sort_unstable();
    tracked.dedup();

    let out_file = File::create(archive_path)
        .map_err(|e| format!("failed to create `{}`: {e}", archive_path.display()))?;
    let gz = GzBuilder::new()
        .mtime(0)
        .write(out_file, Compression::best());
    let mut builder = tar::Builder::new(gz);

    for rel in tracked {
        let full = root.join(rel);
        let bytes =
            fs::read(&full).map_err(|e| format!("failed to read `{}`: {e}", full.display()))?;
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(bytes.len() as u64);
        header.set_mode(normalized_mode(&full));
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_cksum();

        let tar_path = format!("{prefix}/{rel}");
        builder
            .append_data(&mut header, &tar_path, bytes.as_slice())
            .map_err(|e| format!("tar append failed on `{tar_path}`: {e}"))?;
    }

    let gz = builder
        .into_inner()
        .map_err(|e| format!("failed finishing tar stream: {e}"))?;
    gz.finish()
        .map_err(|e| format!("failed finishing gzip stream: {e}"))?;
    Ok(())
}

#[cfg(unix)]
fn normalized_mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    match fs::metadata(path) {
        Ok(m) if (m.permissions().mode() & 0o111) != 0 => 0o755,
        _ => 0o644,
    }
}

#[cfg(not(unix))]
fn normalized_mode(_path: &Path) -> u32 {
    0o644
}

fn verify_unpacked_consumer_archive(archive_path: &Path, prefix: &str) -> Result<(), String> {
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_base = env::temp_dir().join(format!("quota-core-consumer-verify-{pid}-{nonce}"));
    fs::create_dir_all(&temp_base)
        .map_err(|e| format!("failed creating temp consumer dir: {e}"))?;
    let _guard = TempDirGuard(temp_base.clone());

    let file = File::open(archive_path).map_err(|e| format!("open archive failed: {e}"))?;
    let gz = GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive
        .unpack(&temp_base)
        .map_err(|e| format!("unpacking release archive failed: {e}"))?;

    let unpacked_root = temp_base.join(prefix);
    let unpacked_target = temp_base.join("target");

    for args in [
        &[
            "metadata",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ][..],
        &["test", "--offline", "--workspace"][..],
        &[
            "check",
            "--offline",
            "-p",
            "quota-core",
            "--target",
            "thumbv7em-none-eabi",
        ][..],
    ] {
        let out = Command::new("cargo")
            .current_dir(&unpacked_root)
            .env("CARGO_TARGET_DIR", &unpacked_target)
            .args(args)
            .output()
            .map_err(|e| {
                format!(
                    "failed spawning unpacked consumer `cargo {}`: {e}",
                    args.join(" ")
                )
            })?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(format!(
                "unpacked archive consumer check `cargo {}` failed ({}):\n{}",
                args.join(" "),
                out.status,
                stderr.trim()
            ));
        }
    }
    Ok(())
}
