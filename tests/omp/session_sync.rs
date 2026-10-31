//! Explicitly isolated native session interoperability; no model prompts.
use omp_gui::infrastructure::omp::{Client, Event, sessions::SessionStore};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

#[test]
#[ignore = "uses installed OMP with an explicitly isolated PI_CODING_AGENT_DIR; sends no model prompt"]
fn installed_omp_discovers_gui_session_by_id_and_native_creation_is_imported() {
    let config = PathBuf::from(
        std::env::var_os("PI_CODING_AGENT_DIR").expect("set isolated PI_CODING_AGENT_DIR"),
    );
    let expected = std::env::current_dir()
        .unwrap()
        .join("target/session-sync-test");
    assert!(
        config.is_absolute() && config.starts_with(&expected),
        "test must not use real account storage"
    );
    fs::create_dir_all(&config).unwrap();
    // A registered model is required to start RPC even though no prompt is sent.
    fs::write(
        config.join("config.yml"),
        "modelRoles:\n  default: deepseek/deepseek-flash\n",
    )
    .unwrap();
    fs::write(
        config.join("models.yml"),
        "providers:\n  deepseek:\n    apiKey: fixture-only-not-a-secret\n",
    )
    .unwrap();
    let project = expected.join(format!("project-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&project).unwrap();
    let omp = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join("omp/omp.exe");
    let store = SessionStore::discover(omp.to_str().unwrap(), &project).unwrap();
    assert_eq!(store.root, config);
    let gui = store.create(&project, "GUI created").unwrap();
    let client = Client::start_project(omp.to_str().unwrap(), &project, &gui.id[..8]).unwrap();
    fn response(client: &Client, command: &str) -> Value {
        let end = Instant::now() + Duration::from_secs(60);
        while Instant::now() < end {
            match client.events.recv_timeout(Duration::from_secs(2)) {
                Ok(Event::Frame(f)) if f["type"] == "ready" => {
                    client.send(json!({"type":"negotiate_protocol","protocolVersion":2,"id":"protocol"})).unwrap();
                }
                Ok(Event::Frame(f)) if f["type"] == "response" && f["command"] == command => {
                    assert_ne!(f["success"], false, "{f}");
                    return f["data"].clone();
                }
                Ok(Event::Closed) => panic!("OMP closed"),
                Ok(Event::Error(e)) => eprintln!("OMP diagnostic: {e}"),
                _ => {}
            }
        }
        panic!("OMP response timeout: {command}");
    }
    client
        .send(json!({"type":"get_state","id":"state"}))
        .unwrap();
    let state = response(&client, "get_state");
    assert_eq!(state["sessionId"], gui.id);
    client
        .send(json!({"type":"set_session_name","id":"name","name":"native renamed"}))
        .unwrap();
    response(&client, "set_session_name");
    assert!(
        store.available(&gui.id).is_err(),
        "must share OMP's actual native ownership lease"
    );
    client
        .send(json!({"type":"new_session","id":"new"}))
        .unwrap();
    response(&client, "new_session");
    client
        .send(json!({"type":"set_session_name","id":"name2","name":"terminal created"}))
        .unwrap();
    response(&client, "set_session_name");
    client
        .send(json!({"type":"get_state","id":"state2"}))
        .unwrap();
    let other = response(&client, "get_state");
    let sessions = store.list(std::slice::from_ref(&project)).unwrap();
    assert!(
        sessions
            .iter()
            .any(|s| s.id == gui.id && s.title == "native renamed")
    );
    assert!(
        sessions
            .iter()
            .any(|s| s.id == other["sessionId"].as_str().unwrap() && s.title == "terminal created")
    );
    drop(client);
    store.remove(&gui).unwrap();
    assert!(
        !store
            .list(std::slice::from_ref(&project))
            .unwrap()
            .iter()
            .any(|s| s.id == gui.id)
    );
    println!(
        "Verified native ID resume, bidirectional discovery/title updates, ownership and deletion without model prompts."
    );
}
