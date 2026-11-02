//! Opt-in integration check: uses the local OMP account and a disposable project.
use omp_gui::domain::{ProcessKind, process_blocks};
use omp_gui::infrastructure::omp::{Client, Event, message_text};
use serde_json::json;
use std::time::{Duration, Instant};

#[test]
#[ignore = "sends an authorized read-only prompt to verify native working intents"]
fn live_tool_intents_reach_the_gui_projection() {
    use omp_gui::infrastructure::omp::working_intent;
    let root = std::env::temp_dir().join(format!("omp-gui-intent-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("note.txt"), "fixture value: 17\n").unwrap();
    let omp = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("omp/omp.exe");
    let engine = Client::start(omp.to_str().unwrap(), &root, &root.join("sessions"), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(300);
    let (mut settled, mut read, mut answer) = (false, false, false);
    let mut intents = Vec::new();
    while Instant::now() < deadline {
        match engine.events.recv_timeout(Duration::from_secs(1)) {
            Ok(Event::Frame(frame)) => {
                if let Some(intent) = working_intent(&frame)
                    && intents.last() != Some(&intent)
                {
                    println!("Native working intent: {intent}");
                    intents.push(intent);
                }
                match frame["type"].as_str().unwrap_or_default() {
                    "ready" => {
                        engine.send(json!({"type":"negotiate_protocol","id":"protocol","protocolVersion":2})).unwrap();
                        engine.send(json!({"type":"prompt","id":"intent-test","message":"请用 read 工具读取 note.txt，再计算其中数字乘以 19。工具调用的 i 参数填写简短中文动作说明（如：读取测试文件）。最终回答只包含计算结果。不要修改任何文件，也不要调用其他工具。"})).unwrap();
                    }
                    "tool_execution_start" => read |= frame["toolName"] == "read",
                    "message_end" if frame["message"]["role"] == "assistant" => {
                        answer |= message_text(&frame["message"]).contains("323");
                    }
                    "session_settled" => {
                        settled = true;
                        break;
                    }
                    "response" if frame["success"] == false => {
                        panic!("native request failed: {}", frame["command"])
                    }
                    "extension_ui_request"
                        if ["select", "confirm", "input", "editor"]
                            .contains(&frame["method"].as_str().unwrap_or_default()) =>
                    {
                        panic!("test needs interactive input")
                    }
                    _ => {}
                }
            }
            Ok(Event::Closed) => panic!("OMP disconnected"),
            Ok(Event::Error(error)) => panic!("OMP stream failed: {error}"),
            Err(_) => {}
        }
    }
    assert!(
        settled && read && answer && !intents.is_empty(),
        "settled={settled}, read={read}, answer={answer}, intents={intents:?}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("note.txt")).unwrap(),
        "fixture value: 17\n"
    );
}

#[test]
#[ignore = "sends one authorized prompt to the installed OMP model"]
fn live_process_stream_preserves_thinking_tools_and_final_answer() {
    let root = std::env::temp_dir().join(format!("omp-gui-stream-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("note.txt"), "fixture value: 17\n").unwrap();
    let omp = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("omp/omp.exe");
    let engine = Client::start(omp.to_str().unwrap(), &root, &root.join("sessions"), None).unwrap();
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
