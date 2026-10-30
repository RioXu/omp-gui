//! Opt-in integration check: uses the local OMP account and a disposable project.
use super::*;
use crate::domain::{ProcessKind, process_blocks};
use std::time::{Duration, Instant};

#[test]
#[ignore = "sends one authorized prompt to the installed OMP model"]
fn live_process_stream_preserves_thinking_tools_and_final_answer() {
    let root = std::env::temp_dir().join(format!("omp-gui-stream-{}", crate::domain::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("note.txt"), "fixture value: 17\n").unwrap();
    let omp = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("omp/omp.exe");
    let engine = Engine::start(omp.to_str().unwrap(), &root, &root.join("sessions"), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(180);
    let (mut streamed, mut completed, mut tool, mut answer) = (false, false, false, false);
    let mut settled = false;
    while Instant::now() < deadline {
        match engine.events.recv_timeout(Duration::from_secs(2)) {
            Ok(Event::Frame(f)) => match f["type"].as_str().unwrap_or_default() {
                "ready" => {
                    engine.send(json!({"type":"negotiate_protocol","id":"protocol","protocolVersion":2})).unwrap();
                    engine.send(json!({"type":"prompt","id":"test","message":"Read note.txt with the read tool. Then multiply its value by 19. Give a brief final answer. Do not modify files or use other tools."})).unwrap();
                }
                event @ ("message_update" | "message_end") => {
                    let blocks = process_blocks(
                        &f["message"],
                        event,
                        f["assistantMessageEvent"]["type"]
                            .as_str()
                            .unwrap_or_default(),
                    );
                    streamed |= blocks
                        .iter()
                        .any(|b| b.kind == ProcessKind::Thinking && b.streaming);
                    completed |= blocks
                        .iter()
                        .any(|b| b.kind == ProcessKind::Thinking && !b.streaming);
                    tool |= blocks.iter().any(|b| b.kind == ProcessKind::Tool);
                    answer |= event == "message_end" && message_text(&f["message"]).contains("323");
                }
                "session_settled" => {
                    settled = true;
                    break;
                }
                "response" if f["success"] == false => {
                    panic!("native request failed: {}", f["command"])
                }
                "extension_ui_request"
                    if ["select", "confirm", "input", "editor"]
                        .contains(&f["method"].as_str().unwrap_or_default()) =>
                {
                    panic!("test requires interactive input ({})", f["method"])
                }
                _ => {}
            },
            Ok(Event::Closed) => panic!("OMP disconnected"),
            _ => {}
        }
    }
    assert!(
        settled && streamed && completed && tool && answer,
        "settled={settled} streaming={streamed} completed={completed} tool={tool} final={answer}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("note.txt")).unwrap(),
        "fixture value: 17\n"
    );
    println!(
        "Native stream verified: thinking deltas, block completion, read tool, final 323; fixture unchanged."
    );
}
