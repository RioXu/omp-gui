fn main() {
    // GPUI's composed native element trees use substantial stack space in
    // unoptimized Windows builds; reserve an explicit desktop-sized UI stack.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bin=omp-gui=/STACK:8388608");
    }
}
