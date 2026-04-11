use std::env;
use std::io::Write;
use spike::contract::ZipReader;

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| "fixtures/zip_corpus/hello.zip".to_string());
    let mut reader = ZipReader::open(&path).expect("open");
    let body = reader.extract_to_mem(0).expect("extract");
    std::io::stdout().write_all(&body).unwrap();
}
