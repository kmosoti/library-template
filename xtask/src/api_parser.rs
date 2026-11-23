use crate::rust_lex::{
    normalize_ws, split_brace_block, split_comma_top_level, split_top_level_items, strip_cfg_test,
    strip_comments,
};
use std::collections::BTreeSet;

pub fn extract_source_api(mod_path: &str, src: &str) -> Vec<String> {
    let cleaned = strip_cfg_test(&strip_comments(src));
    let mut out = Vec::new();
    extract_items_in_scope(mod_path, &cleaned, &mut out);
    out
}

fn qualify(mod_path: &str, item: String) -> String {
    if mod_path.is_empty() {
        item
    } else {
        format!("mod {mod_path}::{item}")
    }
}

fn extract_items_in_scope(mod_path: &str, cleaned: &str, out: &mut Vec<String>) {
    for stmt in split_top_level_items(cleaned) {
        let (attrs_prefix, item) = extract_type_attrs_and_body(stmt.trim());
        if let Some(rest) = item.strip_prefix("pub mod ") {
            if let Some((name_raw, body)) = split_brace_block(rest) {
                let name = normalize_ws(name_raw);
                out.push(qualify(mod_path, format!("pub mod {name}")));
                let child_mod = if mod_path.is_empty() {
                    name
                } else {
                    format!("{mod_path}::{name}")
                };
                extract_items_in_scope(&child_mod, body, out);
            } else {
                let name = normalize_ws(rest.trim_end_matches(';'));
                out.push(qualify(mod_path, format!("pub mod {name}")));
            }
        } else if let Some(rest) = item.strip_prefix("pub use ") {
            out.push(qualify(
                mod_path,
                format!("pub use {}", normalize_ws(rest.trim_end_matches(';'))),
            ));
        } else if let Some(rest) = item.strip_prefix("pub type ") {
            out.push(qualify(
                mod_path,
                format!("pub type {}", normalize_ws(rest.trim_end_matches(';'))),
            ));
        } else if let Some(rest) = item.strip_prefix("pub const ")
            && !rest.starts_with("fn ")
        {
            let head = rest.split('=').next().unwrap_or(rest).trim_end_matches(';');
            out.push(qualify(
                mod_path,
                format!("pub const {}", normalize_ws(head)),
            ));
        } else if item.starts_with("pub struct ") {
            let head = normalize_ws(item.split(['(', '{', ';']).next().unwrap_or(item));
            let name = head.strip_prefix("pub struct ").unwrap_or(&head);
            out.push(qualify(mod_path, format!("{attrs_prefix}{head}")));
            extract_pub_struct_fields(mod_path, name, item, out);
        } else if let Some(rest) = item.strip_prefix("pub enum ") {
            if let Some((name_raw, body)) = split_brace_block(rest) {
                let name = normalize_ws(name_raw);
                out.push(qualify(mod_path, format!("{attrs_prefix}pub enum {name}")));
                for variant in split_comma_top_level(body) {
                    let (_, v_body) = extract_type_attrs_and_body(variant);
                    let v = normalize_ws(v_body);
                    if !v.is_empty() {
                        out.push(qualify(mod_path, format!("enum {name}::{v}")));
                    }
                }
            }
        } else if let Some(rest) = item.strip_prefix("pub trait ") {
            if let Some((head_raw, body)) = split_brace_block(rest) {
                let head = normalize_ws(head_raw);
                let trait_name = head.split([':', '<']).next().unwrap_or(&head).trim();
                out.push(qualify(mod_path, format!("pub trait {head}")));
                extract_trait_members(mod_path, trait_name, body, out);
            }
        } else if item.starts_with("pub fn ")
            || item.starts_with("pub const fn ")
            || item.starts_with("pub async fn ")
        {
            let sig = item.split('{').next().unwrap_or(item).trim_end_matches(';');
            out.push(qualify(mod_path, normalize_ws(sig)));
        } else if let Some(rest) = item.strip_prefix("impl")
            && rest.starts_with([' ', '<'])
            && let Some((head_raw, body)) = split_brace_block(rest)
        {
            extract_impl_block(mod_path, &normalize_ws(head_raw), body, out);
        }
    }
}

fn extract_trait_members(mod_path: &str, trait_name: &str, body: &str, out: &mut Vec<String>) {
    for member in split_top_level_items(body) {
        let (_, m_body) = extract_type_attrs_and_body(member);
        let trimmed = m_body.trim();
        if trimmed.is_empty() {
            continue;
        }
        let has_default = trimmed.contains('{')
            || (trimmed.starts_with("const ") && trimmed.contains('='))
            || (trimmed.starts_with("type ") && trimmed.contains('='));
        let sig_raw = if trimmed.starts_with("type ") {
            trimmed.trim_end_matches(';')
        } else {
            trimmed
                .split(['{', '='])
                .next()
                .unwrap_or(trimmed)
                .trim_end_matches(';')
        };
        let prefix = if has_default { "default " } else { "" };
        out.push(qualify(
            mod_path,
            format!("trait {trait_name}::{prefix}{}", normalize_ws(sig_raw)),
        ));
    }
}

fn extract_impl_block(mod_path: &str, head: &str, body: &str, out: &mut Vec<String>) {
    if head.contains(" for ") {
        out.push(qualify(mod_path, format!("impl {head}")));
        for member in split_top_level_items(body) {
            let (_, m) = extract_type_attrs_and_body(member);
            let m = m.trim();
            if let Some(rest) = m.strip_prefix("type ") {
                out.push(qualify(
                    mod_path,
                    format!(
                        "impl {head}::type {}",
                        normalize_ws(rest.trim_end_matches(';'))
                    ),
                ));
            } else if let Some(rest) = m.strip_prefix("const ")
                && !rest.starts_with("fn ")
            {
                let c_head =
                    normalize_ws(rest.split('=').next().unwrap_or(rest).trim_end_matches(';'));
                out.push(qualify(mod_path, format!("impl {head}::const {c_head}")));
            } else if m.starts_with("fn ") || m.starts_with("const fn ") {
                let sig = normalize_ws(m.split('{').next().unwrap_or(m));
                out.push(qualify(mod_path, format!("impl {head}::{sig}")));
            }
        }
    } else {
        for member in split_top_level_items(body) {
            let (_, m) = extract_type_attrs_and_body(member);
            let m = m.trim();
            if m.starts_with("pub fn ") || m.starts_with("pub const fn ") {
                let sig = normalize_ws(m.split('{').next().unwrap_or(m));
                out.push(qualify(mod_path, format!("impl {head}::{sig}")));
            } else if let Some(rest) = m.strip_prefix("pub type ") {
                out.push(qualify(
                    mod_path,
                    format!(
                        "impl {head}::pub type {}",
                        normalize_ws(rest.trim_end_matches(';'))
                    ),
                ));
            } else if let Some(rest) = m.strip_prefix("pub const ")
                && !rest.starts_with("fn ")
            {
                let c_head =
                    normalize_ws(rest.split('=').next().unwrap_or(rest).trim_end_matches(';'));
                out.push(qualify(
                    mod_path,
                    format!("impl {head}::pub const {c_head}"),
                ));
            }
        }
    }
}

fn extract_pub_struct_fields(mod_path: &str, name: &str, item: &str, out: &mut Vec<String>) {
    if let Some((_, body)) = split_brace_block(item) {
        for field in split_comma_top_level(body) {
            let (_, f) = extract_type_attrs_and_body(field);
            if f.starts_with("pub ") {
                out.push(qualify(
                    mod_path,
                    format!("struct {name}::{}", normalize_ws(f)),
                ));
            }
        }
    } else if let Some(open) = item.find('(')
        && let Some(close) = item.rfind(')')
        && close > open
    {
        for (idx, elem) in split_comma_top_level(&item[open + 1..close])
            .into_iter()
            .enumerate()
        {
            let (_, f) = extract_type_attrs_and_body(elem);
            if f.starts_with("pub ") {
                out.push(qualify(
                    mod_path,
                    format!("struct {name}::{idx}: {}", normalize_ws(f)),
                ));
            }
        }
    }
}

fn extract_type_attrs_and_body(mut s: &str) -> (String, &str) {
    let mut derives = BTreeSet::new();
    let mut reprs = Vec::new();
    let mut non_exhaustive = false;
    loop {
        s = s.trim_start();
        if s.starts_with("#[") || s.starts_with("#![") {
            let Some(end) = s.find(']') else { break };
            let attr = s[..=end].trim();
            if let Some(inner) = attr
                .strip_prefix("#[derive(")
                .and_then(|a| a.strip_suffix(")]"))
            {
                for d in inner.split(',').map(str::trim).filter(|d| !d.is_empty()) {
                    derives.insert(d.to_string());
                }
            } else if attr.starts_with("#[repr(") {
                reprs.push(normalize_ws(attr));
            } else if attr == "#[non_exhaustive]" {
                non_exhaustive = true;
            }
            s = &s[end + 1..];
        } else {
            break;
        }
    }
    let mut prefix = String::new();
    if non_exhaustive {
        prefix.push_str("#[non_exhaustive] ");
    }
    if !derives.is_empty() {
        let joined: Vec<_> = derives.into_iter().collect();
        prefix.push_str(&format!("#[derive({})] ", joined.join(", ")));
    }
    for r in reprs {
        prefix.push_str(&format!("{r} "));
    }
    (prefix, s)
}
