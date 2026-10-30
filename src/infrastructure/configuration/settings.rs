//! Native OMP configuration. The installed CLI supplies the evolving registry.
use crate::infrastructure::process::hidden_command;
use anyhow::{Context, ensure};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};
use yaml_edit::{Document, path::YamlPath};

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub root: PathBuf,
    pub entries: BTreeMap<String, Value>,
    pub global: BTreeMap<String, Value>,
    pub project: BTreeMap<String, Value>,
    pub prompt_files: BTreeMap<String, Vec<PathBuf>>,
}
pub(crate) fn command(omp: &str, cwd: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = hidden_command(omp).args(args).current_dir(cwd).output()?;
    ensure!(
        out.status.success(),
        "OMP 配置命令失败：{}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8(out.stdout)?)
}
static CACHE: std::sync::LazyLock<std::sync::Mutex<BTreeMap<String, Catalog>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(BTreeMap::new()));
fn cache_key(omp: &str, cwd: &Path) -> String {
    format!("{omp}|{}", cwd.display())
}
pub fn cached(omp: &str, cwd: &Path) -> Option<Catalog> {
    CACHE.lock().ok()?.get(&cache_key(omp, cwd)).cloned()
}
pub fn catalog(omp: &str, cwd: &Path) -> anyhow::Result<Catalog> {
    let results = std::thread::scope(|scope| {
        let args: [&[&str]; 3] = [
            &["config", "path"],
            &["config", "list", "--json"],
            &["config", "list"],
        ];
        let jobs = args
            .into_iter()
            .map(|args| scope.spawn(move || command(omp, cwd, args)))
            .collect::<Vec<_>>();
        jobs.into_iter()
            .map(|job| {
                job.join()
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("OMP 配置查询线程中断")))
            })
            .collect::<anyhow::Result<Vec<_>>>()
    })?;
    let root = PathBuf::from(results[0].trim());
    ensure!(root.is_absolute(), "OMP 返回了无效的配置目录");
    let mut entries: BTreeMap<String, Value> = serde_json::from_str(&results[1])?;
    for line in results[2].lines() {
        let plain = strip_ansi(line);
        let plain = plain.trim();
        let Some((key, _)) = plain.split_once(" = ") else {
            continue;
        };
        if let Some(meta) = entries.get_mut(key)
            && meta["type"] == "enum"
            && let Some((_, tail)) = plain.rsplit_once('(')
            && let Some(values) = tail.strip_suffix(')')
        {
            meta["values"] = serde_json::json!(values.split('|').collect::<Vec<_>>());
        }
    }
    let mut prompt_files = BTreeMap::new();
    for (scope, base) in [("global", root.clone()), ("project", cwd.join(".omp"))] {
        for folder in ["skills", "agents"] {
            let mut paths = fs::read_dir(base.join(folder))
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| {
                    if folder == "skills" {
                        e.path().join("SKILL.md")
                    } else {
                        e.path()
                    }
                })
                .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "md"))
                .collect::<Vec<_>>();
            paths.sort();
            prompt_files.insert(format!("{scope}/{folder}"), paths);
        }
    }
    let global = configured_layer(&read_document(&config_path(&root))?, &entries)?;
    let project = configured_layer(&read_document(&config_path(&cwd.join(".omp")))?, &entries)?;
    let result = Catalog {
        root,
        entries,
        global,
        project,
        prompt_files,
    };
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(cache_key(omp, cwd), result.clone());
    }
    Ok(result)
}

// The CLI supplies effective values and the schema. Explicit values belong to
// their config files; `config list --scope` is not a portable OMP CLI command.
fn configured_layer(
    text: &str,
    entries: &BTreeMap<String, Value>,
) -> anyhow::Result<BTreeMap<String, Value>> {
    let document: Value = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_yaml_ng::from_str(text).context("OMP 配置 YAML 无效")?
    };
    let mut result = BTreeMap::new();
    for key in entries.keys() {
        let value = document.get(key).or_else(|| {
            key.split('.')
                .try_fold(&document, |node, part| node.get(part))
        });
        if let Some(value) = value {
            result.insert(key.clone(), serde_json::json!({"value":value}));
        }
    }
    Ok(result)
}
fn strip_ansi(text: &str) -> String {
    let mut result = String::new();
    let mut escape = false;
    for c in text.chars() {
        if c == '\u{1b}' {
            escape = true;
        } else if escape {
            if c.is_ascii_alphabetic() {
                escape = false;
            }
        } else {
            result.push(c);
        }
    }
    result
}
pub(crate) fn category(key: &str) -> &'static str {
    if key.starts_with("skills.") {
        "技能"
    } else if key.starts_with("mcp.") {
        "MCP"
    } else if key.starts_with("lsp.") {
        "LSP"
    } else if key.starts_with("advisor.")
        || key.starts_with("task.")
        || key.starts_with("modelRole")
        || key.starts_with("modelPreset")
        || key.starts_with("modelTag")
    {
        "角色与提示词"
    } else if key.starts_with("searxng.")
        || key.starts_with("exa.")
        || key.starts_with("web_search.")
        || key.contains("webSearch")
    {
        "网络搜索"
    } else if [
        "tools.", "bash.", "edit.", "read.", "eval.", "python.", "find.", "grep.",
    ]
    .iter()
    .any(|p| key.starts_with(p))
    {
        "工具调用"
    } else if ["model.", "providers.", "retry.", "compaction.", "tier."]
        .iter()
        .any(|p| key.starts_with(p))
        || [
            "temperature",
            "topP",
            "topK",
            "minP",
            "presencePenalty",
            "repetitionPenalty",
            "textVerbosity",
            "enabledModels",
            "defaultThinkingLevel",
        ]
        .contains(&key)
    {
        "模型"
    } else {
        "更多设置"
    }
}
pub(crate) fn config_path(base: &Path) -> PathBuf {
    let yml = base.join("config.yml");
    if !yml.exists() && base.join("config.yaml").exists() {
        base.join("config.yaml")
    } else {
        yml
    }
}
pub(crate) fn yaml_path(base: &Path, stem: &str) -> PathBuf {
    ["yml", "yaml", "json"]
        .iter()
        .map(|ext| base.join(format!("{stem}.{ext}")))
        .find(|p| p.exists())
        .unwrap_or_else(|| base.join(format!("{stem}.yml")))
}
pub(crate) fn read_document(path: &Path) -> anyhow::Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    ensure!(
        fs::metadata(path)?.len() < 2 * 1024 * 1024,
        "配置文件超过 2 MB"
    );
    fs::read_to_string(path).context("无法读取配置文件")
}
pub(crate) fn validate(path: &Path, text: &str) -> anyhow::Result<()> {
    if path.extension().is_some_and(|x| x == "md") {
        ensure!(!text.trim().is_empty(), "提示词不能为空");
        if text.starts_with("---") {
            let rest = text
                .trim_start_matches('-')
                .trim_start_matches(['\r', '\n']);
            let front = rest.split("\n---").next().unwrap_or("");
            let _: Value = serde_yaml_ng::from_str(front).context("Markdown frontmatter 无效")?;
        }
    } else {
        let value: Value = if path.extension().is_some_and(|x| x == "json") {
            serde_json::from_str(text).context("JSON 格式无效")?
        } else {
            serde_yaml_ng::from_str(text).context("YAML 格式无效")?
        };
        ensure!(value.is_object() || value.is_null(), "配置根节点必须是对象");
        if path.file_stem().is_some_and(|s| s == "models") {
            ensure!(
                value.get("providers").is_some_and(Value::is_object),
                "模型配置需要 providers 对象"
            );
        }
    }
    Ok(())
}
pub(crate) fn save_document(path: &Path, expected: &str, text: &str) -> anyhow::Result<()> {
    validate(path, text)?;
    let target = if path.exists() {
        fs::canonicalize(path)?
    } else {
        path.to_owned()
    };
    fs::create_dir_all(target.parent().context("配置路径无父目录")?)?;
    let backup_root =
        crate::infrastructure::persistence::workspace_store::data_dir().join("config-backups");
    fs::create_dir_all(&backup_root)?;
    let file_key = crate::domain::hash(&target.to_string_lossy());
    let lock_path = backup_root.join(format!("{file_key}.lock"));
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.try_lock().context("另一个窗口正在保存配置")?;
    ensure!(
        read_document(&target)? == expected,
        "文件已被 OMP 或其他程序修改，请重新载入后再编辑；本次未覆盖"
    );
    if target.exists() {
        fs::copy(&target, backup_root.join(format!("{file_key}.bak")))?;
    }
    let tmp = target.with_extension(format!("{}.tmp", crate::domain::id()));
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    drop(f);
    ensure!(
        read_document(&target)? == expected,
        "保存前文件发生变化，未覆盖"
    );
    fs::rename(tmp, target)?;
    Ok(())
}
pub(crate) fn set_project(path: &Path, key: &str, value: Option<Value>) -> anyhow::Result<()> {
    let original = read_document(path)?;
    let result = patch_yaml(&original, key, value)?;
    save_document(path, &original, &result)
}
pub(crate) fn patch_yaml(text: &str, key: &str, value: Option<Value>) -> anyhow::Result<String> {
    let source = if text.trim().is_empty() { "{}\n" } else { text };
    let parsed: Value = serde_yaml_ng::from_str(source)?;
    ensure!(parsed.is_object(), "配置根节点必须是对象");
    let doc = Document::from_str(source)?;
    if let Some(value) = value {
        let value_doc =
            Document::from_str(&format!("value: {}\n", serde_json::to_string(&value)?))?;
        doc.try_set_path(key, value_doc.get("value").context("无效设置值")?)?;
    } else {
        let parts: Vec<_> = key.split('.').collect();
        let mut map = doc.as_mapping().context("配置根节点必须是对象")?;
        for part in &parts[..parts.len() - 1] {
            let Some(next) = map.get_mapping(*part) else {
                return Ok(text.to_owned());
            };
            map = next;
        }
        map.remove(*parts.last().unwrap());
    }
    let result = doc.to_string();
    let _: Value = serde_yaml_ng::from_str(&result)?;
    Ok(result)
}
pub(crate) fn parse_value(kind: &str, input: &str) -> anyhow::Result<Value> {
    if kind == "string" || kind == "enum" {
        return Ok(Value::String(input.into()));
    }
    let value: Value = serde_json::from_str(input).context("请输入有效的 JSON 值")?;
    ensure!(
        match kind {
            "boolean" => value.is_boolean(),
            "number" => value.is_number(),
            "array" => value.is_array(),
            "record" | "object" => value.is_object(),
            _ => true,
        },
        "值的类型应为 {kind}"
    );
    Ok(value)
}
pub(crate) fn native_name(name: &str) -> anyhow::Result<&str> {
    ensure!(
        !name.is_empty()
            && name.len() <= 80
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "名称只允许字母、数字、- 和 _"
    );
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_layers_preserve_nested_and_dotted_values_without_inventing_defaults() {
        let entries = [
            "defaultThinkingLevel",
            "lsp.enabled",
            "modelRoles",
            "unconfigured",
        ]
        .map(|key| (key.to_owned(), Value::Null))
        .into_iter()
        .collect();
        let layer = configured_layer("defaultThinkingLevel: auto\nlsp:\n  enabled: false\nmodelRoles: {default: deepseek/model}\n", &entries).unwrap();
        assert_eq!(layer["defaultThinkingLevel"]["value"], "auto");
        assert_eq!(layer["lsp.enabled"]["value"], false);
        assert_eq!(layer["modelRoles"]["value"]["default"], "deepseek/model");
        assert!(!layer.contains_key("unconfigured"));
        assert_eq!(
            configured_layer("lsp.enabled: true", &entries).unwrap()["lsp.enabled"]["value"],
            true
        );
        assert!(configured_layer("", &entries).unwrap().is_empty());
    }
    #[test]
    fn enum_display_strips_terminal_styles() {
        assert_eq!(
            strip_ansi("\u{1b}[32mtools.mode\u{1b}[0m = low (low|high)"),
            "tools.mode = low (low|high)"
        );
    }
    #[test]
    fn yaml_edits_preserve_unknowns_comments_and_types() {
        let original = "# keep\nfuture: {enabled: true}\nlsp:\n  enabled: false # preserve neighboring comment\n  lazy: true\n";
        let changed = patch_yaml(original, "lsp.enabled", Some(Value::Bool(true))).unwrap();
        assert!(changed.contains("# keep"));
        assert!(changed.contains("future: {enabled: true}"));
        let parsed: Value = serde_yaml_ng::from_str(&changed).unwrap();
        assert_eq!(parsed["lsp"]["enabled"], true);
        let changed = patch_yaml(
            &changed,
            "modelRoles",
            Some(serde_json::json!({"default":"provider/model","advisor":"other/model"})),
        )
        .unwrap();
        let parsed: Value = serde_yaml_ng::from_str(&changed).unwrap();
        assert_eq!(parsed["modelRoles"]["advisor"], "other/model");
        let reset = patch_yaml(&changed, "lsp.enabled", None).unwrap();
        let parsed: Value = serde_yaml_ng::from_str(&reset).unwrap();
        assert!(parsed["lsp"].get("enabled").is_none());
        assert_eq!(parsed["lsp"]["lazy"], true);
    }
    #[test]
    fn stale_documents_cannot_overwrite_external_changes() {
        let p = std::env::temp_dir().join(format!("omp-settings-{}.yml", crate::domain::id()));
        fs::write(&p, "a: 2\n").unwrap();
        assert!(save_document(&p, "a: 1\n", "a: 3\n").is_err());
        assert_eq!(read_document(&p).unwrap(), "a: 2\n");
    }
}
