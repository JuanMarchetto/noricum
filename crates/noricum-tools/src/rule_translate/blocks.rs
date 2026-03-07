//! Block-level translation: printf, if/while/for chains, main function.

use super::expressions::{split_args, translate_expr};
use super::parsing::{extract_block_body, extract_paren_condition};
use super::statements::{translate_body, translate_statement, translate_var_decl};

/// Translate an if/else-if/else chain from lines starting at `start`.
/// Returns the translated block and the number of lines consumed.
pub(super) fn translate_if_chain(
    lines: &[&str],
    start: usize,
    indent: &str,
) -> Option<(String, usize)> {
    let mut result = String::new();
    let mut i = start;
    let inner_indent = format!("{indent}    ");

    // First "if"
    let trimmed = lines[i].trim();
    let cond = extract_paren_condition(trimmed)?;
    let rust_cond = translate_expr(&cond)?;

    let (body_str, body_end) = extract_block_body(lines, i)?;
    let body = translate_body(&body_str, &inner_indent)?;
    result.push_str(&format!("{indent}if {rust_cond} {{\n{body}\n{indent}}}"));

    // Check if the closing brace line also has "else"
    let closing_line = lines[body_end].trim();
    if closing_line.contains("else") {
        i = body_end; // stay on this line for else processing
    } else {
        i = body_end + 1;
    }

    // Else-if and else
    while i < lines.len() {
        let t = lines[i].trim();

        if t.contains("else if") {
            let cond = extract_paren_condition(t)?;
            let rust_cond = translate_expr(&cond)?;
            let (body_str, body_end) = extract_block_body(lines, i)?;
            let body = translate_body(&body_str, &inner_indent)?;
            result.push_str(&format!(" else if {rust_cond} {{\n{body}\n{indent}}}"));
            let closing = lines[body_end].trim();
            i = if closing.contains("else") {
                body_end
            } else {
                body_end + 1
            };
        } else if t.contains("else") {
            let (body_str, body_end) = extract_block_body(lines, i)?;
            let body = translate_body(&body_str, &inner_indent)?;
            result.push_str(&format!(" else {{\n{body}\n{indent}}}"));
            i = body_end + 1;
            break;
        } else {
            break;
        }
    }

    result.push('\n');
    Some((result, i - start))
}

pub(super) fn translate_while_block(
    lines: &[&str],
    start: usize,
    indent: &str,
) -> Option<(String, usize)> {
    let trimmed = lines[start].trim();
    let cond = extract_paren_condition(trimmed)?;
    let rust_cond = translate_expr(&cond)?;
    let inner_indent = format!("{indent}    ");

    let (body_str, body_end) = extract_block_body(lines, start)?;
    let body = translate_body(&body_str, &inner_indent)?;

    let result = format!("{indent}while {rust_cond} {{\n{body}\n{indent}}}\n");
    Some((result, body_end + 1 - start))
}

pub(super) fn translate_for_block(
    lines: &[&str],
    start: usize,
    indent: &str,
) -> Option<(String, usize)> {
    let trimmed = lines[start].trim();
    let inner_indent = format!("{indent}    ");

    // Extract for(...) content
    let for_header = extract_paren_condition(trimmed)?;
    let parts: Vec<&str> = for_header.splitn(3, ';').collect();
    if parts.len() != 3 {
        return None;
    }

    let init = parts[0].trim();
    let cond = parts[1].trim();
    let step = parts[2].trim();

    let rust_init = if init.is_empty() {
        String::new()
    } else if init.contains(' ') {
        // Variable declaration in init -- always mut since the step mutates it
        let decl = translate_var_decl(&format!("{init};"))?;
        decl.replacen("let ", "let mut ", 1)
    } else {
        translate_statement(&format!("{init};"))?
    };

    let rust_cond = if cond.is_empty() {
        "true".to_string()
    } else {
        translate_expr(cond)?
    };

    let rust_step = if step.is_empty() {
        String::new()
    } else {
        translate_statement(&format!("{step};"))?
    };

    let (body_str, body_end) = extract_block_body(lines, start)?;
    let body = translate_body(&body_str, &inner_indent)?;

    let mut result = String::new();
    if !rust_init.is_empty() {
        result.push_str(&format!("{indent}{rust_init}\n"));
    }
    result.push_str(&format!("{indent}while {rust_cond} {{\n{body}\n"));
    if !rust_step.is_empty() {
        result.push_str(&format!("{inner_indent}{rust_step}\n"));
    }
    result.push_str(&format!("{indent}}}\n"));

    Some((result, body_end + 1 - start))
}

pub(super) fn translate_printf(stmt: &str) -> Option<String> {
    let inner = stmt.strip_prefix("printf(")?.strip_suffix(");")?.trim();

    let fmt_start = inner.find('"')?;
    let fmt_end = inner[fmt_start + 1..].find('"')? + fmt_start + 1;
    let fmt_str = &inner[fmt_start + 1..fmt_end];

    // Longest specifiers first to avoid partial matches (e.g. %lld before %d)
    const FMT_MAP: &[(&str, &str)] = &[
        ("%lld", "{}"),
        ("%llu", "{}"),
        ("%ld", "{}"),
        ("%lu", "{}"),
        ("%lf", "{}"),
        ("%zu", "{}"),
        ("%d", "{}"),
        ("%i", "{}"),
        ("%u", "{}"),
        ("%f", "{}"),
        ("%s", "{}"),
        ("%c", "{}"),
        ("%x", "{:x}"),
        ("%X", "{:X}"),
        ("%o", "{:o}"),
        ("%p", "{:p}"),
        ("%%", "%"),
        ("\\n", ""),
    ];
    let mut rust_fmt = fmt_str.to_string();
    for &(from, to) in FMT_MAP {
        rust_fmt = rust_fmt.replace(from, to);
    }

    let after_fmt = &inner[fmt_end + 1..];
    let args_str = after_fmt.trim().strip_prefix(',').unwrap_or("").trim();

    if args_str.is_empty() {
        Some(format!("println!(\"{rust_fmt}\");"))
    } else {
        let args = split_args(args_str);
        let mut rust_args = Vec::new();
        for arg in &args {
            rust_args.push(translate_expr(arg.trim())?);
        }
        Some(format!(
            "println!(\"{rust_fmt}\", {});",
            rust_args.join(", ")
        ))
    }
}

pub(super) fn translate_main(body: &str) -> Option<String> {
    let cleaned = body.replace("return 0;", "");
    let body = translate_body(&cleaned, "    ")?;
    Some(format!("fn main() {{\n{body}\n}}"))
}
