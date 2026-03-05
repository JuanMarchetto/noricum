/// HTML report generation for migration results.
///
/// Generates a standalone HTML file with:
/// - Side-by-side C vs Rust source (with basic syntax highlighting)
/// - Migration state, difficulty, score, unsafe count
/// - Diff test results
/// - Timing and cost metrics
use noricum_ir::FunctionUnit;

/// Generate a standalone HTML report for a single function unit.
pub fn generate_html_report(unit: &FunctionUnit) -> String {
    let name = html_escape(&unit.name);
    let state = format!("{:?}", unit.state);
    let difficulty = unit
        .difficulty
        .map(|d| format!("{d:?}"))
        .unwrap_or_else(|| "N/A".to_string());
    let score = unit
        .idiomatic_score
        .map(|s| format!("{s}/100"))
        .unwrap_or_else(|| "N/A".to_string());
    let unsafe_count = unit
        .unsafe_count
        .map(|u| u.to_string())
        .unwrap_or_else(|| "N/A".to_string());

    let c_source = html_escape(&unit.c_source);
    let rust_source = unit
        .rust_output
        .as_deref()
        .map(html_escape)
        .unwrap_or_else(|| "(no output)".to_string());

    let diff_test_html = match unit.metrics.diff_test_passed {
        Some(true) => r#"<span class="pass">PASSED</span>"#.to_string(),
        Some(false) => r#"<span class="fail">FAILED</span>"#.to_string(),
        None => "N/A".to_string(),
    };

    let score_value = unit.idiomatic_score.unwrap_or(0);
    let score_class = if score_value >= 90 {
        "score-high"
    } else if score_value >= 70 {
        "score-mid"
    } else {
        "score-low"
    };

    let tests_html = unit
        .generated_tests
        .as_deref()
        .map(|t| {
            format!(
                r#"<section><h2>Generated Tests</h2><pre><code class="language-rust">{}</code></pre></section>"#,
                html_escape(t)
            )
        })
        .unwrap_or_default();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Noricum Migration Report: {name}</title>
<style>
:root {{
  --bg: #0d1117; --fg: #e6edf3; --card: #161b22; --border: #30363d;
  --accent: #58a6ff; --pass: #3fb950; --fail: #f85149; --warn: #d29922;
}}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: var(--bg); color: var(--fg); padding: 2rem; }}
h1 {{ color: var(--accent); margin-bottom: 1rem; font-size: 1.8rem; }}
h2 {{ color: var(--accent); margin: 1.5rem 0 0.75rem; font-size: 1.2rem; border-bottom: 1px solid var(--border); padding-bottom: 0.5rem; }}
.header {{ display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 1rem; }}
.badge {{ display: inline-block; padding: 0.25rem 0.75rem; border-radius: 2rem; font-weight: 600; font-size: 0.85rem; }}
.pass {{ background: #1a3a2a; color: var(--pass); }}
.fail {{ background: #3a1a1a; color: var(--fail); }}
.state {{ background: #1a2a3a; color: var(--accent); }}
.metrics {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 1rem; margin: 1rem 0; }}
.metric {{ background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 1rem; text-align: center; }}
.metric .value {{ font-size: 1.8rem; font-weight: 700; }}
.metric .label {{ font-size: 0.8rem; color: #8b949e; margin-top: 0.25rem; }}
.score-high .value {{ color: var(--pass); }}
.score-mid .value {{ color: var(--warn); }}
.score-low .value {{ color: var(--fail); }}
.side-by-side {{ display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; margin: 1rem 0; }}
.code-panel {{ background: var(--card); border: 1px solid var(--border); border-radius: 8px; overflow: hidden; }}
.code-panel h3 {{ background: var(--border); padding: 0.5rem 1rem; font-size: 0.9rem; }}
.code-panel pre {{ padding: 1rem; overflow-x: auto; font-size: 0.85rem; line-height: 1.5; }}
code {{ font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace; }}
section {{ margin-bottom: 2rem; }}
.footer {{ margin-top: 3rem; padding-top: 1rem; border-top: 1px solid var(--border); color: #8b949e; font-size: 0.8rem; }}
@media (max-width: 768px) {{ .side-by-side {{ grid-template-columns: 1fr; }} }}
</style>
</head>
<body>
<div class="header">
  <h1>Noricum Migration Report</h1>
  <div>
    <span class="badge state">{state}</span>
  </div>
</div>

<section>
<h2>Summary</h2>
<div class="metrics">
  <div class="metric">
    <div class="value">{name}</div>
    <div class="label">Function</div>
  </div>
  <div class="metric">
    <div class="value">{difficulty}</div>
    <div class="label">Difficulty</div>
  </div>
  <div class="metric {score_class}">
    <div class="value">{score}</div>
    <div class="label">Idiomatic Score</div>
  </div>
  <div class="metric">
    <div class="value">{unsafe_count}</div>
    <div class="label">Unsafe Blocks</div>
  </div>
  <div class="metric">
    <div class="value">{diff_test_html}</div>
    <div class="label">Diff Test</div>
  </div>
</div>
</section>

<section>
<h2>Source Code</h2>
<div class="side-by-side">
  <div class="code-panel">
    <h3>C (Original)</h3>
    <pre><code class="language-c">{c_source}</code></pre>
  </div>
  <div class="code-panel">
    <h3>Rust (Migrated)</h3>
    <pre><code class="language-rust">{rust_source}</code></pre>
  </div>
</div>
</section>

<section>
<h2>Pipeline Metrics</h2>
<div class="metrics">
  <div class="metric">
    <div class="value">{total_ms}ms</div>
    <div class="label">Total Time</div>
  </div>
  <div class="metric">
    <div class="value">{llm_calls}</div>
    <div class="label">LLM Calls</div>
  </div>
  <div class="metric">
    <div class="value">{repair_iters}</div>
    <div class="label">Repair Iterations</div>
  </div>
  <div class="metric">
    <div class="value">{c_lines}L → {rust_lines}L</div>
    <div class="label">C Lines → Rust Lines</div>
  </div>
</div>
</section>

{tests_html}

<div class="footer">
  Generated by <strong>Noricum</strong> — Autonomous C/C++ to Rust Migration Agent
</div>
</body>
</html>"#,
        name = name,
        state = state,
        difficulty = difficulty,
        score = score,
        score_class = score_class,
        unsafe_count = unsafe_count,
        diff_test_html = diff_test_html,
        c_source = c_source,
        rust_source = rust_source,
        total_ms = unit.metrics.total_ms,
        llm_calls = unit.metrics.llm_calls,
        repair_iters = unit.metrics.repair_iterations,
        c_lines = unit.metrics.c_lines,
        rust_lines = unit.metrics.rust_lines,
        tests_html = tests_html,
    )
}

/// Generate an HTML report for multiple function units (project-level).
pub fn generate_project_html_report(units: &[FunctionUnit], project_name: &str) -> String {
    let mut rows = String::new();
    for unit in units {
        let name = html_escape(&unit.name);
        let state = format!("{:?}", unit.state);
        let score = unit.idiomatic_score.unwrap_or(0);
        let unsafe_count = unit.unsafe_count.unwrap_or(0);
        let diff = match unit.metrics.diff_test_passed {
            Some(true) => r#"<span class="pass">PASS</span>"#,
            Some(false) => r#"<span class="fail">FAIL</span>"#,
            None => "N/A",
        };
        let score_class = if score >= 90 {
            "pass"
        } else if score >= 70 {
            "warn"
        } else {
            "fail"
        };

        rows.push_str(&format!(
            r#"<tr>
  <td>{name}</td>
  <td><span class="badge state">{state}</span></td>
  <td><span class="{score_class}">{score}/100</span></td>
  <td>{unsafe_count}</td>
  <td>{diff}</td>
  <td>{total_ms}ms</td>
  <td>{calls}</td>
</tr>
"#,
            name = name,
            state = state,
            score_class = score_class,
            score = score,
            unsafe_count = unsafe_count,
            diff = diff,
            total_ms = unit.metrics.total_ms,
            calls = unit.metrics.llm_calls,
        ));
    }

    let total = units.len();
    let validated = units
        .iter()
        .filter(|u| u.state == noricum_ir::MigrationState::Validated)
        .count();
    let project_name = html_escape(project_name);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Noricum Project Report: {project_name}</title>
<style>
:root {{
  --bg: #0d1117; --fg: #e6edf3; --card: #161b22; --border: #30363d;
  --accent: #58a6ff; --pass: #3fb950; --fail: #f85149; --warn: #d29922;
}}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: var(--bg); color: var(--fg); padding: 2rem; }}
h1 {{ color: var(--accent); margin-bottom: 0.5rem; }}
h2 {{ color: var(--accent); margin: 1.5rem 0 0.75rem; border-bottom: 1px solid var(--border); padding-bottom: 0.5rem; }}
.summary {{ font-size: 1.1rem; margin-bottom: 1.5rem; color: #8b949e; }}
table {{ width: 100%; border-collapse: collapse; margin: 1rem 0; }}
th, td {{ padding: 0.75rem 1rem; text-align: left; border-bottom: 1px solid var(--border); }}
th {{ background: var(--card); font-weight: 600; }}
tr:hover {{ background: var(--card); }}
.badge {{ display: inline-block; padding: 0.2rem 0.6rem; border-radius: 2rem; font-weight: 600; font-size: 0.8rem; }}
.pass {{ color: var(--pass); }} .fail {{ color: var(--fail); }} .warn {{ color: var(--warn); }}
.state {{ background: #1a2a3a; color: var(--accent); }}
.footer {{ margin-top: 3rem; padding-top: 1rem; border-top: 1px solid var(--border); color: #8b949e; font-size: 0.8rem; }}
</style>
</head>
<body>
<h1>Noricum Project Report</h1>
<p class="summary">{project_name} — {validated}/{total} functions validated</p>

<h2>Migration Results</h2>
<table>
<thead><tr>
  <th>Function</th><th>State</th><th>Score</th><th>Unsafe</th><th>Diff Test</th><th>Time</th><th>LLM Calls</th>
</tr></thead>
<tbody>
{rows}
</tbody>
</table>

<div class="footer">
  Generated by <strong>Noricum</strong> — Autonomous C/C++ to Rust Migration Agent
</div>
</body>
</html>"#,
        project_name = project_name,
        validated = validated,
        total = total,
        rows = rows,
    )
}

/// Escape HTML special characters.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<div>"), "&lt;div&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"hello\""), "&quot;hello&quot;");
    }

    #[test]
    fn test_generate_html_report_basic() {
        let unit = FunctionUnit::new(
            "test_fn".into(),
            "test.c".into(),
            "int add(int a, int b) { return a + b; }".into(),
        );
        let html = generate_html_report(&unit);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("test_fn"));
        assert!(html.contains("Noricum Migration Report"));
        assert!(html.contains("int add"));
    }

    #[test]
    fn test_generate_html_report_with_rust_output() {
        let mut unit = FunctionUnit::new(
            "add".into(),
            "add.c".into(),
            "int add(int a, int b) { return a + b; }".into(),
        );
        unit.rust_output = Some("pub fn add(a: i32, b: i32) -> i32 { a + b }".into());
        unit.idiomatic_score = Some(95);
        unit.unsafe_count = Some(0);
        let html = generate_html_report(&unit);
        assert!(html.contains("pub fn add"));
        assert!(html.contains("95/100"));
        assert!(html.contains("score-high"));
    }

    #[test]
    fn test_generate_project_html_report() {
        let units = vec![
            FunctionUnit::new("a".into(), "a.c".into(), "int a() { return 1; }".into()),
            FunctionUnit::new("b".into(), "b.c".into(), "int b() { return 2; }".into()),
        ];
        let html = generate_project_html_report(&units, "test_project");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("test_project"));
        assert!(html.contains("0/2 functions validated"));
    }
}
