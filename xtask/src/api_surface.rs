use crate::{api_parser, util};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub fn check_api_surface(root: &Path, base_ref: Option<&str>) -> Result<(), String> {
    let src_dir = root.join("crates/quota-core/src");
    let api_file = root.join("api/quota-core.api");

    let actual = extract_dir_api(&src_dir)?;
    let raw_expected = fs::read_to_string(&api_file)
        .map_err(|e| format!("failed to read `{}`: {e}", api_file.display()))?;
    let expected = parse_manifest_lines(&raw_expected);

    if actual != expected {
        let exp_set: BTreeSet<&str> = expected.iter().map(String::as_str).collect();
        let act_set: BTreeSet<&str> = actual.iter().map(String::as_str).collect();
        let missing: Vec<&str> = exp_set.difference(&act_set).copied().collect();
        let added: Vec<&str> = act_set.difference(&exp_set).copied().collect();
        return Err(format!(
            "public API surface drift detected against `api/quota-core.api`!\n  \
             removed/changed: {missing:?}\n  added/unexpected: {added:?}"
        ));
    }

    if let Some(base) = base_ref {
        let base_api_raw = util::capture_cmd(
            root,
            "git",
            &["show", &format!("{base}:api/quota-core.api")],
        )?;
        let base_cargo = util::capture_cmd(root, "git", &["show", &format!("{base}:Cargo.toml")])?;
        let base_ver = util::extract_workspace_version(&base_cargo)?;
        let cur_ver = util::workspace_version(root)?;
        let has_notes = root.join(format!("docs/releases/v{cur_ver}.md")).is_file();
        verify_semver_evolution(
            &parse_manifest_lines(&base_api_raw),
            &expected,
            &base_ver,
            &cur_ver,
            has_notes,
        )?;
    }
    Ok(())
}

pub fn parse_manifest_lines(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !(l.starts_with('#') && !l.starts_with("#[")))
        .map(ToOwned::to_owned)
        .collect()
}

pub fn verify_semver_evolution(
    base_api: &[String],
    cur_api: &[String],
    base_ver: &str,
    cur_ver: &str,
    has_release_notes: bool,
) -> Result<(), String> {
    let base_set: BTreeSet<&str> = base_api.iter().map(String::as_str).collect();
    let cur_set: BTreeSet<&str> = cur_api.iter().map(String::as_str).collect();
    let removed: Vec<&str> = base_set.difference(&cur_set).copied().collect();
    let added: Vec<&str> = cur_set.difference(&base_set).copied().collect();
    if removed.is_empty() && added.is_empty() {
        return Ok(());
    }

    let exhaustive_enums = collect_exhaustive_enums(base_api);
    let existing_traits = collect_existing_traits(base_api);

    let mut breaking_additions: Vec<&str> = Vec::new();
    for line in &added {
        if is_breaking_addition(line, &exhaustive_enums, &existing_traits) {
            breaking_additions.push(line);
        }
    }

    let (maj0, min0, pat0) = parse_semver_core(base_ver)?;
    let (maj1, min1, pat1) = parse_semver_core(cur_ver)?;
    if !removed.is_empty() || !breaking_additions.is_empty() {
        let bumped_breaking = if maj0 == 0 {
            maj1 > 0 || (maj1 == 0 && min1 > min0)
        } else {
            maj1 > maj0
        };
        if !bumped_breaking || !has_release_notes {
            return Err(format!(
                "breaking API change from {base_ver} -> {cur_ver} (removed/mutated={removed:?}, breaking_additions={breaking_additions:?}) requires SemVer breaking bump + `docs/releases/v{cur_ver}.md`"
            ));
        }
    } else {
        let bumped_additive = if maj0 >= 1 {
            maj1 > maj0 || (maj1 == maj0 && min1 > min0)
        } else {
            (maj1, min1, pat1) > (maj0, min0, pat0)
        };
        if !bumped_additive || !has_release_notes {
            return Err(format!(
                "additive API change from {base_ver} -> {cur_ver} requires SemVer bump + `docs/releases/v{cur_ver}.md`: {added:?}"
            ));
        }
    }
    Ok(())
}

fn split_mod_prefix(line: &str) -> (&str, &str) {
    if let Some(rest) = line.strip_prefix("mod ")
        && let Some((m, item)) = rest.split_once("::")
    {
        return (m, item);
    }
    ("", line)
}

fn collect_exhaustive_enums(lines: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in lines {
        let (mod_p, item) = split_mod_prefix(line);
        if let Some((_, enum_name)) = item.rsplit_once("pub enum ")
            && !item.contains("#[non_exhaustive]")
        {
            let name = enum_name.split('<').next().unwrap_or(enum_name).trim();
            out.insert(format!("{mod_p}::{name}"));
        }
    }
    out
}

fn collect_existing_traits(lines: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in lines {
        let (mod_p, item) = split_mod_prefix(line);
        if let Some(rest) = item.strip_prefix("pub trait ") {
            let name = rest.split([':', '<']).next().unwrap_or(rest).trim();
            out.insert(format!("{mod_p}::{name}"));
        }
    }
    out
}

fn is_breaking_addition(
    line: &str,
    exhaustive_enums: &BTreeSet<String>,
    existing_traits: &BTreeSet<String>,
) -> bool {
    let (mod_p, item) = split_mod_prefix(line);
    if let Some(rest) = item.strip_prefix("enum ")
        && let Some((enum_name, _)) = rest.split_once("::")
    {
        return exhaustive_enums.contains(&format!("{mod_p}::{enum_name}"));
    }
    if let Some(rest) = item.strip_prefix("trait ")
        && let Some((trait_name, member)) = rest.split_once("::")
    {
        return existing_traits.contains(&format!("{mod_p}::{trait_name}"))
            && !member.starts_with("default ");
    }
    false
}

fn parse_semver_core(ver: &str) -> Result<(u64, u64, u64), String> {
    let core = ver.split('-').next().unwrap_or(ver);
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err(format!("invalid version `{ver}`"));
    }
    Ok((
        parts[0].parse().map_err(|_| format!("bad major `{ver}`"))?,
        parts[1].parse().map_err(|_| format!("bad minor `{ver}`"))?,
        parts[2].parse().map_err(|_| format!("bad patch `{ver}`"))?,
    ))
}

pub fn extract_dir_api(src_dir: &Path) -> Result<Vec<String>, String> {
    let mut rs_files = Vec::new();
    walk_rs_files(src_dir, &mut rs_files)?;
    rs_files.sort();
    let mut set = BTreeSet::new();
    for path in rs_files {
        let mod_path = file_mod_path(src_dir, &path)?;
        let src = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
        for item in api_parser::extract_source_api(&mod_path, &src) {
            set.insert(item);
        }
    }
    Ok(set.into_iter().collect())
}

fn file_mod_path(src_dir: &Path, path: &Path) -> Result<String, String> {
    let rel = path.strip_prefix(src_dir).map_err(|e| e.to_string())?;
    let mut comps: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    if let Some(last) = comps.last_mut() {
        *last = last.trim_end_matches(".rs").to_string();
    }
    if comps.as_slice() == ["lib"] {
        return Ok(String::new());
    }
    if comps.last().map(String::as_str) == Some("mod") {
        comps.pop();
    }
    Ok(comps.join("::"))
}

fn walk_rs_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("read_dir `{}`: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            walk_rs_files(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_assoc_type_mutation_exhaustive_enum_variant_and_required_trait_item_as_breaking() {
        let v1 = api_parser::extract_source_api(
            "seq",
            "pub enum Decision { Allow, Deny }\npub trait Cursor { fn pos(&self) -> usize; }\nimpl Iterator for Seq { type Item = u8; fn next(&mut self) -> Option<u8> { None } }",
        );
        let v2_assoc = api_parser::extract_source_api(
            "seq",
            "pub enum Decision { Allow, Deny }\npub trait Cursor { fn pos(&self) -> usize; }\nimpl Iterator for Seq { type Item = u16; fn next(&mut self) -> Option<u16> { None } }",
        );
        assert_ne!(v1, v2_assoc);
        assert!(verify_semver_evolution(&v1, &v2_assoc, "0.1.0", "0.1.1", true).is_err());
        assert!(verify_semver_evolution(&v1, &v2_assoc, "1.0.0", "1.1.0", true).is_err());
        assert!(verify_semver_evolution(&v1, &v2_assoc, "1.0.0", "2.0.0", true).is_ok());

        // Adding a variant to exhaustive `Decision` is breaking on patch or >=1.0 minor!
        let v2_variant = api_parser::extract_source_api(
            "seq",
            "pub enum Decision { Allow, Deny, Defer }\npub trait Cursor { fn pos(&self) -> usize; }\nimpl Iterator for Seq { type Item = u8; fn next(&mut self) -> Option<u8> { None } }",
        );
        assert!(verify_semver_evolution(&v1, &v2_variant, "0.1.0", "0.1.1", true).is_err());
        assert!(verify_semver_evolution(&v1, &v2_variant, "1.0.0", "1.1.0", true).is_err());
        assert!(verify_semver_evolution(&v1, &v2_variant, "0.1.0", "0.2.0", true).is_ok());

        // Adding a non-defaulted trait method is breaking; defaulted method is additive!
        let v2_req_trait = api_parser::extract_source_api(
            "seq",
            "pub enum Decision { Allow, Deny }\npub trait Cursor { fn pos(&self) -> usize; fn reset(&mut self); }\nimpl Iterator for Seq { type Item = u8; fn next(&mut self) -> Option<u8> { None } }",
        );
        assert!(verify_semver_evolution(&v1, &v2_req_trait, "1.0.0", "1.1.0", true).is_err());

        let v2_def_trait = api_parser::extract_source_api(
            "seq",
            "pub enum Decision { Allow, Deny }\npub trait Cursor { fn pos(&self) -> usize; fn is_zero(&self) -> bool { self.pos() == 0 } }\nimpl Iterator for Seq { type Item = u8; fn next(&mut self) -> Option<u8> { None } }",
        );
        assert!(verify_semver_evolution(&v1, &v2_def_trait, "1.0.0", "1.1.0", true).is_ok());
    }
}
