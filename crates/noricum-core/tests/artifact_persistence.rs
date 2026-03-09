//! Integration tests for pipeline artifact persistence.

#[test]
fn test_artifact_store_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let store = noricum_core::artifacts::ArtifactStore::new(tmp.path(), "lifecycle_test").unwrap();

    // Simulate a full pipeline artifact trail
    store.save_c_source("int add(int a, int b) { return a + b; }").unwrap();
    store.save_analysis(r#"{"difficulty":"easy","patterns":["arithmetic"]}"#).unwrap();
    store.save_c2rust("unsafe fn add() {}").unwrap();
    store.save_translation_final("fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();
    store.save_initial_validation(r#"{"compiles":true,"idiomatic_score":90}"#).unwrap();
    store.save_final_output("fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();
    store.save_manifest(r#"{"name":"add","state":"Validated"}"#).unwrap();

    // Verify directory structure
    let dir = store.run_dir();
    assert!(dir.join("00-c-source.c").exists());
    assert!(dir.join("01-analysis.json").exists());
    assert!(dir.join("02-c2rust.rs").exists());
    assert!(dir.join("03-translation/final.rs").exists());
    assert!(dir.join("04-validation-initial.json").exists());
    assert!(dir.join("06-final.rs").exists());
    assert!(dir.join("manifest.json").exists());

    // Verify content round-trip
    let c_source = std::fs::read_to_string(dir.join("00-c-source.c")).unwrap();
    assert_eq!(c_source, "int add(int a, int b) { return a + b; }");
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
    assert!(manifest.contains("Validated"));
}

#[test]
fn test_artifact_store_repair_loop_simulation() {
    let tmp = tempfile::tempdir().unwrap();
    let store = noricum_core::artifacts::ArtifactStore::new(tmp.path(), "repair_test").unwrap();

    store.save_c_source("int complex() { /* ... */ }").unwrap();
    store.save_translation_final("fn complex() { todo!() }").unwrap();
    store.save_initial_validation(r#"{"compiles":false}"#).unwrap();

    // Simulate 3 repair iterations
    store.save_repair_iteration(1, "fn complex() { /* attempt 1 */ }", r#"{"compiles":false}"#).unwrap();
    store.save_repair_rejected(2, "unsafe fn complex() {}").unwrap();
    store.save_repair_iteration(3, "fn complex() -> i32 { 42 }", r#"{"compiles":true,"idiomatic_score":75}"#).unwrap();
    store.save_best_version("fn complex() -> i32 { 42 }", 75, 0, true).unwrap();
    store.save_final_output("fn complex() -> i32 { 42 }").unwrap();

    // All artifacts should exist
    let dir = store.run_dir();
    assert!(dir.join("05-repair/iter-01.rs").exists());
    assert!(dir.join("05-repair/iter-01-validation.json").exists());
    assert!(dir.join("05-repair/iter-02-rejected.rs").exists());
    assert!(dir.join("05-repair/iter-03.rs").exists());
    assert!(dir.join("05-repair/iter-03-validation.json").exists());
    assert!(dir.join("05-repair/best-version.rs").exists());
    assert!(dir.join("05-repair/best-version-meta.json").exists());
    assert!(dir.join("06-final.rs").exists());

    // Verify best-version meta
    let meta = std::fs::read_to_string(dir.join("05-repair/best-version-meta.json")).unwrap();
    assert!(meta.contains("\"score\":75"));
}

#[test]
fn test_artifact_store_chunked_translation_simulation() {
    let tmp = tempfile::tempdir().unwrap();
    let store = noricum_core::artifacts::ArtifactStore::new(tmp.path(), "chunked_test").unwrap();

    store.save_c_source("/* 2000 LOC C file */").unwrap();
    store.save_agreed_signatures("fn parse() -> Result<(), Error>;\nfn lex() -> Vec<Token>;").unwrap();
    store.save_foundation("struct Token { kind: TokenKind, span: Span }").unwrap();
    store.save_translation_chunk(0, "struct Token { kind: TokenKind, span: Span }").unwrap();
    store.save_translation_chunk(1, "fn lex() -> Vec<Token> { vec![] }").unwrap();
    store.save_translation_chunk(2, "fn parse() -> Result<(), Error> { Ok(()) }").unwrap();
    store.save_translation_final("// combined output").unwrap();

    let dir = store.run_dir();
    assert!(dir.join("03-translation/agreed-signatures.rs").exists());
    assert!(dir.join("03-translation/foundation.rs").exists());
    assert!(dir.join("03-translation/chunk-00.rs").exists());
    assert!(dir.join("03-translation/chunk-01.rs").exists());
    assert!(dir.join("03-translation/chunk-02.rs").exists());
    assert!(dir.join("03-translation/final.rs").exists());
}

#[test]
fn test_artifact_store_modular_migration_simulation() {
    let tmp = tempfile::tempdir().unwrap();
    let store = noricum_core::artifacts::ArtifactStore::new(tmp.path(), "modular_test").unwrap();

    store.save_translation_module("parser", "fn parse() {}").unwrap();
    store.save_translation_module("lexer", "fn lex() {}").unwrap();
    store.save_translation_module("eval", "fn eval() {}").unwrap();

    let dir = store.run_dir();
    assert!(dir.join("03-translation/module-parser.rs").exists());
    assert!(dir.join("03-translation/module-lexer.rs").exists());
    assert!(dir.join("03-translation/module-eval.rs").exists());
}

#[test]
fn test_artifact_store_retranslation_artifacts() {
    let tmp = tempfile::tempdir().unwrap();
    let store = noricum_core::artifacts::ArtifactStore::new(tmp.path(), "retrans_test").unwrap();

    store.save_retranslation("stub", "fn foo() { /* retranslated from stub */ }").unwrap();
    store.save_retranslation("unsafe", "fn bar() { /* retranslated to remove unsafe */ }").unwrap();
    store.save_retranslation_stall("fn baz() { /* stall retranslation */ }").unwrap();

    let dir = store.run_dir();
    assert!(dir.join("03-translation/retranslation-stub.rs").exists());
    assert!(dir.join("03-translation/retranslation-unsafe.rs").exists());
    assert!(dir.join("05-repair/retranslation-stall.rs").exists());

    let stall = std::fs::read_to_string(dir.join("05-repair/retranslation-stall.rs")).unwrap();
    assert!(stall.contains("stall retranslation"));
}
