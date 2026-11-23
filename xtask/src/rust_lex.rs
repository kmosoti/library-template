pub fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else {
            let ch = src[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

pub fn strip_cfg_test(src: &str) -> String {
    let Some(idx) = src.find("#[cfg(test)]") else {
        return src.to_string();
    };
    let prefix = &src[..idx];
    let after = &src[idx + "#[cfg(test)]".len()..];
    let (mut depth, mut entered, mut end_offset) = (0usize, false, after.len());
    for (pos, ch) in after.char_indices() {
        if ch == '{' {
            depth += 1;
            entered = true;
        } else if ch == '}' {
            depth = depth.saturating_sub(1);
            if entered && depth == 0 {
                end_offset = pos + 1;
                break;
            }
        } else if !entered && ch == ';' {
            end_offset = pos + 1;
            break;
        }
    }
    format!("{prefix}{}", strip_cfg_test(&after[end_offset..]))
}

pub fn split_top_level_items(src: &str) -> Vec<&str> {
    let (mut items, mut depth, mut start) = (Vec::new(), 0usize, 0usize);
    for (idx, ch) in src.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    items.push(&src[start..=idx]);
                    start = idx + 1;
                }
            }
            ';' if depth == 0 => {
                items.push(&src[start..=idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    items
}

pub fn split_brace_block(s: &str) -> Option<(&str, &str)> {
    let (open, close) = (s.find('{')?, s.rfind('}')?);
    (close > open).then_some((&s[..open], &s[open + 1..close]))
}

pub fn split_comma_top_level(s: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start) = (Vec::new(), 0usize, 0usize);
    for (idx, ch) in s.char_indices() {
        match ch {
            '{' | '(' | '<' => depth += 1,
            '}' | ')' | '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&s[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

pub fn normalize_ws(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" (", "(")
        .replace("( ", "(")
        .replace(" )", ")")
        .replace(" ,", ",")
        .replace(", }", " }")
        .replace(",)", ")")
        .replace(" :", ":")
}
