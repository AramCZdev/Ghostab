fn main() {
    println!("cargo:rustc-link-search=native={}/vendor/lib", env!("CARGO_MANIFEST_DIR"));
}
