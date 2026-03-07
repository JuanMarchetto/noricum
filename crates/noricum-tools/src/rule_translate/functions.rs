//! Function-level translation: signatures, parameters, type mapping, mutation detection.

use super::statements::translate_body;

pub(super) fn translate_function(func: &super::CFunction) -> Option<String> {
    let rust_ret = c_type_to_rust(&func.return_type)?;
    let mutated = find_mutated_vars(&func.body);
    let rust_params = translate_params_with_mut(&func.params, &mutated)?;
    let rust_body = translate_body(&func.body, "    ")?;

    let ret_str = if rust_ret == "()" {
        String::new()
    } else {
        format!(" -> {rust_ret}")
    };

    Some(format!(
        "pub fn {name}({params}){ret} {{\n{body}\n}}",
        name = func.name,
        params = rust_params,
        ret = ret_str,
        body = rust_body,
    ))
}

/// Find variables that are assigned to in the body (to mark params as `mut`).
pub(super) fn find_mutated_vars(body: &str) -> Vec<String> {
    let is_ident = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_');
    let mut seen = std::collections::HashSet::new();
    let mut vars = Vec::new();

    let mut add = |name: &str| {
        if seen.insert(name.to_string()) {
            vars.push(name.to_string());
        }
    };

    for line in body.lines() {
        let trimmed = line.trim();

        // x = ...; or x += ...; etc.
        for op in &["=", "+=", "-=", "*=", "/=", "%="] {
            if let Some(pos) = trimmed.find(op) {
                if *op == "=" {
                    let prev_is_compound = pos > 0
                        && matches!(
                            trimmed.as_bytes()[pos - 1],
                            b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%'
                        );
                    let next_is_eq = pos + 1 < trimmed.len() && trimmed.as_bytes()[pos + 1] == b'=';
                    if prev_is_compound || next_is_eq {
                        continue;
                    }
                }
                let lhs = trimmed[..pos].trim();
                if is_ident(lhs) {
                    add(lhs);
                }
                break;
            }
        }

        // x++, x--, ++x, --x
        let no_semi = trimmed.strip_suffix(';').unwrap_or(trimmed).trim();
        for var in [
            no_semi.strip_suffix("++"),
            no_semi.strip_suffix("--"),
            no_semi.strip_prefix("++"),
            no_semi.strip_prefix("--"),
        ]
        .into_iter()
        .flatten()
        {
            if is_ident(var) {
                add(var);
            }
        }
    }
    vars
}

fn translate_params_with_mut(params: &[(String, String)], mutated: &[String]) -> Option<String> {
    let mut rust_params = Vec::new();
    for (ty, name) in params {
        let rust_ty = c_type_to_rust(ty)?;
        if mutated.contains(name) {
            rust_params.push(format!("mut {name}: {rust_ty}"));
        } else {
            rust_params.push(format!("{name}: {rust_ty}"));
        }
    }
    Some(rust_params.join(", "))
}

pub(super) fn c_type_to_rust(c_type: &str) -> Option<&'static str> {
    match c_type.trim() {
        "int" => Some("i32"),
        "unsigned int" | "unsigned" => Some("u32"),
        "long" | "long long" => Some("i64"),
        "unsigned long" | "unsigned long long" => Some("u64"),
        "short" => Some("i16"),
        "unsigned short" => Some("u16"),
        "char" => Some("i8"),
        "unsigned char" => Some("u8"),
        "float" => Some("f32"),
        "double" => Some("f64"),
        "void" => Some("()"),
        "size_t" => Some("usize"),
        "int32_t" => Some("i32"),
        "uint32_t" => Some("u32"),
        "int64_t" => Some("i64"),
        "uint64_t" => Some("u64"),
        "int16_t" => Some("i16"),
        "uint16_t" => Some("u16"),
        "int8_t" => Some("i8"),
        "uint8_t" => Some("u8"),
        "bool" | "_Bool" => Some("bool"),
        _ => None,
    }
}

pub(super) fn default_for_type(rust_type: &str) -> &str {
    match rust_type {
        "f32" | "f64" => "0.0",
        "bool" => "false",
        _ => "0",
    }
}
