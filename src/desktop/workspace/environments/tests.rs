use super::EnvironmentEvent;
use crate::desktop::workspace::runtime::sessions::tests::fixture;
use gpui_kit::{AppContext, TestAppContext, WindowOptions, open_window};
use omp_core::{
    domain::Session,
    environment::{Connection, Environment, HostPath, ProjectLocation},
    host::{ExecutionHost, NativeStore},
    remote::RemoteHost,
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

#[gpui_kit::test]
fn deletion_blocks_reconnect_and_send_until_result_without_blocking_other_sessions(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| fixture(window, cx))
        })
        .unwrap()
    });
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |this, cx| {
                this.demo = false;
                this.save_disabled = false;
                // There is intentionally no host adapter: this test cannot launch OMP.
                this.db.active_environment = "offline-remote".into();
                for session in &mut this.db.sessions {
                    session.location = Some(ProjectLocation {
                        environment: "offline-remote".into(),
                        profile: None,
                        path: HostPath::Posix("/fixture".into()),
                    });
                }
                let id = this.db.sessions[0].id.clone();
                this.environments.deleting.insert(id.clone());
                this.runtime.status = "before periodic reconnect".into();
                this.connect();
                assert_eq!(this.runtime.status, "正在删除会话…");
                assert!(!this.runtime.starting);
                assert!(this.runtime.connection_result.is_none());
                let messages = this.db.sessions[0].messages.len();
                this.send(window, cx);
                assert_eq!(this.db.sessions[0].messages.len(), messages);
                assert!(this.runtime.pending_send.is_none());
                this.active = Some(1);
                assert!(!this.deleting_active_session());
                this.active = Some(0);
                this.environments
                    .tx
                    .send(EnvironmentEvent::Deleted {
                        id,
                        empty_only: false,
                        result: Err("external native owner".into()),
                    })
                    .unwrap();
                this.poll_environments(window, cx);
                assert!(!this.deleting_active_session());
                assert_eq!(this.db.sessions.len(), 2);
                assert_eq!(this.runtime.status, "删除失败，会话已保留");
                assert!(
                    this.runtime
                        .error
                        .as_deref()
                        .unwrap()
                        .contains("external native owner")
                );
            })
        })
        .unwrap();
}

#[gpui_kit::test]
#[ignore = "explicit SSH regression: deletes only two newly created owned QA sessions"]
fn remote_delete_active_session_survives_periodic_sync(cx: &mut TestAppContext) {
    let host = std::env::var("OMP_GUI_TEST_SSH_HOST").unwrap();
    let helper = std::env::var("OMP_GUI_TEST_HELPER").unwrap();
    let project = std::env::var("OMP_GUI_TEST_PROJECT").unwrap();
    assert!(project.contains("/.cache/omp-gui-tests/run-") && project.ends_with("/project"));
    let root = HostPath::posix(project).unwrap();
    let remote = Arc::new(RemoteHost::connect(&host, &helper).unwrap());
    let store = NativeStore::discover(ExecutionHost::Remote(remote.clone()), "omp", &root).unwrap();
    let mut sessions = Vec::new();
    for title in [
        "OMP GUI deletion race existing QA",
        "OMP GUI deletion race blank QA",
    ] {
        let native = store.create(&root, title).unwrap();
        let mut session = Session::new(Default::default());
        session.location = Some(ProjectLocation {
            environment: "deletion-qa".into(),
            profile: None,
            path: root.clone(),
        });
        session.title = title.into();
        session.draft = "owned QA draft".into();
        session.native_id = Some(native.id);
        session.native_root = Some(store.root.text());
        session.native_revision = Some(native.revision);
        session.omp_session = Some(native.path.to_string_lossy().into_owned());
        sessions.push(session);
    }
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| fixture(window, cx))
        })
        .unwrap()
    });
    handle
        .update(cx, |_, _, cx| {
            view.update(cx, |this, _| {
                this.demo = false;
                this.save_disabled = false;
                this.db.sessions = sessions.clone();
                this.db.active_environment = "deletion-qa".into();
                this.db.environments.push(Environment {
                    id: "deletion-qa".into(),
                    name: "deletion QA".into(),
                    connection: Connection::Ssh { host },
                    omp_override: None,
                    profile: None,
                    helper: Some(helper),
                });
                this.db.remote_projects = vec![sessions[0].project_location()];
                this.db.active = Some(sessions[0].id.clone());
                this.environments
                    .hosts
                    .insert("deletion-qa".into(), ExecutionHost::Remote(remote));
                this.session_sync.store = Some(store.clone());
                this.connect();
            })
        })
        .unwrap();
    // Run the same timers as the desktop, including periodic synchronization.
    // Report failures only after releasing our writer and cleaning our fixtures.
    let result = (|| -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            let ready = handle.update(cx, |_, window, cx| {
                view.update(cx, |this, cx| {
                    this.poll_engine(window, cx);
                    this.poll_sessions(window, cx);
                    this.runtime.connected
                        && !this.runtime.starting
                        && this.session_sync.store.is_some()
                })
            })?;
            if ready {
                break;
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "QA native connection did not become ready"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        for session in sessions.iter().rev() {
            let started = Instant::now();
            handle.update(cx, |_, window, cx| {
                view.update(cx, |this, cx| {
                    this.manage_session(&session.id, true, window, cx);
                })
            })?;
            loop {
                let (removed, error, reopened) = handle.update(cx, |_, window, cx| {
                    view.update(cx, |this, cx| {
                        this.poll_environments(window, cx);
                        this.poll_sessions(window, cx);
                        this.poll_engine(window, cx);
                        let deleting = this.deleting_active_session();
                        (
                            !this.db.sessions.iter().any(|saved| saved.id == session.id),
                            // This fixture has no disk persistence worker. Its
                            // expected save error is unrelated to native deletion.
                            this.runtime
                                .error
                                .clone()
                                .filter(|error| error != "保存服务已停止"),
                            deleting
                                && (this.runtime.engine.is_some()
                                    || this.runtime.starting
                                    || this.runtime.connection_result.is_some()),
                        )
                    })
                })?;
                anyhow::ensure!(!reopened, "periodic sync reopened a pending deletion");
                anyhow::ensure!(error.is_none(), "GUI deletion failed: {error:?}");
                if removed {
                    break;
                }
                anyhow::ensure!(
                    started.elapsed() < Duration::from_secs(30),
                    "deletion did not finish"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
            eprintln!("{} removed in {:?}", session.title, started.elapsed());
        }
        Ok(())
    })();
    handle
        .update(cx, |_, _, cx| {
            view.update(cx, |this, _| {
                this.runtime.engine = None;
                this.runtime.connection_result = None;
                this.background.clear();
            })
        })
        .unwrap();
    for session in &sessions {
        let deadline = Instant::now() + Duration::from_secs(20);
        while store
            .available(&root, session.native_id.as_deref().unwrap())
            .is_err()
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(100));
        }
        store.delete(session, Path::new("")).unwrap();
    }
    result.unwrap();
}
