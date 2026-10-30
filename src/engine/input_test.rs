//! Uses the installed OMP account in a disposable directory.
use super::*;
use std::time::{Duration, Instant};

#[test]
#[ignore = "sends one authorized file-mention prompt to the installed OMP model"]
fn live_slash_commands_and_unicode_file_mentions() {
    let root = std::env::temp_dir().join(format!("omp-gui-input-{}", crate::domain::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("中文 note.txt"),
        "Verification token: FILE_REF_7319\n",
    )
    .unwrap();
    let omp = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("omp/omp.exe");
    let engine = Engine::start(omp.to_str().unwrap(), &root, &root.join("sessions"), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let (mut commands, mut renamed, mut answer, mut settled) = (false, false, false, false);
    while Instant::now() < deadline {
        let Ok(Event::Frame(frame)) = engine.events.recv_timeout(Duration::from_secs(1)) else {
            continue;
        };
        match frame["type"].as_str().unwrap_or_default() {
            "ready" => {
                engine
                    .send(json!({"type":"negotiate_protocol","id":"protocol","protocolVersion":2}))
                    .unwrap();
                engine
                    .send(json!({"type":"get_available_commands","id":"commands"}))
                    .unwrap();
            }
            "response" if frame["id"] == "commands" => {
                assert_eq!(frame["success"], true);
                commands = frame["data"]["commands"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["name"] == "rename");
                assert!(commands);
                engine.send(json!({"type":"prompt","id":"rename","message":"/rename GUI input verification"})).unwrap();
            }
            "response" if frame["id"] == "rename" => {
                assert_eq!(frame["success"], true);
                engine
                    .send(json!({"type":"get_state","id":"verify-name"}))
                    .unwrap();
            }
            "response" if frame["id"] == "verify-name" => {
                assert!(
                    frame["data"]["sessionName"]
                        .as_str()
                        .is_some_and(|name| name.contains("GUI input verification"))
                );
                renamed = true;
                engine.send(json!({"type":"prompt","id":"mention","message":"@\"中文 note.txt\" Reply with only the verification token contained in this attached file. Do not use tools or modify files."})).unwrap();
            }
            "message_end" if frame["message"]["role"] == "assistant" => {
                answer |= message_text(&frame["message"]).contains("FILE_REF_7319");
            }
            "prompt_result" if frame["id"] == "mention" => {
                assert_eq!(frame["status"], "completed");
                settled = true;
                break;
            }
            "response" if frame["success"] == false => {
                panic!("OMP request failed: {}", frame["command"])
            }
            _ => {}
        }
    }
    assert!(commands && renamed && answer && settled);
    assert_eq!(
        std::fs::read_to_string(root.join("中文 note.txt")).unwrap(),
        "Verification token: FILE_REF_7319\n"
    );
}
