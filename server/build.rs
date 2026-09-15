//! Compiles the vendored tree-sitter-janet-simple grammar.
//! `grammar/` is `src/` of sogaiu/tree-sitter-janet-simple at the rev pinned in extension.toml.

fn main() {
    let dir = std::path::Path::new("grammar");
    cc::Build::new()
        .include(dir)
        .file(dir.join("parser.c"))
        .file(dir.join("scanner.c"))
        .warnings(false)
        .compile("tree-sitter-janet-simple");
    println!("cargo:rerun-if-changed=grammar");
}
