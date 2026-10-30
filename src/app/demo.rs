use super::*;
pub(super) fn demo_session() -> Session {
    let mut s = Session::new(std::env::current_dir().unwrap_or_default());
    s.title = "优化会话列表 · 演示".into();
    s.messages.push(Message::new(
        "user",
        "会话多的时候切换项目有点慢，优化一下，同时保留最近打开的会话。".into(),
    ));
    let m=Message::new("assistant","会话列表已改为按项目分组，并保留最近访问顺序和当前选中状态。\n\n切换项目时会复用已加载的记录，重复打开同一项目不再触发完整扫描。性能提升还需要专项测量。".into());
    let phrase = "重复打开同一项目不再触发完整扫描";
    let start = m.text.find(phrase).unwrap();
    let a = Anchor::new(
        Source::Message(m.id.clone()),
        m.text.clone(),
        start..start + phrase.len(),
        phrase.into(),
    )
    .unwrap();
    s.messages.push(m);
    let snap = Snapshot::new(
        "src/ui/sidebar.rs".into(),
        "fn sessions() {\n    store.scan_all()\n}\n".into(),
        "fn sessions() {\n    store.cached_sessions()\n}\n".into(),
    );
    let doc = snap.document();
    let start = doc.find("store.cached_sessions()").unwrap();
    let code = Anchor::new(
        Source::File {
            path: snap.path.clone(),
            snapshot: snap.id.clone(),
        },
        doc,
        start..start + "store.cached_sessions()".len(),
        "store.cached_sessions()".into(),
    )
    .unwrap();
    s.snapshots.insert(snap.id.clone(), snap);
    let c1 = Comment::new(a, "这个结论有测试数据支撑吗？".into());
    let c2 = Comment::new(code, "缓存失效时会刷新吗？".into());
    s.comments.extend([c1, c2]);
    s.submit(String::new(), None);
    let reply = Message::new(
        "assistant",
        "还没有专项性能数据，需要补充测量。缓存失效检查也需要继续验证，评论保留为待确认。".into(),
    );
    for c in &mut s.comments {
        c.reply_ids.push(reply.id.clone());
    }
    s.messages.push(reply);
    s
}
