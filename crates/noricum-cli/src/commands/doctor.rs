use anyhow::Result;

pub fn cmd_doctor() -> Result<()> {
    println!("Noricum v{}", env!("CARGO_PKG_VERSION"));
    println!();

    // Check C compiler
    print!("  C compiler (cc): ");
    match std::process::Command::new("cc").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let first_line = version.lines().next().unwrap_or("unknown");
            println!("OK ({first_line})");
        }
        _ => println!("NOT FOUND"),
    }

    // Check Rust compiler
    print!("  Rust compiler (rustc): ");
    match std::process::Command::new("rustc")
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => {
            println!("OK ({})", String::from_utf8_lossy(&output.stdout).trim());
        }
        _ => println!("NOT FOUND"),
    }

    // Check c2rust
    print!("  C2Rust (c2rust): ");
    match noricum_tools::c2rust::check_c2rust_available() {
        Ok(version) => println!("OK ({version})"),
        Err(_) => println!("NOT FOUND (install with: cargo install c2rust)"),
    }

    // Check clippy
    print!("  Clippy (clippy-driver): ");
    match std::process::Command::new("clippy-driver")
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => {
            println!("OK ({})", String::from_utf8_lossy(&output.stdout).trim());
        }
        _ => println!("NOT FOUND (install with: rustup component add clippy)"),
    }

    // Check C preprocessor
    print!("  C preprocessor: ");
    if noricum_tools::preprocessor::preprocessor_available() {
        println!("OK");
    } else {
        println!("NOT FOUND");
    }

    // Check Ollama
    print!("  Ollama: ");
    match std::process::Command::new("ollama")
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stderr);
            let ver_str = version.trim();
            let ver_display = if ver_str.is_empty() {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            } else {
                ver_str.to_string()
            };
            println!("OK ({ver_display})");

            // Check installed models
            print!("  Ollama models: ");
            match std::process::Command::new("ollama").arg("list").output() {
                Ok(list_output) if list_output.status.success() => {
                    let list_str = String::from_utf8_lossy(&list_output.stdout);
                    let model_count = list_str.lines().skip(1).filter(|l| !l.trim().is_empty()).count();
                    if model_count == 0 {
                        println!("none (run: ollama pull qwen2.5-coder:32b)");
                    } else {
                        println!("{model_count} model(s) installed");
                    }
                }
                _ => println!("could not list models"),
            }
        }
        _ => println!("NOT FOUND (install from: https://ollama.ai)"),
    }

    println!();
    println!(
        "  ANTHROPIC_API_KEY: {}",
        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            "set"
        } else {
            "not set"
        }
    );

    Ok(())
}
