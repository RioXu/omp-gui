use super::*;
#[test]
fn direct_sessions_and_archive_delete_keep_project_mapping() {
    let project = PathBuf::from("plain-folder");
    let mut db = Database::default();
    db.projects.push(project.clone());
    let mut s = Session::new(project.clone());
    assert_eq!(s.cwd(), project);
    assert!(s.branch.is_none() && s.worktree.is_none());
    s.archived = true;
    let id = s.id.clone();
    db.active = Some(id.clone());
    db.sessions.push(s);
    let copy: Database = serde_json::from_str(&serde_json::to_string(&db).unwrap()).unwrap();
    assert!(copy.sessions[0].archived);
    db.remove_session(&id);
    assert!(db.sessions.is_empty());
    assert!(db.active.is_none());
    assert_eq!(db.projects, vec![project]);
}
#[test]
fn unicode_source_ranges_remain_exact() {
    let doc = "相同文字 / 相同文字".to_string();
    let start = doc.rfind("相同").unwrap();
    let a = Anchor::new(
        Source::Message("m".into()),
        doc.clone(),
        start..doc.len(),
        "相同文字".into(),
    )
    .unwrap();
    assert_eq!(&a.document[a.range], "相同文字");
    assert!(Anchor::new(Source::Message("m".into()), doc, 1..4, "相".into()).is_err());
}
#[test]
fn repeated_code_quotes_never_guess_a_source_position() {
    let a = Anchor::from_quote(
        Source::File {
            path: "file.py".into(),
            snapshot: "snapshot".into(),
        },
        "same\nsame\n".into(),
        "same".into(),
    )
    .unwrap();
    assert!(!a.exact_range);
    assert_eq!(a.document, "same\nsame\n");
    let a =
        Anchor::from_quote(a.source, "before\nunique\nafter\n".into(), "unique".into()).unwrap();
    assert!(a.exact_range);
    assert_eq!(&a.document[a.range], "unique");
}
#[test]
fn windows_diff_normalizes_presentation_but_preserves_snapshot_bytes() {
    let s = Snapshot::new(
        "file.py".into(),
        "same\nold\n".into(),
        "same\r\nnew\r\n".into(),
    );
    assert!(s.text.contains("\r\n"));
    assert!(!s.patch.contains("\r"));
    assert!(s.patch.contains(" same\n-old\n+new\n"));
}
#[test]
fn file_comments_preserve_history() {
    let s = Snapshot::new("a.rs".into(), "old\n".into(), "new\n".into());
    let doc = s.document();
    let a = Anchor::new(
        Source::File {
            path: s.path.clone(),
            snapshot: s.id.clone(),
        },
        doc.clone(),
        0..doc.len(),
        "new".into(),
    )
    .unwrap();
    let later = Snapshot::new(s.path.clone(), s.text.clone(), "replacement\n".into());
    assert_ne!(a.digest, hash(&later.document()));
    assert_eq!(a.document, doc);
}
#[test]
fn followups_append_and_reopen() {
    let mut s = Session::new("repo".into());
    let a = Anchor::new(
        Source::Message("source".into()),
        "abc".into(),
        0..3,
        "abc".into(),
    )
    .unwrap();
    let c = Comment::new(a, "question".into());
    let cid = c.id.clone();
    s.comments.push(c);
    s.submit(String::new(), None);
    s.comments[0].resolved = true;
    s.submit("followup".into(), Some(cid));
    assert_eq!(s.messages.len(), 2);
    assert_eq!(s.comments[0].text, "question");
    assert!(!s.comments[0].resolved);
    assert_eq!(s.comments[0].reply_ids.len(), 1);
}
