use super::*;
use std::time::{Duration, Instant};

/// Real RPC contract test: no tools, files or external messages requested.
#[test]
#[ignore = "sends authorized test prompts to the installed OMP model"]
fn live_steering_and_stop_are_acknowledged_and_settle() {
    let root = std::env::temp_dir().join(format!("omp-gui-control-{}", crate::domain::id()));
    std::fs::create_dir_all(&root).unwrap();
    let omp = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("omp/omp.exe");
    let engine = Engine::start(omp.to_str().unwrap(), &root, &root.join("sessions"), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let (mut stage, mut extra_sent, mut extra_ack, mut saw_extra, mut stop_sent, mut stop_ack) =
        (0, false, false, false, false, false);
    let mut done = false;
    while Instant::now() < deadline {
        let Ok(Event::Frame(f)) = engine.events.recv_timeout(Duration::from_secs(2)) else {
            continue;
        };
        match f["type"].as_str().unwrap_or_default() {
            "ready" => {
                engine.send(json!({"type":"prompt","id":"first","message":"Explain how binary search works, in about 1500 words. Do not use tools."})).unwrap();
                stage = 1;
            }
            "message_update" if stage == 1 && !extra_sent => {
                engine.send(json!({"type":"prompt","id":"extra","streamingBehavior":"steer","message":"Change of plan: do not explain binary search. Reply only APPEND_OK. Do not use tools."})).unwrap();
                extra_sent = true;
            }
            "message_end" if stage == 1 => {
                saw_extra |= message_text(&f["message"]).contains("APPEND_OK");
            }
            "response" => {
                assert_ne!(
                    f["success"], false,
                    "RPC command rejected: {}",
                    f["command"]
                );
                if f["id"] == "extra" {
                    extra_ack = true;
                }
                if f["id"] == "stop" {
                    stop_ack = true;
                }
            }
            "session_settled" if stage == 1 && extra_ack && saw_extra => {
                engine.send(json!({"type":"prompt","id":"long","message":"Write an extensive 4000 word explanation of sorting algorithms. Do not use tools."})).unwrap();
                stage = 2;
            }
            "message_update" if stage == 2 && !stop_sent => {
                engine
                    .send(json!({"type":"abort_and_restore_queue","id":"stop"}))
                    .unwrap();
                stop_sent = true;
            }
            "session_settled" if stage == 2 && stop_sent => {
                stage = 3;
            }
            _ => {}
        }
        if stage == 3 && stop_ack {
            done = true;
            break;
        }
    }
    assert!(
        done && extra_sent && extra_ack && saw_extra && stop_sent && stop_ack,
        "stage={stage} appended={extra_sent}/{extra_ack}/{saw_extra} stopped={stop_sent}/{stop_ack}"
    );
    println!("Native steering accepted and applied; stop acknowledged; session settled.");
}
