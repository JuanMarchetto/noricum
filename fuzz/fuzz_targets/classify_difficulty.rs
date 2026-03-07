#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let _ = noricum_tools::ast::classify_difficulty_ast(data);
});
