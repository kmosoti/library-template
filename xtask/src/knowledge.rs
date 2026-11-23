use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Debug)]
struct KnowledgeNode {
    id: String,
    links: Vec<String>,
}

pub fn check_knowledge(root: &Path) -> Result<(), String> {
    let kb_dir = root.join("knowledge");
    let readme_path = kb_dir.join("README.md");
    let readme = fs::read_to_string(&readme_path)
        .map_err(|e| format!("missing `knowledge/README.md`: {e}"))?;

    let categories = [
        ("decisions", "D-", "decision"),
        ("research", "R-", "research"),
        ("observations", "O-", "observation"),
    ];

    let mut nodes: BTreeMap<String, KnowledgeNode> = BTreeMap::new();

    for (subdir, prefix, expected_kind) in categories {
        let dir = kb_dir.join(subdir);
        let entries = fs::read_dir(&dir)
            .map_err(|e| format!("missing knowledge directory `{}`: {e}", dir.display()))?;
        let mut count = 0usize;
        for entry in entries {
            let entry = entry.map_err(|e| format!("dir entry error: {e}"))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            count += 1;
            let file_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| "invalid knowledge filename".to_string())?;
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
            let node = parse_node(file_name, prefix, expected_kind, &content)?;
            if nodes.insert(node.id.clone(), node).is_some() {
                return Err(format!("duplicate knowledge node ID in `{file_name}`"));
            }
        }
        if count == 0 {
            return Err(format!(
                "`knowledge/{subdir}` must contain at least one epistemic markdown node"
            ));
        }
    }

    let all_ids: BTreeSet<String> = nodes.keys().cloned().collect();
    for node in nodes.values() {
        if !readme.contains(&node.id) {
            return Err(format!(
                "`knowledge/README.md` must index knowledge node `{}`",
                node.id
            ));
        }
        for link in &node.links {
            if !all_ids.contains(link) {
                return Err(format!(
                    "knowledge node `{}` links to non-existent ID `{link}`",
                    node.id
                ));
            }
        }
    }
    Ok(())
}

fn parse_node(
    file_name: &str,
    expected_prefix: &str,
    expected_kind: &str,
    content: &str,
) -> Result<KnowledgeNode, String> {
    let rest = content
        .strip_prefix("---\n")
        .ok_or_else(|| format!("`{file_name}` must start with YAML frontmatter `---`"))?;
    let (frontmatter, body) = rest
        .split_once("\n---\n")
        .ok_or_else(|| format!("`{file_name}` is missing closing frontmatter `---`"))?;
    if body.trim().is_empty() {
        return Err(format!("`{file_name}` body cannot be empty"));
    }

    let mut id = None;
    let mut kind = None;
    let mut status = None;
    let mut added_by = None;
    let mut human_ratified = None;
    let mut links = Vec::new();
    let mut in_links = false;

    for raw in frontmatter.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if in_links && let Some(item) = trimmed.strip_prefix("- ") {
            links.push(item.trim().to_string());
            continue;
        }
        in_links = false;
        let Some((k, v)) = trimmed.split_once(':') else {
            return Err(format!(
                "invalid frontmatter line in `{file_name}`: `{trimmed}`"
            ));
        };
        let key = k.trim();
        let val = v.trim();
        match key {
            "id" => id = Some(val.to_string()),
            "kind" => kind = Some(val.to_string()),
            "status" => status = Some(val.to_string()),
            "added_by" => added_by = Some(val.to_string()),
            "human_ratified" => human_ratified = Some(val.to_string()),
            "links" => {
                if val == "[]" {
                    in_links = false;
                } else if val.is_empty() {
                    in_links = true;
                }
            }
            _ => {}
        }
    }

    let id = id.ok_or_else(|| format!("`{file_name}` missing `id`"))?;
    if !id.starts_with(expected_prefix) || !file_name.starts_with(&format!("{id}-")) {
        return Err(format!(
            "`{file_name}` id `{id}` must start with `{expected_prefix}` and match filename prefix"
        ));
    }
    if kind.as_deref() != Some(expected_kind) {
        return Err(format!(
            "`{file_name}` kind must be `{expected_kind}`, got {kind:?}"
        ));
    }
    if !matches!(
        status.as_deref(),
        Some("proposed" | "active" | "verified" | "superseded")
    ) {
        return Err(format!("`{file_name}` has invalid `status`: {status:?}"));
    }
    if !matches!(added_by.as_deref(), Some("agent" | "human")) {
        return Err(format!(
            "`{file_name}` has invalid `added_by`: {added_by:?}"
        ));
    }
    if !matches!(human_ratified.as_deref(), Some("true" | "false")) {
        return Err(format!(
            "`{file_name}` `human_ratified` must be boolean `true` or `false`"
        ));
    }

    Ok(KnowledgeNode { id, links })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_epistemic_frontmatter() {
        let doc = "---\nid: D-0001\nkind: decision\nstatus: active\nadded_by: agent\nhuman_ratified: false\nlinks:\n  - R-0001\n---\n# Decision\nBody.\n";
        let node = parse_node("D-0001-sample.md", "D-", "decision", doc).unwrap();
        assert_eq!(node.id, "D-0001");
        assert_eq!(node.links, vec!["R-0001"]);
    }
}
