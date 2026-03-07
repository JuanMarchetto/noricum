#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let _ = noricum_tools::rule_translate::try_translate(data, "fuzz_fn");
});
