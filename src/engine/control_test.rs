use super::*;
use std::time::{Duration, Instant};

#[test]
#[ignore = "tests native follow-up queue and immediate promotion with installed OMP"]
fn live_follow_up_waits_until_promoted() {
    let root = std::env::temp_dir().join(format!("omp-gui-followup-{}", crate::domain::id()));
    std::fs::create_dir_all(&root).unwrap();
    let omp = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("omp/omp.exe");
    let engine = Engine::start(omp.to_str().unwrap(), &root, &root.join("sessions"), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let (mut queued, mut observed_queue, mut promoted, mut consumed, mut reply) =
        (false, false, false, false, false);
    let follow = "Change of plan: reply only QUEUE_PROMOTED, without using tools.";
    while Instant::now() < deadline {
        let Ok(Event::Frame(f)) = engine.events.recv_timeout(Duration::from_secs(2)) else {
            continue;
        };
        match f["type"].as_str().unwrap_or_default() {
            "ready" => {
                engine
                    .send(json!({"type":"set_interrupt_mode","mode":"immediate"}))
                    .unwrap();
                engine.send(json!({"type":"prompt","id":"initial","message":"Write a detailed 5000 word tutorial on sorting algorithms. Do not use tools. End with FIRST_FINISHED."})).unwrap();
            }
            "message_update" if !queued => {
                engine.send(json!({"type":"prompt","id":"queued","streamingBehavior":"followUp","message":follow})).unwrap();
                queued = true;
            }
            "response" => {
                assert_ne!(f["success"], false, "{}", f["command"]);
                if f["id"] == "queued" {
                    engine
                        .send(json!({"type":"get_state","id":"inspect-queue"}))
                        .unwrap();
                }
                if f["id"] == "inspect-queue" {
                    assert!(
                        f["data"]["queuedMessages"]["followUp"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|v| v == follow)
                    );
                    assert!(!consumed, "follow-up was delivered before promotion");
                    observed_queue = true;
                    engine
                        .send(json!({"type":"promote_queued_message","id":"now","message":follow}))
                        .unwrap();
                }
                if f["id"] == "now" {
                    assert_eq!(f["data"]["promoted"], true);
                    promoted = true;
                }
            }
            "message_start"
                if f["message"]["role"] == "user"
                    && message_text(&f["message"]).contains("QUEUE_PROMOTED") =>
            {
                assert!(observed_queue);
                consumed = true;
            }
            "message_end" if f["message"]["role"] == "assistant" => {
                reply |= message_text(&f["message"]).contains("QUEUE_PROMOTED");
            }
            "session_settled" if reply => break,
            _ => {}
        }
    }
    assert!(observed_queue && promoted && consumed && reply);
    println!(
        "Follow-up stayed queued; explicit promotion moved it to steering and produced QUEUE_PROMOTED."
    );
}

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
