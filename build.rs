fn main() {
    use std::{io::Write, path::Path};
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("remote_sources.rs");
    let mut files = vec![
        "Cargo.toml".to_owned(),
        "Cargo.lock".into(),
        "src/lib.rs".into(),
        "src/main.rs".into(),
        "build.rs".into(),
        "resources/omp-services.json".into(),
        "resources/OMP-LICENSE.txt".into(),
    ];
    fn collect(root: &Path, path: &Path, files: &mut Vec<String>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                if entry.file_name() != "target" {
                    collect(root, &entry.path(), files);
                }
            } else if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "rs")
                || entry.file_name() == "Cargo.toml"
            {
                files.push(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    for directory in ["crates/omp-core", "crates/omp-remote"] {
        collect(&root, &root.join(directory), &mut files);
    }
    files.sort();
    let mut generated = std::fs::File::create(out).unwrap();
    writeln!(generated, "pub const REMOTE_SOURCES: &[(&str,&[u8])] = &[").unwrap();
    for path in files {
        let absolute = root.join(&path);
        println!("cargo:rerun-if-changed={path}");
        writeln!(
            generated,
            "({path:?}, include_bytes!({:?})),",
            absolute.to_string_lossy()
        )
        .unwrap();
    }
    writeln!(generated, "];").unwrap();
    // GPUI's composed native element trees use substantial stack space in
    // unoptimized Windows builds; reserve an explicit desktop-sized UI stack.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bin=omp-gui=/STACK:8388608");
    }
}
