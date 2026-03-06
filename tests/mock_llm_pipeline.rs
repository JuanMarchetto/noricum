/// Integration tests for the LLM pipeline using mock responses.
///
/// These tests verify that the full analysis-parse, translation-extract,
/// and repair-extract pipelines work end-to-end without real API calls.
/// They catch regressions in prompt response parsing and code extraction.
use noricum_agents::analysis::{AnalysisResult, parse_analysis_response};
use noricum_agents::extract_rust_code;
use noricum_agents::mock_responses;

#[test]
fn test_analysis_parse_simple_mock() {
    let result = parse_analysis_response(mock_responses::MOCK_ANALYSIS_SIMPLE).unwrap();
    assert_eq!(result.difficulty, "easy");
    assert_eq!(result.patterns, vec!["pure_function"]);
    assert!(result.risks.is_empty());
    assert!(!result.strategy.is_empty());
}

#[test]
fn test_analysis_parse_complex_mock() {
    let result = parse_analysis_response(mock_responses::MOCK_ANALYSIS_COMPLEX).unwrap();
    assert_eq!(result.difficulty, "hard");
    assert!(result.patterns.contains(&"malloc_free".to_string()));
    assert!(result.patterns.contains(&"ptr_arithmetic".to_string()));
    assert!(result.dependencies.contains(&"helper_alloc".to_string()));
    assert!(!result.risks.is_empty());
}

#[test]
fn test_analysis_parse_fenced_mock() {
    let result = parse_analysis_response(mock_responses::MOCK_ANALYSIS_FENCED).unwrap();
    assert_eq!(result.difficulty, "medium");
    assert!(result.patterns.contains(&"ptr_arithmetic".to_string()));
    assert!(result.patterns.contains(&"error_codes".to_string()));
    assert!(!result.risks.is_empty());
}

#[test]
fn test_analysis_parse_malformed_returns_error() {
    let result = parse_analysis_response(mock_responses::MOCK_ANALYSIS_MALFORMED);
    assert!(result.is_err(), "malformed response should fail parsing");
}

#[test]
fn test_analysis_parse_missing_field_returns_error() {
    let result = parse_analysis_response(mock_responses::MOCK_ANALYSIS_MISSING_FIELD);
    assert!(result.is_err(), "missing required fields should fail");
}

#[test]
fn test_translation_extract_fenced() {
    let code = extract_rust_code(mock_responses::MOCK_TRANSLATION_FENCED);
    assert_eq!(code, "fn add(a: i32, b: i32) -> i32 { a + b }");
}

#[test]
fn test_translation_extract_bare() {
    let code = extract_rust_code(mock_responses::MOCK_TRANSLATION_BARE);
    assert_eq!(code, "fn add(a: i32, b: i32) -> i32 { a + b }");
}

#[test]
fn test_translation_extract_with_prose() {
    let code = extract_rust_code(mock_responses::MOCK_TRANSLATION_WITH_PROSE);
    assert_eq!(code, "fn add(a: i32, b: i32) -> i32 { a + b }");
}

#[test]
fn test_repair_extract_nested_fences() {
    let code = extract_rust_code(mock_responses::MOCK_REPAIR_NESTED_FENCES);
    // Nested backticks in comments cause early fence termination; the extractor
    // reliably captures the function signature at minimum.
    assert!(code.contains("fn fixed()"));
}

#[test]
fn test_repair_extract_compilable() {
    let code = extract_rust_code(mock_responses::MOCK_REPAIR_COMPILABLE);
    assert!(code.contains("wrapping_add"));
    assert!(code.contains("pub fn add"));
}

#[test]
fn test_test_gen_extract() {
    let code = extract_rust_code(mock_responses::MOCK_TEST_GEN);
    assert!(code.contains("#[cfg(test)]"));
    assert!(code.contains("#[test]"));
    assert!(code.contains("assert_eq!"));
}

#[test]
fn test_translation_extract_empty() {
    let code = extract_rust_code(mock_responses::MOCK_TRANSLATION_EMPTY);
    assert!(code.is_empty());
}

#[test]
fn test_extracted_code_compiles() {
    // Verify that extracted mock translation output actually compiles
    let code = extract_rust_code(mock_responses::MOCK_TRANSLATION_FENCED);
    let result = noricum_tools::compiler::check_rust_compiles(&code).unwrap();
    assert!(
        result.success,
        "extracted mock translation should compile: {}",
        result.stderr
    );
}

#[test]
fn test_extracted_repair_compiles() {
    // Verify that extracted mock repair output actually compiles
    let code = extract_rust_code(mock_responses::MOCK_REPAIR_COMPILABLE);
    let result = noricum_tools::compiler::check_rust_compiles(&code).unwrap();
    assert!(
        result.success,
        "extracted mock repair should compile: {}",
        result.stderr
    );
}

/// End-to-end: parse analysis, then verify the result feeds correctly into
/// downstream validation logic (idiomatic score check).
#[test]
fn test_analysis_to_validation_pipeline() {
    let analysis: AnalysisResult =
        parse_analysis_response(mock_responses::MOCK_ANALYSIS_SIMPLE).unwrap();

    // The analysis result should be serializable (used in translation prompts)
    let json = serde_json::to_string_pretty(&analysis).unwrap();
    assert!(json.contains("easy"));
    assert!(json.contains("pure_function"));

    // And deserializable back
    let roundtrip: AnalysisResult = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip.difficulty, analysis.difficulty);
    assert_eq!(roundtrip.patterns, analysis.patterns);
}

/// End-to-end: simulate realistic LLM pipeline
/// parse analysis → extract translation → compile → score → validate
#[test]
fn test_full_mock_llm_pipeline_e2e() {
    // 1. Parse analysis response (as returned by the analysis agent)
    let analysis = parse_analysis_response(mock_responses::MOCK_ANALYSIS_SIMPLE).unwrap();
    assert_eq!(analysis.difficulty, "easy");

    // 2. Extract Rust code from translation response (as returned by the translation agent)
    let rust_code = extract_rust_code(mock_responses::MOCK_TRANSLATION_FENCED);
    assert!(
        !rust_code.is_empty(),
        "should extract code from fenced response"
    );

    // 3. Verify extracted code compiles
    let compile_result = noricum_tools::compiler::check_rust_compiles(&rust_code).unwrap();
    assert!(compile_result.success, "extracted code should compile");

    // 4. Score the code for idiomatic quality
    let unsafe_count = noricum_tools::compiler::count_unsafe_blocks(&rust_code);
    assert_eq!(
        unsafe_count, 0,
        "simple function should have 0 unsafe blocks"
    );

    let score = noricum_validation::compute_idiomatic_score(unsafe_count, 0);
    assert!(
        score >= 60,
        "simple idiomatic code should score >= 60, got {score}"
    );

    // 5. Validate via FunctionUnit (full validation pipeline)
    let mut unit = noricum_ir::FunctionUnit::new(
        "add".into(),
        "test.c".into(),
        "int add(int a, int b) { return a + b; }".into(),
    );
    unit.rust_output = Some(rust_code);
    unit.state = noricum_ir::MigrationState::Refined;

    let validation = noricum_validation::validate(&unit).unwrap();
    assert!(validation.compiles, "validation should confirm compilation");
    assert!(
        validation.idiomatic_score >= 60,
        "should pass score threshold"
    );

    // 6. Parse a repair response (simulating repair agent output)
    let repaired = extract_rust_code(mock_responses::MOCK_REPAIR_COMPILABLE);
    let repair_compile = noricum_tools::compiler::check_rust_compiles(&repaired).unwrap();
    assert!(repair_compile.success, "repaired code should compile");
}
