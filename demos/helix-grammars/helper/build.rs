fn main() {
    println!(
        "cargo:rustc-env=BUILD_TARGET={}",
        std::env::var("HOST").unwrap()
    );
}
