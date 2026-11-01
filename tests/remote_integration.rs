//! Explicit SSH tests. Never run remote operations or model requests by default.
use anyhow::{Context, Result, ensure};
use omp_core::{
    domain::{Session, id},
    environment::{HostPath, ProjectLocation},
    host::{
        ConfigAction, Configuration, DirectoryBrowser, ExecutionHost, NativeStore, ProjectFiles,
    },
    infrastructure::{
        filesystem::attachments,
        omp::{Event, SessionConnection, message_text},
    },
    remote::{RemoteHost, protocol::Operation},
};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

fn fixture() -> Result<(Arc<RemoteHost>, HostPath)> {
    let host =
        std::env::var("OMP_GUI_TEST_SSH_HOST").context("set OMP_GUI_TEST_SSH_HOST explicitly")?;
    let helper =
        std::env::var("OMP_GUI_TEST_HELPER").context("set OMP_GUI_TEST_HELPER explicitly")?;
    let project = std::env::var("OMP_GUI_TEST_PROJECT")
        .context("set an isolated OMP_GUI_TEST_PROJECT explicitly")?;
    ensure!(
        project.contains("/.cache/omp-gui-tests/run-") && project.ends_with("/project"),
        "use the owned isolated test project"
    );
    Ok((
        Arc::new(RemoteHost::connect(&host, &helper)?),
        HostPath::posix(project)?,
    ))
}

#[test]
#[ignore = "crashes an isolated supervisor that owns only this test's native OMP"]
fn supervisor_crash_changes_epoch_without_replaying_the_prompt() -> Result<()> {
    let (remote, root) = fixture()?;
    let parent = root.parent().context("fixture parent")?.text();
    let nonce = id();
    let wrapper = format!("{parent}/crash-helper-{nonce}");
    let data = format!("{parent}/crash-state-{nonce}");
    let launch = format!(
        "#!/bin/sh\nexport OMP_GUI_DATA_DIR={}\nexec {} \"$@\"\n",
        omp_core::remote::ssh::quote(&data)?,
        omp_core::remote::ssh::quote(&remote.helper)?
    );
    let setup = format!(
        "from pathlib import Path\np=Path({})\np.write_text({},encoding='utf-8')\np.chmod(0o700)\n",
        serde_json::to_string(&wrapper)?,
        serde_json::to_string(&launch)?
    );
    let output = omp_core::remote::ssh::ssh_script(
        &remote.host,
        &format!("python3 -c {}", omp_core::remote::ssh::quote(&setup)?),
    )?
    .output()?;
    ensure!(output.status.success(), "isolated supervisor setup failed");
    let isolated = Arc::new(RemoteHost::connect(&remote.host, &wrapper)?);
    let store = NativeStore::discover(ExecutionHost::Remote(isolated.clone()), "omp", &root)?;
    let native = store.create(&root, "OMP GUI supervisor crash fixture")?;
    let mut session = Session::new(Default::default());
    session.location = Some(ProjectLocation {
        environment: "ssh-crash".into(),
        profile: None,
        path: root.clone(),
    });
    session.native_root = Some(store.root.text());
    session.native_id = Some(native.id.clone());
    session.omp_session = Some(native.path.to_string_lossy().into_owned());
    let _cleanup = NativeCleanup {
        store: store.clone(),
        session: session.clone(),
        root: root.clone(),
    };
    let connection =
        isolated.start_session("omp", &root, session.omp_session.as_deref().unwrap())?;
    frame(&connection, Duration::from_secs(60), |value| {
        value["type"] == "ready"
    })?;
    connection.send(json!({"type":"negotiate_protocol","id":"qa-protocol","protocolVersion":2}))?;
    connection.send(json!({"type":"set_thinking_level","id":"qa-thinking","level":"off"}))?;
    let prompt = "Use the bash tool to execute `sleep 8; printf supervisor-proof-314159`. Then reply with the result. Do not edit any file.";
    connection.send(json!({"type":"prompt","id":"qa-crash-prompt","message":prompt}))?;
    frame(&connection, Duration::from_secs(180), |value| {
        value["type"] == "tool_execution_start"
    })?;
    // The wrapper isolates the socket/receipts; also verify the daemon's entire
    // child set before terminating it, so another session is never affected.
    let script = format!(
        "import os,signal,pathlib\ntarget={}\nroot={}\nhelper={}\nfound=[]\nfor name in os.listdir('/proc'):\n if not name.isdigit():continue\n try:\n  args=open('/proc/'+name+'/cmdline','rb').read().decode().split('\\0')\n  if '--resume' in args and args[args.index('--resume')+1]==target and os.readlink('/proc/'+name+'/cwd')==root:found.append(int(name))\n except (OSError,UnicodeError,IndexError):pass\nassert len(found)==1,found\npid=found[0]\nparent=int(next(line for line in open('/proc/'+str(pid)+'/status') if line.startswith('PPid:')).split()[1])\nargs=open('/proc/'+str(parent)+'/cmdline','rb').read().decode().split('\\0')\nassert args[0]==helper and '--daemon' in args\nchildren=set()\nfor task in pathlib.Path('/proc/'+str(parent)+'/task').iterdir():children.update(int(value) for value in (task/'children').read_text().split())\nassert children=={{pid}},children\nos.kill(parent,signal.SIGKILL)\n",
        serde_json::to_string(&session.omp_session.as_deref().unwrap())?,
        serde_json::to_string(&root.text())?,
        serde_json::to_string(&remote.helper)?
    );
    let output = omp_core::remote::ssh::ssh_script(
        &remote.host,
        &format!("python3 -c {}", omp_core::remote::ssh::quote(&script)?),
    )?
    .output()?;
    ensure!(
        output.status.success(),
        "supervisor ownership check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    frame(&connection, Duration::from_secs(30), |value| {
        value["type"] == "__omp_gui_transport" && value["state"] == "disconnected"
    })?;
    let restored = frame(&connection, Duration::from_secs(60), |value| {
        value["type"] == "__omp_gui_transport" && value["state"] == "connected"
    })?;
    ensure!(
        restored["epoch_changed"] == true && restored["gap"] == true,
        "service replacement was not disclosed"
    );
    connection.send(json!({"type":"get_state","id":"qa-after-crash"}))?;
    let state = frame(&connection, Duration::from_secs(30), |value| {
        value["id"] == "qa-after-crash"
    })?;
    ensure!(
        state["data"]["isSettled"] == true,
        "crashed work was automatically resumed"
    );
    connection.send(json!({"type":"get_messages","id":"qa-crash-history"}))?;
    let history = frame(&connection, Duration::from_secs(30), |value| {
        value["id"] == "qa-crash-history"
    })?;
    let messages = history["data"]["messages"]
        .as_array()
        .context("history shape")?;
    ensure!(
        messages
            .iter()
            .filter(|message| message["role"] == "user" && message_text(message) == prompt)
            .count()
            == 1,
        "crash recovery duplicated the prompt"
    );
    Ok(())
}

#[test]
#[ignore = "creates only owned Unicode/path/profile fixtures inside the configured test root"]
fn remote_paths_profiles_and_readonly_files_are_isolated() -> Result<()> {
    let (remote, root) = fixture()?;
    let script = format!(
        "import pathlib,os\nroot=pathlib.Path({})\nfolder=root/'目录 空格'\nfolder.mkdir(exist_ok=True)\n(folder/'a\\\\b.rs').write_text('unicode-path-proof\\n',encoding='utf-8')\nfile=folder/'readonly.txt'\nif file.exists():file.chmod(0o644)\nfile.write_text('readonly-proof\\n',encoding='utf-8')\nfile.chmod(0o444)\nlink=root/'linked-project'\nif not link.exists():link.symlink_to(folder,target_is_directory=True)\nassert link.resolve()==folder.resolve()\n",
        serde_json::to_string(&root.text())?
    );
    let output = omp_core::remote::ssh::ssh_script(
        &remote.host,
        &format!("python3 -c {}", omp_core::remote::ssh::quote(&script)?),
    )?
    .output()?;
    ensure!(
        output.status.success(),
        "fixture setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let nested = HostPath::posix(format!("{}/目录 空格", root.text()))?;
    let alias = HostPath::posix(format!("{}/linked-project", root.text()))?;
    ensure!(
        remote.browse(Some(&alias))?.path == nested,
        "symlink directory identity changed"
    );
    let snapshot = remote.document(&nested, Path::new(r"a\b.rs"), false)?.0;
    ensure!(
        snapshot.text == "unicode-path-proof\n",
        "POSIX backslash filename was normalized as a separator"
    );
    let (snapshot, target) = remote.document(&nested, Path::new("readonly.txt"), false)?;
    ensure!(
        remote
            .save(
                &nested,
                Path::new("readonly.txt"),
                &target.context("missing target")?,
                &snapshot.text,
                "must not overwrite"
            )
            .is_err(),
        "readonly file was overwritten"
    );
    ensure!(
        remote
            .document(&nested, Path::new("readonly.txt"), false)?
            .0
            .text
            == "readonly-proof\n",
        "readonly content changed"
    );
    ensure!(
        remote
            .call::<Value>(Operation::Hello {
                omp: "/omp-gui-deliberately-missing".into()
            })
            .is_err(),
        "missing executable accepted"
    );
    ensure!(
        remote.browse(Some(&root)).is_ok(),
        "application error closed the management channel"
    );
    let profile = format!(
        "{}/profile-中文",
        root.parent().context("fixture parent")?.text()
    );
    let separate = Arc::new(RemoteHost::connect_profile(
        &remote.host,
        &remote.helper,
        Some(&profile),
    )?);
    let store = NativeStore::discover(ExecutionHost::Remote(separate), "omp", &nested)?;
    ensure!(store.root.text() == profile, "profile override was ignored");
    let native = store.create(&nested, "OMP GUI profile isolation fixture")?;
    let mut session = Session::new(Default::default());
    session.location = Some(ProjectLocation {
        environment: "profile-test".into(),
        profile: Some(profile),
        path: nested.clone(),
    });
    session.native_root = Some(store.root.text());
    session.native_id = Some(native.id.clone());
    session.omp_session = Some(native.path.to_string_lossy().into_owned());
    let _cleanup = NativeCleanup {
        store: store.clone(),
        session,
        root: nested.clone(),
    };
    let default = NativeStore::discover(ExecutionHost::Remote(remote), "omp", &nested)?;
    ensure!(
        default.root != store.root,
        "profiles shared storage identity"
    );
    let scan = default.scan(&[nested], &[], Path::new(""))?;
    ensure!(
        !scan.sessions.iter().any(|session| session.id == native.id),
        "custom profile session leaked into default profile"
    );
    Ok(())
}

#[test]
#[ignore = "terminates only an OMP process bound to this test's newly created journal"]
fn remote_omp_exit_is_reported_without_replaying_work() -> Result<()> {
    let (remote, root) = fixture()?;
    let store = NativeStore::discover(ExecutionHost::Remote(remote.clone()), "omp", &root)?;
    let native = store.create(&root, "OMP GUI exit fixture")?;
    let mut session = Session::new(Default::default());
    session.location = Some(ProjectLocation {
        environment: "ssh-test".into(),
        profile: None,
        path: root.clone(),
    });
    session.native_root = Some(store.root.text());
    session.native_id = Some(native.id.clone());
    session.omp_session = Some(native.path.to_string_lossy().into_owned());
    let _cleanup = NativeCleanup {
        store: store.clone(),
        session: session.clone(),
        root: root.clone(),
    };
    let connection = remote.start_session("omp", &root, session.omp_session.as_deref().unwrap())?;
    frame(&connection, Duration::from_secs(60), |value| {
        value["type"] == "ready"
    })?;
    ensure!(
        store.available(&root, &native.id).is_err(),
        "native OMP ownership was not protected"
    );
    let script = format!(
        "import os,signal\ntarget={}\nroot={}\nfound=[]\nfor name in os.listdir('/proc'):\n if not name.isdigit(): continue\n try:\n  args=open('/proc/'+name+'/cmdline','rb').read().decode().split('\\0')\n  if '--resume' in args and args[args.index('--resume')+1]==target and os.readlink('/proc/'+name+'/cwd')==root:\n   found.append(int(name))\n except (OSError,UnicodeError,IndexError): pass\nassert len(found)==1,found\nos.kill(found[0],signal.SIGTERM)\n",
        serde_json::to_string(&session.omp_session.as_deref().unwrap())?,
        serde_json::to_string(&root.text())?
    );
    let output = omp_core::remote::ssh::ssh_script(
        &remote.host,
        &format!("python3 -c {}", omp_core::remote::ssh::quote(&script)?),
    )?
    .output()?;
    ensure!(
        output.status.success(),
        "owned native process termination failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let failure = frame(&connection, Duration::from_secs(30), |value| {
        value["type"] == "__omp_gui_transport" && value["state"] == "failed"
    })?;
    ensure!(
        failure["error"]
            .as_str()
            .is_some_and(|error| error.contains("退出")),
        "missing interruption status"
    );
    drop(connection);
    Ok(())
}

struct NativeCleanup {
    store: NativeStore,
    session: Session,
    root: HostPath,
}
impl Drop for NativeCleanup {
    fn drop(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while self
            .store
            .available(
                &self.root,
                self.session.native_id.as_deref().unwrap_or_default(),
            )
            .is_err()
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(100));
        }
        if let Err(error) = self.store.delete(&self.session, Path::new("")) {
            eprintln!("isolated native test cleanup needs retry: {error:#}");
        }
    }
}

#[test]
#[ignore = "interrupts an actual SSH bridge while a remote model tool is running"]
fn remote_task_survives_ssh_loss_and_gui_detachment_without_duplicate_prompts() -> Result<()> {
    let (remote, root) = fixture()?;
    let store = NativeStore::discover(ExecutionHost::Remote(remote.clone()), "omp", &root)?;
    let native = store.create(&root, "OMP GUI reconnect fixture")?;
    let mut session = Session::new(Default::default());
    session.location = Some(ProjectLocation {
        environment: "ssh-test".into(),
        profile: None,
        path: root.clone(),
    });
    session.native_root = Some(store.root.text());
    session.native_id = Some(native.id.clone());
    session.omp_session = Some(native.path.to_string_lossy().into_owned());
    let _cleanup = NativeCleanup {
        store: store.clone(),
        session: session.clone(),
        root: root.clone(),
    };
    let connection = remote.start_session("omp", &root, session.omp_session.as_deref().unwrap())?;
    frame(&connection, Duration::from_secs(60), |value| {
        value["type"] == "ready"
    })?;
    connection.send(json!({"type":"negotiate_protocol","id":"qa-protocol","protocolVersion":2}))?;
    connection.send(json!({"type":"set_thinking_level","id":"qa-thinking","level":"off"}))?;
    let rejected = remote.start_session("omp", &root, session.omp_session.as_deref().unwrap())?;
    let blocked = frame(&rejected, Duration::from_secs(30), |value| {
        value["type"] == "__omp_gui_transport" && value["state"] == "blocked"
    })?;
    ensure!(
        blocked["error"]
            .as_str()
            .is_some_and(|error| error.contains("另一个 GUI")),
        "multiple GUI ownership was not rejected"
    );
    drop(rejected);
    let prompt = "Use the bash tool to execute `sleep 8; printf reconnect-proof-927461`. Then reply with exactly reconnect-proof-927461. Do not edit any file.";
    connection.send(json!({"type":"prompt","id":"qa-reconnect-prompt","message":prompt}))?;
    frame(&connection, Duration::from_secs(180), |value| {
        value["type"] == "tool_execution_start"
    })?;
    connection.interrupt_transport();
    frame(&connection, Duration::from_secs(30), |value| {
        value["type"] == "__omp_gui_transport" && value["state"] == "disconnected"
    })?;
    frame(&connection, Duration::from_secs(60), |value| {
        value["type"] == "__omp_gui_transport" && value["state"] == "connected"
    })?;
    frame(&connection, Duration::from_secs(180), |value| {
        value["type"] == "message_end"
            && value["message"]["role"] == "assistant"
            && message_text(&value["message"]).trim() == "reconnect-proof-927461"
    })?;
    // Also simulate closing/reopening GUI: a fresh owner attaches to the same actor.
    drop(connection);
    std::thread::sleep(Duration::from_millis(500));
    let reopened = remote.start_session("omp", &root, session.omp_session.as_deref().unwrap())?;
    frame(&reopened, Duration::from_secs(60), |value| {
        value["type"] == "ready"
    })?;
    reopened.send(json!({"type":"get_messages","id":"qa-history"}))?;
    let history = frame(&reopened, Duration::from_secs(30), |value| {
        value["id"] == "qa-history"
    })?;
    let messages = history["data"]["messages"]
        .as_array()
        .context("native get_messages shape")?;
    ensure!(
        messages
            .iter()
            .filter(|message| message["role"] == "user" && message_text(message) == prompt)
            .count()
            == 1,
        "a prompt was duplicated across reconnection"
    );
    ensure!(
        messages.iter().any(|message| message["role"] == "assistant"
            && message_text(message).contains("reconnect-proof-927461")),
        "remote result was lost"
    );
    Ok(())
}

fn frame(
    connection: &dyn SessionConnection,
    timeout: Duration,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        for event in connection.poll(200) {
            match event {
                Event::Frame(value) => {
                    if std::env::var_os("OMP_GUI_TEST_TRACE").is_some() {
                        eprintln!(
                            "frame type={} id={} command={} state={} success={}",
                            value["type"],
                            value["id"],
                            value["command"],
                            value["state"],
                            value["success"]
                        );
                    }
                    if predicate(&value) {
                        return Ok(value);
                    }
                }
                Event::Error(error) => anyhow::bail!("OMP transport: {error}"),
                Event::Closed => anyhow::bail!("OMP closed before the expected event"),
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    anyhow::bail!("remote event deadline exceeded")
}

#[test]
#[ignore = "requires an explicitly configured isolated Linux SSH fixture"]
fn remote_host_files_configuration_and_conflicts() -> Result<()> {
    let (remote, root) = fixture()?;
    let hello: Value = remote.call(Operation::Hello { omp: "omp".into() })?;
    ensure!(
        hello["os"] == "linux"
            && hello["omp"]
                .as_str()
                .is_some_and(|path| path.starts_with('/')),
        "remote discovery failed"
    );
    let directory = remote.browse(Some(&root))?;
    ensure!(
        directory.path == root,
        "remote canonical directory changed unexpectedly"
    );
    let (files, _) = remote.lists(&root)?;
    ensure!(
        files
            .iter()
            .any(|path| path == Path::new("remote-check.txt")),
        "missing fixture file"
    );
    let (original, target) = remote.document(&root, Path::new("remote-check.txt"), false)?;
    let target = target.context("missing editable target")?;
    ensure!(
        original.text.contains("remote-code-1729"),
        "wrong host file content"
    );
    let edited = format!("{}GUI-save-check\n", original.text);
    remote.save(
        &root,
        Path::new("remote-check.txt"),
        &target,
        &original.text,
        &edited,
    )?;
    ensure!(
        remote
            .save(
                &root,
                Path::new("remote-check.txt"),
                &target,
                &original.text,
                "must not overwrite"
            )
            .is_err(),
        "stale version was accepted"
    );
    ensure!(
        remote.browse(Some(&root)).is_ok(),
        "a file conflict closed the SSH transport"
    );
    remote.save(
        &root,
        Path::new("remote-check.txt"),
        &target,
        &edited,
        &original.text,
    )?;
    let operation_id = id();
    let save = || Operation::SaveDocument {
        root: root.clone(),
        path: "remote-check.txt".into(),
        target: target.clone(),
        expected: original.text.clone(),
        text: edited.clone(),
    };
    remote.call_with_id::<()>(&operation_id, save())?;
    let reconnected = RemoteHost::connect(&remote.host, &remote.helper)?;
    let receipt: Option<omp_core::remote::receipts::Receipt> =
        reconnected.call(Operation::QueryOperation {
            id: operation_id.clone(),
        })?;
    ensure!(
        receipt.is_some_and(|receipt| receipt.result.is_some_and(|result| result.is_ok())),
        "completed save acknowledgement was not durable"
    );
    remote.save(
        &root,
        Path::new("remote-check.txt"),
        &target,
        &edited,
        &original.text,
    )?;
    // A late duplicate acknowledgement cannot overwrite a newer external edit.
    reconnected.call_with_id::<()>(&operation_id, save())?;
    ensure!(
        remote
            .document(&root, Path::new("remote-check.txt"), false)?
            .0
            .text
            == original.text,
        "duplicate operation overwrote a subsequent revision"
    );
    let configuration = Configuration {
        host: ExecutionHost::Remote(remote),
        omp: "omp".into(),
        cwd: root,
    };
    let catalog = configuration.catalog()?;
    ensure!(
        !catalog.root.as_os_str().is_empty(),
        "remote catalog root missing"
    );
    ensure!(
        !configuration.models()?.is_empty(),
        "remote configured models missing"
    );
    let base = configuration.join(Path::new(&configuration.cwd.text()), ".omp");
    let agent_path = configuration
        .join(&base, "agents/gui-qa-agent.md")
        .to_string_lossy()
        .into_owned();
    let mut agent = if configuration
        .call::<String>(ConfigAction::Read {
            path: agent_path.clone(),
        })?
        .is_empty()
    {
        omp_core::infrastructure::configuration::authoring::AgentDraft::default()
    } else {
        configuration.call(ConfigAction::LoadAgent { path: agent_path })?
    };
    let document = agent.document(
        "gui-qa-agent",
        "Isolated remote form test",
        "Reply only to test requests",
        None,
    )?;
    configuration.call::<()>(ConfigAction::SaveAgent {
        draft: Box::new(agent.clone()),
        base: base.to_string_lossy().into_owned(),
        name: "gui-qa-agent".into(),
        text: document,
    })?;
    agent = configuration.call(ConfigAction::LoadAgent {
        path: configuration
            .join(&base, "agents/gui-qa-agent.md")
            .to_string_lossy()
            .into_owned(),
    })?;
    ensure!(
        agent.name == "gui-qa-agent",
        "remote agent form saved to the wrong host"
    );
    Ok(())
}

#[test]
#[ignore = "submits a real model prompt through the configured Linux OMP"]
fn remote_omp_session_and_uploaded_file_reach_the_model() -> Result<()> {
    let (remote, root) = fixture()?;
    let host = ExecutionHost::Remote(remote);
    let store = NativeStore::discover(host.clone(), "omp", &root)?;
    let native = store.create(&root, "OMP GUI isolated SSH test")?;
    let mut session = Session::new(Default::default());
    session.location = Some(ProjectLocation {
        environment: "ssh-test".into(),
        profile: None,
        path: root.clone(),
    });
    session.native_root = Some(store.root.text());
    session.native_id = Some(native.id.clone());
    session.omp_session = Some(native.path.to_string_lossy().into_owned());
    let _cleanup = NativeCleanup {
        store: store.clone(),
        session: session.clone(),
        root: root.clone(),
    };
    let connection = store.connect(&root, session.omp_session.as_deref().unwrap(), &native.id)?;
    frame(connection.as_ref(), Duration::from_secs(60), |value| {
        value["type"] == "ready"
    })?;
    connection.send(json!({"type":"negotiate_protocol","id":"qa-protocol","protocolVersion":2}))?;
    connection.send(json!({"type":"get_state","id":"qa-state"}))?;
    let state = frame(connection.as_ref(), Duration::from_secs(30), |value| {
        value["id"] == "qa-state"
    })?;
    ensure!(state["success"] == true, "remote handshake rejected");
    connection.send(json!({"type":"set_thinking_level","id":"qa-thinking","level":"off"}))?;
    let local = std::env::temp_dir().join(format!("omp-gui-ssh-upload-{}", id()));
    std::fs::create_dir_all(&local)?;
    let source = local.join("远程附件.txt");
    std::fs::write(&source, "attachment-proof-684271\n")?;
    let attachment = attachments::import(
        &local.join("cache"),
        &session.id,
        attachments::Source::File(source),
    )?;
    let (prompt, images) = host.prepare_input(
        &session.id,
        "读取文件附件，并读取当前项目的 remote-check.txt。只回复两个校验字符串，不修改文件。",
        &[attachment],
    )?;
    ensure!(
        images.is_empty() && !prompt.contains("omp-gui-ssh-upload-"),
        "local attachment path leaked into the remote prompt"
    );
    connection.send(json!({"type":"prompt","id":"qa-prompt","message":prompt}))?;
    let response = frame(connection.as_ref(), Duration::from_secs(180), |value| {
        value["type"] == "message_end"
            && value["message"]["role"] == "assistant"
            && message_text(&value["message"]).contains("attachment-proof-684271")
            && message_text(&value["message"]).contains("remote-code-1729")
    })?;
    let answer = message_text(&response["message"]);
    ensure!(
        answer.contains("remote-code-1729"),
        "model did not read the remote project"
    );
    drop(connection);
    let deadline = Instant::now() + Duration::from_secs(15);
    while store.available(&root, &native.id).is_err() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let scan = store.scan(
        std::slice::from_ref(&root),
        std::slice::from_ref(&session),
        Path::new(""),
    )?;
    ensure!(
        scan.sessions
            .iter()
            .any(|saved| saved.id == native.id && !saved.messages.is_empty()),
        "native remote transcript was not persisted"
    );
    store.delete(&session, Path::new(""))?;
    ensure!(
        local.is_absolute()
            && local.starts_with(std::env::temp_dir())
            && local
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("omp-gui-ssh-upload-"),
        "unexpected local fixture identity"
    );
    std::fs::remove_dir_all(local)?;
    Ok(())
}
