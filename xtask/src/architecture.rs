use crate::util;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub const MAX_RS_LINES: usize = 350;

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<MetadataPackage>,
}

#[derive(Deserialize)]
struct MetadataPackage {
    name: String,
    dependencies: Vec<MetadataDep>,
    targets: Vec<MetadataTarget>,
}

#[derive(Deserialize)]
struct MetadataDep {
    name: String,
    kind: Option<String>,
}

#[derive(Deserialize)]
struct MetadataTarget {
    name: String,
    kind: Vec<String>,
}

pub fn check_architecture(root: &Path) -> Result<(), String> {
    let raw = util::capture_cmd(
        root,
        "cargo",
        &["metadata", "--format-version", "1", "--no-deps", "--locked"],
    )?;
    validate_metadata_json(&raw)?;

    let core_lib = fs::read_to_string(root.join("crates/quota-core/src/lib.rs"))
        .map_err(|e| format!("failed to read quota-core/src/lib.rs: {e}"))?;
    if !core_lib.lines().any(|l| l.trim() == "#![no_std]") {
        return Err("crates/quota-core/src/lib.rs must declare `#![no_std]`".to_string());
    }
    if !core_lib
        .lines()
        .any(|l| l.trim() == "#![forbid(unsafe_code)]")
    {
        return Err(
            "crates/quota-core/src/lib.rs must declare `#![forbid(unsafe_code)]`".to_string(),
        );
    }
    Ok(())
}

pub fn validate_metadata_json(raw: &str) -> Result<(), String> {
    let meta: CargoMetadata = serde_json::from_str(raw)
        .map_err(|e| format!("failed to parse `cargo metadata` JSON: {e}"))?;

    let workspace_names: BTreeSet<String> = meta.packages.iter().map(|p| p.name.clone()).collect();

    let core_pkg = meta
        .packages
        .iter()
        .find(|p| p.name == "quota-core")
        .ok_or_else(|| "workspace is missing package `quota-core`".to_string())?;

    let non_dev_deps: Vec<&str> = core_pkg
        .dependencies
        .iter()
        .filter(|d| matches!(d.kind.as_deref(), None | Some("build")))
        .map(|d| d.name.as_str())
        .collect();
    if !non_dev_deps.is_empty() {
        return Err(format!(
            "`quota-core` must have zero normal/build dependencies; found: {}",
            non_dev_deps.join(", ")
        ));
    }

    let non_test_targets: Vec<&MetadataTarget> = core_pkg
        .targets
        .iter()
        .filter(|t| !t.kind.iter().all(|k| k == "test" || k == "bench"))
        .collect();
    if non_test_targets.len() != 1 || non_test_targets[0].kind != vec!["lib".to_string()] {
        let summary: Vec<String> = non_test_targets
            .iter()
            .map(|t| format!("{}:{:?}", t.name, t.kind))
            .collect();
        return Err(format!(
            "`quota-core` must expose only a single `lib` target; found: {}",
            summary.join(", ")
        ));
    }

    let adapters_pkg = meta
        .packages
        .iter()
        .find(|p| p.name == "quota-adapters")
        .ok_or_else(|| "workspace is missing package `quota-adapters`".to_string())?;

    for dep in &adapters_pkg.dependencies {
        if matches!(dep.kind.as_deref(), None | Some("build")) {
            if workspace_names.contains(&dep.name) && dep.name != "quota-core" {
                return Err(format!(
                    "`quota-adapters` cannot depend on workspace crate `{}` (only `quota-core` allowed)",
                    dep.name
                ));
            }
            if !matches!(dep.name.as_str(), "quota-core" | "serde" | "serde_json") {
                return Err(format!(
                    "`quota-adapters` unexpected dependency `{}` (allowed: quota-core, serde, serde_json)",
                    dep.name
                ));
            }
        }
    }

    let depends_on_core = adapters_pkg
        .dependencies
        .iter()
        .any(|d| d.name == "quota-core" && d.kind.is_none());
    if !depends_on_core {
        return Err("`quota-adapters` must depend on `quota-core`".to_string());
    }
    Ok(())
}

pub fn check_line_caps(root: &Path) -> Result<usize, String> {
    let files = util::collect_repo_files(root)?;
    let mut max_lines = 0usize;
    let mut violations = Vec::new();

    for path in files {
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
        let count = content.lines().count();
        if count > max_lines {
            max_lines = count;
        }
        if count > MAX_RS_LINES {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            violations.push(format!(
                "{} ({count} > {MAX_RS_LINES} lines)",
                rel.display()
            ));
        }
    }

    if !violations.is_empty() {
        return Err(format!(
            "Rust file line cap exceeded: {}",
            violations.join(", ")
        ));
    }
    Ok(max_lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_core_normal_dependency_in_metadata() {
        let bad = r#"{
            "packages": [
                {
                    "name": "quota-core",
                    "dependencies": [{"name": "serde", "kind": null}],
                    "targets": [{"name": "quota_core", "kind": ["lib"]}]
                },
                {
                    "name": "quota-adapters",
                    "dependencies": [{"name": "quota-core", "kind": null}],
                    "targets": [{"name": "quota_adapters", "kind": ["lib"]}]
                }
            ]
        }"#;
        assert!(validate_metadata_json(bad).is_err());
    }
}
