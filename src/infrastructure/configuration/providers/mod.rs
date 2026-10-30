use crate::infrastructure::{configuration::settings, process::hidden_command};
use anyhow::{Context, ensure};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    str::FromStr,
};
use yaml_edit::Document;
mod catalog;
mod draft;
pub use catalog::ServiceCatalog;
pub(crate) use catalog::selector;
pub(crate) use draft::ProviderDraft;

#[derive(Clone, Default)]
pub struct Providers {
    pub path: PathBuf,
    pub original: String,
    pub values: serde_json::Map<String, Value>,
}
impl Providers {
    pub fn load(root: &Path) -> anyhow::Result<Self> {
        let path = settings::yaml_path(root, "models");
        let original = settings::read_document(&path)?;
        let document: Value = if original.trim().is_empty() {
            json!({})
        } else {
            serde_yaml_ng::from_str(&original)?
        };
        Ok(Self {
            path,
            original,
            values: document
                .get("providers")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
        })
    }
    pub fn save(&self, id: &str, value: Option<&Value>) -> anyhow::Result<()> {
        ensure!(
            !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)),
            "服务标识只允许字母、数字、-、_ 和 ."
        );
        let source = if self.original.trim().is_empty() {
            "providers: {}\n"
        } else {
            &self.original
        };
        let doc = Document::from_str(source)?;
        let root = doc.as_mapping().context("模型配置根节点无效")?;
        if root.get_mapping("providers").is_none() {
            let empty = Document::from_str("providers: {}")?;
            root.set("providers", empty.get("providers").unwrap());
        }
        let providers = root.get_mapping("providers").unwrap();
        if let Some(value) = value {
            let parsed = Document::from_str(&format!("entry: {}\n", value))?;
            providers.set(id, parsed.get("entry").unwrap());
        } else {
            providers.remove(id);
        }
        settings::save_document(&self.path, &self.original, &doc.to_string())
    }
}
pub(crate) fn import_key(omp: &str, cwd: &Path, provider: &str, key: &str) -> anyhow::Result<()> {
    let mut child = hidden_command(omp)
        .args(["auth", "import", "-"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let payload = json!({"version":1,"credentials":[{"provider":provider,"credential":{"type":"api_key","key":key,"source":"login"}}]});
    child
        .stdin
        .take()
        .context("无法打开凭据输入管道")?
        .write_all(payload.to_string().as_bytes())?;
    ensure!(
        child.wait()?.success(),
        "OMP 未能保存凭据，请检查该版本是否支持 auth import，或在 OMP 终端登录"
    );
    Ok(())
}
pub(crate) fn discover_models(base: &str, key: &str, api: &str) -> anyhow::Result<Vec<String>> {
    let mut url = reqwest::Url::parse(base).context("API 地址无效")?;
    ensure!(
        ["http", "https"].contains(&url.scheme())
            && url.username().is_empty()
            && url.password().is_none(),
        "请输入 HTTP(S) API 根地址，认证请填写在密钥字段"
    );
    url.set_path(&format!("{}/models", url.path().trim_end_matches('/')));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    ensure!(
        !key.starts_with('!'),
        "GUI 不执行命令凭据，请在 OMP 中验证此服务"
    );
    let credential = std::env::var(key).unwrap_or_else(|_| key.to_owned());
    let mut request = client.get(url);
    if !credential.is_empty() {
        if api == "anthropic-messages" {
            request = request
                .header("x-api-key", credential)
                .header("anthropic-version", "2023-06-01");
        } else {
            request = request.bearer_auth(credential);
        }
    }
    let response = request.send().context("无法连接服务，请检查地址与网络")?;
    ensure!(
        response.status().is_success(),
        "模型列表请求失败：HTTP {}",
        response.status().as_u16()
    );
    let body: Value = response.json().context("服务未返回模型列表 JSON")?;
    let mut ids = body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ensure!(
        !ids.is_empty(),
        "服务未提供兼容的 /models 列表，可手动添加模型"
    );
    Ok(ids)
}
pub(crate) fn save_scoped(
    omp: &str,
    cwd: &Path,
    root: &Path,
    project: bool,
    key: &str,
    value: Value,
) -> anyhow::Result<()> {
    if project {
        settings::set_project(&settings::config_path(&cwd.join(".omp")), key, Some(value))
    } else {
        let _ = root;
        settings::command(
            omp,
            cwd,
            &["config", "set", key, &value.to_string(), "--json"],
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_updates_keep_unknown_fields_and_dotted_names() {
        let root = std::env::temp_dir().join(format!("omp-gui-provider-{}", crate::domain::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("models.yml"),"# preserved\nproviders:\n  untouched:\n    baseUrl: https://example.invalid\n    future: true\n").unwrap();
        let loaded = Providers::load(&root).unwrap();
        loaded
            .save(
                "local.test",
                Some(&json!({"baseUrl":"http://localhost:1234/v1","models":[]})),
            )
            .unwrap();
        let updated = Providers::load(&root).unwrap();
        assert!(updated.original.contains("# preserved"));
        assert_eq!(updated.values["untouched"]["future"], true);
        assert!(updated.values.contains_key("local.test"));
        assert!(loaded.save("stale", Some(&json!({}))).is_err());
    }
    #[test]
    fn discovers_models_without_executing_a_prompt() {
        use std::{io::Read, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = socket.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            assert!(request.starts_with("GET /v1/models "));
            assert!(
                request
                    .to_lowercase()
                    .contains("authorization: bearer test-only")
            );
            let body = r#"{"data":[{"id":"b"},{"id":"a"},{"id":"a"}]}"#;
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        });
        assert_eq!(
            discover_models(
                &format!("http://{address}/v1"),
                "test-only",
                "openai-completions"
            )
            .unwrap(),
            vec!["a", "b"]
        );
        server.join().unwrap();
    }
}
