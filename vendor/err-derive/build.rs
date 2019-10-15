use rustc_version::version;

fn version_ge(minor_min: u64) -> bool {
    let version = version().unwrap();

    version.major == 1 && version.minor >= minor_min
}

fn main() {
    if !version_ge(32) {
        panic!("`err-derive` depends on `quote 1.0`, which requires rustc >= 1.32");
    }
    generate_doc_tests()
}

#[cfg(feature = "skeptic")]
fn generate_doc_tests() {
    skeptic::generate_doc_tests(&["README.md"]);
}

#[cfg(not(feature = "skeptic"))]
fn generate_doc_tests() { }
