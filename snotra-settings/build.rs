fn main() {
    let build_date = std::env::var("BUILD_DATE").unwrap_or_else(|_| "local".to_string());
    println!("cargo:rustc-env=APP_BUILD_DATE={build_date}");
    println!("cargo:rerun-if-env-changed=BUILD_DATE");
}
