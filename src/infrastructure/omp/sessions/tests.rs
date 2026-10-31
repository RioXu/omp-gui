use super::*;
use crate::domain::{MessageState, process_blocks};

struct Fixture {
    base: PathBuf,
    store: SessionStore,
    project: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!("omp-gui-native-{}", uuid::Uuid::new_v4()));
        let project = base.join("project");
        fs::create_dir_all(&project).unwrap();
        let store = SessionStore {
            root: base.join("profile"),
            owners: base.join("owners"),
            cache: Default::default(),
        };
        Self {
            base,
            store,
            project,
        }
    }
    fn bound(&self, n: &NativeSession) -> Session {
        let mut s = Session::new(self.project.clone());
        s.omp_session = Some(n.path.to_string_lossy().into_owned());
        s.native_root = Some(self.store.root.to_string_lossy().into_owned());
        s.native_id = Some(n.id.clone());
        s
    }
    fn scan(&self, known: &[Session]) -> Scan {
        self.store
            .scan(
                std::slice::from_ref(&self.project),
                &known.iter().map(Registration::from).collect::<Vec<_>>(),
                &self.base.join("legacy"),
            )
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.base.is_absolute()
                && self.base.starts_with(std::env::temp_dir())
                && self
                    .base
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("omp-gui-native-")
        );
        let _ = fs::remove_dir_all(&self.base);
    }
}
fn append(path: &Path, rows: &[Value]) {
    let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
    for row in rows {
        writeln!(file, "{row}").unwrap();
    }
    file.sync_all().unwrap();
}
#[test]
fn native_buckets_match_home_temp_absolute_and_extended_windows_paths() {
    assert_eq!(
        bucket(
            Path::new(r"E:\dev\app"),
            Path::new(r"C:\Users\ls"),
            Path::new(r"C:\Users\ls\AppData\Local\Temp")
        ),
        "--E--dev-app--"
    );
    assert_eq!(
        bucket(
            Path::new(r"C:\Users\ls\app"),
            Path::new(r"C:\Users\ls"),
            Path::new(r"C:\Users\ls\AppData\Local\Temp")
        ),
        "-app"
    );
    assert_eq!(
        bucket(
            Path::new(r"C:\Users\ls\AppData\Local\Temp\app"),
            Path::new(r"C:\Users\ls"),
            Path::new(r"C:\Users\ls\AppData\Local\Temp")
        ),
        "-tmp-app"
    );
    assert_eq!(
        plain_path(Path::new(r"\\?\E:\dev\app")),
        PathBuf::from(r"E:\dev\app")
    );
    assert_eq!(
        plain_path(Path::new(r"\\?\UNC\server\share")),
        PathBuf::from(r"\\server\share")
    );
}
#[test]
fn native_creation_discovery_deletion_and_profile_boundaries() {
    let f = Fixture::new();
    let native = f.store.create(&f.project, "新建会话").unwrap();
    let s = f.bound(&native);
    assert!(
        !s.is_blank(),
        "durable empty sessions must survive GUI persistence"
    );
    assert_eq!(f.scan(&[]).sessions.len(), 1);
    let artifact = native.path.with_extension("");
    fs::create_dir(&artifact).unwrap();
    fs::write(artifact.join("attachment"), "data").unwrap();
    let backup = native.path.with_file_name(format!(
        "{}.42.bak",
        native.path.file_name().unwrap().to_string_lossy()
    ));
    fs::copy(&native.path, &backup).unwrap();
    let mut foreign = s.clone();
    foreign.native_root = Some(f.base.join("other-profile").to_string_lossy().into_owned());
    assert!(f.store.delete(&foreign, &f.base.join("legacy")).is_err());
    let held = lease::acquire(&f.store.owners, &native.id).unwrap();
    assert!(f.store.delete(&s, &f.base.join("legacy")).is_err());
    assert!(native.path.is_file());
    drop(held);
    f.store.delete(&s, &f.base.join("legacy")).unwrap();
    assert!(!native.path.exists() && !artifact.exists() && !backup.exists());
    assert_eq!(f.scan(std::slice::from_ref(&s)).missing, vec![s.id]);
    assert!(f.scan(&[]).sessions.is_empty());
    f.store.create(&f.project, "").unwrap();
    assert_eq!(
        f.scan(&[]).sessions.len(),
        1,
        "materialized terminal /new sessions are discoverable before the first prompt"
    );
}
#[test]
fn native_branch_history_title_slot_tools_and_partial_writes() {
    let f = Fixture::new();
    let native = f.store.create(&f.project, "header title").unwrap();
    append(
        &native.path,
        &[
            json!({"type":"message","id":"u1","parentId":null,"message":{"role":"user","content":"question","timestamp":1000}}),
            json!({"type":"message","id":"a1","parentId":"u1","message":{"role":"assistant","content":[{"type":"toolCall","id":"call","name":"read","arguments":{"path":"x"}}]}}),
            json!({"type":"message","id":"t1","parentId":"a1","message":{"role":"toolResult","toolCallId":"call","content":[{"type":"text","text":"file text"}]}}),
            json!({"type":"message","id":"discarded","parentId":"t1","message":{"role":"assistant","content":"discarded branch"}}),
            json!({"type":"message","id":"a2","parentId":"t1","message":{"role":"assistant","content":"retained answer"}}),
            json!({"type":"session_info","id":"name","parentId":"a2","name":"native renamed"}),
            json!({"type":"model_usage","id":"usage","parentId":"name","activeLeafId":"name"}),
        ],
    );
    let parsed = history::read(&native.path).unwrap();
    assert_eq!(parsed.title, "native renamed");
    assert_eq!(parsed.messages.len(), 3);
    assert_eq!(
        parsed.messages[1].process[0].result.as_deref(),
        Some("file text")
    );
    assert_eq!(parsed.messages[2].text, "retained answer");
    append(
        &native.path,
        &[
            json!({"type":"model_usage","id":"dangling-usage","parentId":"usage","activeLeafId":"missing-leaf"}),
        ],
    );
    assert_eq!(history::read(&native.path).unwrap().messages.len(), 3);
    let content = fs::read_to_string(&native.path).unwrap();
    fs::write(
        &native.path,
        format!(
            "{}\n{content}",
            json!({"type":"title","v":1,"title":"slot title"})
        ),
    )
    .unwrap();
    assert_eq!(f.scan(&[]).sessions[0].title, "slot title");
    fs::OpenOptions::new()
        .append(true)
        .open(&native.path)
        .unwrap()
        .write_all(b"{\"type\":")
        .unwrap();
    let scan = f.scan(&[f.bound(&native)]);
    assert!(scan.sessions.is_empty() && !scan.errors.is_empty() && scan.missing.is_empty());
    f.store
        .delete(&f.bound(&native), &f.base.join("legacy"))
        .unwrap();
    assert!(
        !native.path.exists(),
        "a torn final append must not prevent an explicit deletion"
    );
}
#[test]
fn migration_preserves_journal_artifacts_gui_identity_draft_and_archive() {
    let f = Fixture::new();
    let native = f.store.create(&f.project, "existing").unwrap();
    append(
        &native.path,
        &[
            json!({"type":"message","id":"u","parentId":null,"message":{"role":"user","content":"hello"}}),
        ],
    );
    let mut s = Session::new(f.project.clone());
    let legacy = f.base.join("legacy");
    let old = legacy.join(&s.id).join(native.path.file_name().unwrap());
    fs::create_dir_all(old.parent().unwrap()).unwrap();
    fs::rename(&native.path, &old).unwrap();
    fs::create_dir(old.with_extension("")).unwrap();
    fs::write(old.with_extension("").join("blob"), "original attachment").unwrap();
    s.omp_session = Some(old.to_string_lossy().into_owned());
    s.archived = true;
    s.draft = "my draft".into();
    let gui = Message::new("user", "hello".into());
    let gui_id = gui.id.clone();
    s.messages.push(gui);
    let scan = f
        .store
        .scan(
            std::slice::from_ref(&f.project),
            &[Registration::from(&s)],
            &legacy,
        )
        .unwrap();
    assert_eq!(scan.bindings.len(), 1);
    let migrated = &scan.bindings[0].1;
    assert_eq!(fs::read(&old).unwrap(), fs::read(&migrated.path).unwrap());
    assert_eq!(
        fs::read_to_string(migrated.path.with_extension("").join("blob")).unwrap(),
        "original attachment"
    );
    append(
        &migrated.path,
        &[
            json!({"type":"message","id":"after-crash","parentId":"u","message":{"role":"assistant","content":"new native turn"}}),
        ],
    );
    let retry = f
        .store
        .scan(
            std::slice::from_ref(&f.project),
            &[Registration::from(&s)],
            &legacy,
        )
        .unwrap();
    assert!(retry.errors.is_empty());
    assert_eq!(
        retry.bindings[0].1.messages.len(),
        2,
        "canonical history wins after migration publication but before binding persistence"
    );
    s.native_root = Some(f.store.root.to_string_lossy().into_owned());
    s.native_id = Some(migrated.id.clone());
    s.omp_session = Some(migrated.path.to_string_lossy().into_owned());
    s.merge_native_history(migrated.messages.as_ref().clone());
    assert_eq!(s.messages[0].id, gui_id);
    assert!(s.archived && s.draft == "my draft");
    assert!(
        f.store
            .scan(
                std::slice::from_ref(&f.project),
                &[Registration::from(&s)],
                &legacy
            )
            .unwrap()
            .bindings
            .is_empty()
    );
    f.store.delete(&s, &legacy).unwrap();
    assert!(!old.exists());
}
#[test]
fn same_bucket_collision_never_imports_unrelated_project_and_backups_prevent_false_deletion() {
    let f = Fixture::new();
    let native = f.store.create(&f.project, "ours").unwrap();
    let unrelated = f.base.join("different");
    fs::create_dir(&unrelated).unwrap();
    let other = f.store.create(&unrelated, "theirs").unwrap();
    fs::rename(
        &other.path,
        native.path.parent().unwrap().join("other.jsonl"),
    )
    .unwrap();
    assert_eq!(f.scan(&[]).sessions.len(), 1);
    let backup = native.path.with_file_name(format!(
        "{}.1.bak",
        native.path.file_name().unwrap().to_string_lossy()
    ));
    fs::rename(&native.path, &backup).unwrap();
    assert!(f.scan(&[f.bound(&native)]).missing.is_empty());
}
#[test]
fn merge_duplicate_prompts_preserves_anchors_and_augmented_comment_text() {
    let mut s = Session::new("project".into());
    let mut one = Message::new("user", "same".into());
    one.native_text = Some("same + comment context".into());
    one.comments.push("comment".into());
    let mut answer = Message::new("assistant", "reply".into());
    answer.process = process_blocks(
        &json!({"content":[{"type":"toolCall","id":"tool","name":"edit","arguments":{}}]}),
        "message_end",
        "",
    );
    answer.process[0].patch = Some("historical patch".into());
    let two = Message::new("user", "same".into());
    let ids = [one.id.clone(), answer.id.clone(), two.id.clone()];
    s.messages = vec![one, answer, two];
    let mut native: Vec<_> = [
        ("user", "same + comment context"),
        ("assistant", "reply"),
        ("user", "same"),
    ]
    .into_iter()
    .enumerate()
    .map(|(ix, (role, text))| {
        let mut m = Message::new(role, text.into());
        m.native_id = Some(format!("n{ix}"));
        m
    })
    .collect();
    native[1].process = s.messages[1].process.clone();
    native[1].process[0].patch = None;
    s.merge_native_history(native.clone());
    s.merge_native_history(native);
    assert_eq!(
        s.messages.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ids.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(s.messages[0].text, "same");
    assert_eq!(s.messages[0].comments, vec!["comment"]);
    assert_eq!(
        s.messages[1].process[0].patch.as_deref(),
        Some("historical patch")
    );
    let mut unsent = Message::new("user", "withdrawn".into());
    unsent.state = MessageState::Cancelled;
    s.messages.push(unsent);
    let rewind = s.messages[..1].to_vec();
    s.merge_native_history(rewind);
    assert_eq!(s.messages.len(), 2);
    assert_eq!(s.messages[1].text, "withdrawn");
}
