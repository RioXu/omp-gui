use omp_gui::infrastructure::configuration::{
    providers::{Providers, ServiceCatalog},
    settings::{cached, catalog},
};
use serde_json::{Value, json};

#[test]
#[ignore = "reads installed OMP registry"]
fn installed_registry_includes_configured_deepseek() {
    let omp = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("omp/omp.exe");
    let c = ServiceCatalog::load(omp.to_str().unwrap(), &std::env::current_dir().unwrap());
    assert!(c.warning.is_none(), "{:?}", c.warning);
    assert!(c.registered_providers().contains("deepseek"));
    assert!(c.available.iter().any(|s| s.starts_with("deepseek/")));
    assert!(c.search_options().len() >= 22);
    println!(
        "{} available models; {} providers in directory",
        c.available.len(),
        c.models.len()
    );
}

#[test]
#[ignore = "validates an isolated models.yml against installed OMP"]
fn installed_omp_accepts_minimal_native_model_override() {
    let root =
        std::env::temp_dir().join(format!("omp-gui-native-override-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = Providers::load(&root).unwrap();
    store.save("deepseek",Some(&json!({"apiKey":"fixture-only-not-a-secret","modelOverrides":{"deepseek-flash":{"contextWindow":2000000}}}))).unwrap();
    let omp = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("omp/omp.exe");
    let out = std::process::Command::new(omp.to_str().unwrap())
        .env("PI_CODING_AGENT_DIR", &root)
        .current_dir(&root)
        .args(["models", "ls", "deepseek", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let output: Value = serde_json::from_slice(&out.stdout).unwrap();
    let model = output["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "deepseek-flash")
        .expect("native model survives override");
    assert_eq!(model["contextWindow"], 2000000);
}

#[test]
#[ignore = "reads installed OMP registry for integration/performance verification"]
fn installed_catalog_exposes_native_enums_and_fast_cache() {
    let omp = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap())
        .join("omp/omp.exe")
        .to_string_lossy()
        .into_owned();
    let cwd = std::env::current_dir().unwrap();
    let start = std::time::Instant::now();
    let result = catalog(&omp, &cwd).unwrap();
    let cold = start.elapsed();
    assert!(
        result.entries["defaultThinkingLevel"]["values"]
            .as_array()
            .unwrap()
            .len()
            > 1
    );
    let start = std::time::Instant::now();
    assert!(cached(&omp, &cwd).is_some());
    println!(
        "registry: {} entries, cold {:?}, cached {:?}",
        result.entries.len(),
        cold,
        start.elapsed()
    );
}
