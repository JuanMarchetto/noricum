//! Expression translation: C expressions to Rust equivalents.

use super::functions::c_type_to_rust;

/// Split arguments respecting parenthesized expressions.
pub(super) fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        args.push(trimmed);
    }
    args
}

pub(super) fn translate_expr(expr: &str) -> Option<String> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }

    // Boolean literals
    if expr == "true" || expr == "false" {
        return Some(expr.to_string());
    }

    // Simple identifiers
    if expr.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Some(expr.to_string());
    }

    // Numeric literals
    if is_numeric_literal(expr) {
        return Some(clean_numeric_literal(expr));
    }

    // Negative number
    if expr.starts_with('-') && expr.len() > 1 && is_numeric_literal(&expr[1..]) {
        return Some(format!("-{}", clean_numeric_literal(&expr[1..])));
    }

    // Ternary: cond ? a : b
    if let Some(result) = try_translate_ternary(expr) {
        return Some(result);
    }

    // Binary operators (ordered by precedence: logical -> comparison -> bitwise -> arithmetic)
    const BINARY_OPS: &[&[&str]] = &[
        &[" && ", " || "],
        &[" == ", " != ", " <= ", " >= ", " < ", " > "],
        &[" << ", " >> ", " & ", " | ", " ^ "],
        &[" + ", " - ", " * ", " / ", " % "],
    ];
    for group in BINARY_OPS {
        for op in *group {
            if let Some(pos) = find_operator(expr, op) {
                let lhs = translate_expr(&expr[..pos])?;
                let rhs = translate_expr(&expr[pos + op.len()..])?;
                return Some(format!("{lhs}{op}{rhs}"));
            }
        }
    }

    // Unary not
    if let Some(rest) = expr.strip_prefix('!') {
        let inner = translate_expr(rest)?;
        return Some(format!("!{inner}"));
    }

    // Unary minus (expressions)
    if expr.starts_with('-') && expr.len() > 1 {
        let inner = translate_expr(&expr[1..])?;
        return Some(format!("-{inner}"));
    }

    // Cast: (type)expr
    if expr.starts_with('(')
        && let Some(close) = expr.find(')')
    {
        let maybe_type = expr[1..close].trim();
        if c_type_to_rust(maybe_type).is_some() && close + 1 < expr.len() {
            let inner = translate_expr(&expr[close + 1..])?;
            let rust_ty = c_type_to_rust(maybe_type)?;
            return Some(format!("{inner} as {rust_ty}"));
        }
    }

    // Function call: name(args)
    if let Some(paren) = expr.find('(')
        && expr.ends_with(')')
    {
        let name = &expr[..paren];
        if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.is_empty() {
            let args_str = &expr[paren + 1..expr.len() - 1];
            let args = split_args(args_str);
            let mut rust_args = Vec::new();
            for arg in &args {
                let t = arg.trim();
                if t.is_empty() {
                    continue;
                }
                rust_args.push(translate_expr(t)?);
            }
            return Some(format!("{name}({})", rust_args.join(", ")));
        }
    }

    // Parenthesized expression
    if expr.starts_with('(') && expr.ends_with(')') {
        let inner = &expr[1..expr.len() - 1];
        if parens_balanced(inner) {
            let translated = translate_expr(inner)?;
            return Some(format!("({translated})"));
        }
    }

    None
}

pub(super) fn is_numeric_literal(s: &str) -> bool {
    // Check hex first (before stripping suffixes, since F is a hex digit)
    if s.starts_with("0x") || s.starts_with("0X") {
        let hex = s.trim_end_matches(['u', 'U', 'l', 'L']);
        return hex.len() > 2 && hex[2..].chars().all(|c| c.is_ascii_hexdigit() || c == '_');
    }
    let s = s.trim_end_matches(['l', 'L', 'u', 'U', 'f', 'F']);
    s.parse::<i64>().is_ok() || s.parse::<f64>().is_ok()
}

pub(super) fn clean_numeric_literal(s: &str) -> String {
    s.trim_end_matches(['l', 'L', 'u', 'U', 'f', 'F'])
        .to_string()
}

fn parens_balanced(s: &str) -> bool {
    let mut depth: i32 = 0;
    for ch in s.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Find an operator in an expression, respecting parenthesized sub-expressions.
pub(super) fn find_operator(expr: &str, op: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    let bytes = expr.as_bytes();
    let op_bytes = op.as_bytes();
    if bytes.len() < op_bytes.len() {
        return None;
    }
    for i in 0..=bytes.len() - op_bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && &bytes[i..i + op_bytes.len()] == op_bytes {
            return Some(i);
        }
    }
    None
}

fn try_translate_ternary(expr: &str) -> Option<String> {
    let q_pos = find_operator(expr, " ? ")?;
    let rest = &expr[q_pos + 3..];
    let c_pos = find_operator(rest, " : ")?;

    let cond = translate_expr(expr[..q_pos].trim())?;
    let then_expr = translate_expr(rest[..c_pos].trim())?;
    let else_expr = translate_expr(rest[c_pos + 3..].trim())?;

    Some(format!(
        "if {cond} {{ {then_expr} }} else {{ {else_expr} }}"
    ))
}
