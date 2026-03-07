#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let _ = noricum_validation::compute_idiomatic_score_from_source(0, 0, data, data);
});
