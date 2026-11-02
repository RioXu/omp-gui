//! Reproducible JSONL receive-path comparison. No model or private logs needed.
use serde_json::{Value, json};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
enum Parser {
    Serde,
    Sonic,
    SimdCopy,
    SimdMutable,
    SimdReuse,
    Production,
}
const PARSERS: [Parser; 6] = [
    Parser::Serde,
    Parser::Sonic,
    Parser::SimdCopy,
    Parser::SimdMutable,
    Parser::SimdReuse,
    Parser::Production,
];
impl Parser {
    fn parse(self, bytes: &mut [u8], buffers: &mut simd_json::Buffers) -> Value {
        match self {
            Self::Serde => serde_json::from_slice(bytes).unwrap(),
            Self::Sonic => sonic_rs::from_slice(bytes).unwrap(),
            Self::SimdCopy => simd_json::serde::from_slice(&mut bytes.to_vec()).unwrap(),
            Self::SimdMutable => simd_json::serde::from_slice(bytes).unwrap(),
            Self::SimdReuse => simd_json::serde::from_slice_with_buffers(bytes, buffers).unwrap(),
            Self::Production => super::framing::parse_frame(bytes, buffers).unwrap(),
        }
    }
}
fn workloads() -> Vec<(&'static str, Vec<Vec<u8>>)> {
    let line = |value: Value| {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        bytes
    };
    let delta = (0..64).map(|ix| line(json!({
        "type":"message_update", "messageId":"assistant-1",
        "assistantMessageEvent":{"type":"text_delta", "contentIndex":0, "delta":format!("中文 token {ix}\n\"code\"")},
        "message":{"role":"assistant", "content":[{"type":"text", "text":format!("short reply {ix}")}]}
    }))).collect();
    let snapshot = (1..=64).map(|ix| line(json!({
        "type":"message_update", "messageId":"assistant-1",
        "assistantMessageEvent":{"type":"text_delta", "contentIndex":0, "delta":"继续生成"},
        "message":{"role":"assistant", "content":[{"type":"thinking", "thinking":"analysis ".repeat(64)},
            {"type":"text", "text":"中英文 streaming text \n\"code\" ".repeat(ix*32)}],
            "usage":{"input":4096,"output":ix*20,"cost":{"total":0.5}}, "timestamp":1700000000000u64}
    }))).collect();
    let tools = (0..64).map(|ix| line(json!({
        "type":"tool_execution_update", "toolCallId":"tool-1",
        "partialResult":{"content":[{"type":"text", "text":"tool output line\n".repeat(128)}],
            "details":{"tasks":(0..32).map(|task| json!({"id":task,"title":format!("任务 {task}"),"completed":task<ix,"progress":50,"path":format!("src/module_{task}.rs"),"tags":["rust","windows"]})).collect::<Vec<_>>()}}
    }))).collect();
    let chunks = (0..8).map(|ix| line(json!({
        "type":"rpc_chunk", "chunkId":"payload-1", "index":ix, "count":8, "byteLength":3145728,
        "data":"YWJj".repeat(128*1024)
    }))).collect();
    vec![
        ("small-delta", delta),
        ("growing-snapshot", snapshot),
        ("structured-tool", tools),
        ("512KiB-chunk", chunks),
    ]
}

#[test]
fn stream_parsers_agree_on_representative_frames() {
    for (_, frames) in workloads() {
        for frame in frames {
            let expected: Value = serde_json::from_slice(&frame).unwrap();
            for parser in PARSERS {
                let mut input = frame.clone();
                assert_eq!(
                    parser.parse(&mut input, &mut simd_json::Buffers::default()),
                    expected,
                    "{parser:?}"
                );
            }
        }
    }
}

#[test]
#[ignore = "manual release-mode JSONL parser comparison"]
fn stream_json_performance() {
    eprintln!("debug_assertions={}", cfg!(debug_assertions));
    eprintln!(
        "All outputs: serde_json::Value; input allocation excluded equally; parse+output-drop included."
    );
    eprintln!(
        "SimdCopy includes extra copy for previous immutable decoder API; Mutable consumes received bytes; Reuse also retains internal scratch buffers."
    );
    for (name, frames) in workloads() {
        let bytes: usize = frames.iter().map(Vec::len).sum();
        let mut results: [Vec<f64>; 6] = Default::default();
        let mut scratch = (0..PARSERS.len())
            .map(|_| simd_json::Buffers::with_max_depth(128, 127))
            .collect::<Vec<_>>();
        // Rotate order each round to avoid consistently favoring the first or
        // last implementation through CPU boosting or thermal drift.
        for round in 0..9 {
            for offset in 0..PARSERS.len() {
                let index = (round + offset) % PARSERS.len();
                let parser = PARSERS[index];
                let mut elapsed = Duration::ZERO;
                let mut count = 0;
                while elapsed < Duration::from_millis(80) {
                    // Stage identical mutable input for every parser outside the
                    // timer. Production reuses its receive Vec between frames.
                    let mut batch = frames.clone();
                    let began = Instant::now();
                    for frame in &mut batch {
                        black_box(parser.parse(black_box(frame), &mut scratch[index]));
                    }
                    elapsed += began.elapsed();
                    count += frames.len();
                }
                results[index].push(elapsed.as_secs_f64() * 1e9 / count as f64);
            }
        }
        eprintln!(
            "WORKLOAD {name}: frames={}, mean_bytes={:.0}",
            frames.len(),
            bytes as f64 / frames.len() as f64
        );
        for (parser, samples) in PARSERS.iter().zip(&mut results) {
            samples.sort_by(f64::total_cmp);
            let median = samples[4];
            let mbps = (bytes as f64 / frames.len() as f64) / (median / 1e9) / 1e6;
            eprintln!(
                "  {parser:?}: median_us={:.3}, range_us={:.3}..{:.3}, MBps={mbps:.1}",
                median / 1000.,
                samples[0] / 1000.,
                samples[8] / 1000.
            );
        }
    }
}
