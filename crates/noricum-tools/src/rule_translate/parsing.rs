//! Parsing utilities: function signature parsing, brace matching, parameter extraction.

pub(super) type FuncSignature = (String, String, Vec<(String, String)>);

pub(super) fn try_parse_function_start(line: &str) -> Option<FuncSignature> {
    let paren_open = line.find('(')?;
    let paren_close = line.find(')')?;
    if paren_open >= paren_close {
        return None;
    }

    let before_paren = line[..paren_open].trim();
    let parts: Vec<&str> = before_paren.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let name = parts.last()?.trim_start_matches('*').to_string();
    let return_type = parts[..parts.len() - 1].join(" ");

    if [
        "typedef", "struct", "enum", "union", "if", "while", "for", "switch",
    ]
    .contains(&return_type.as_str())
    {
        return None;
    }

    let params_str = &line[paren_open + 1..paren_close];
    let params = parse_params(params_str);

    Some((name, return_type, params))
}

pub(super) fn parse_params(params_str: &str) -> Vec<(String, String)> {
    let trimmed = params_str.trim();
    if trimmed.is_empty() || trimmed == "void" {
        return Vec::new();
    }

    trimmed
        .split(',')
        .filter_map(|p| {
            let p = p.trim();
            let parts: Vec<&str> = p.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts.last()?.trim_start_matches('*').to_string();
                let ty = parts[..parts.len() - 1].join(" ");
                Some((ty, name))
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn find_brace_line(lines: &[&str], start: usize) -> Option<usize> {
    (start..lines.len()).find(|&i| lines[i].contains('{'))
}

/// Extract the content between the first `{` and its matching `}`.
/// Returns the inner text (without outer braces) and the line index of the closing `}`.
pub(super) fn extract_braced_block(lines: &[&str], start: usize) -> Option<(String, usize)> {
    // Flatten lines from start onward into a single string, tracking line boundaries
    let mut all_text = String::new();
    for line in lines.iter().skip(start) {
        if !all_text.is_empty() {
            all_text.push('\n');
        }
        all_text.push_str(line);
    }

    // Find the first { and its matching }
    let open_pos = all_text.find('{')?;
    let mut depth = 0;
    for (pos, ch) in all_text[open_pos..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let inner = &all_text[open_pos + 1..open_pos + pos];
                    // Count how many lines we consumed
                    let consumed = all_text[..open_pos + pos + 1].matches('\n').count();
                    return Some((inner.to_string(), start + consumed));
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract the parenthesized condition from a line like `if (x < 0) {` or `} else if (x > 0) {`.
pub(super) fn extract_paren_condition(line: &str) -> Option<String> {
    let paren_start = line.find('(')?;
    extract_balanced_parens(line, paren_start)
}

fn extract_balanced_parens(s: &str, start: usize) -> Option<String> {
    if s.as_bytes().get(start)? != &b'(' {
        return None;
    }
    let mut depth = 0;
    for (i, ch) in s[start..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start + 1..start + i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract the body of a braced block from `lines[start..]`.
/// The `{` may be on the same line as the control statement or on the next line.
/// Returns (body_content, last_line_index).
pub(super) fn extract_block_body(lines: &[&str], start: usize) -> Option<(String, usize)> {
    let brace_line = if lines[start].contains('{') {
        start
    } else if start + 1 < lines.len() && lines[start + 1].trim().starts_with('{') {
        start + 1
    } else {
        return None;
    };

    extract_braced_block(lines, brace_line)
}
