fn main() {
    println!("cargo:rustc-link-arg=-Twasm_runner/src/linker.ld");
}
