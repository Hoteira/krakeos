fn main() {
    println!("cargo:rustc-link-arg=-Tkernel/src/linker.ld");
}
