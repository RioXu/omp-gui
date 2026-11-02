//! Validate protocol frames and reconstruct chunked UTF-8 payloads.
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;

pub(super) const MAX_FRAME: usize = 1024 * 1024;
pub(super) const MAX_MESSAGE: usize = 256 * 1024 * 1024;
pub(super) const MAX_COMMAND: usize = 64 * 1024 * 1024;
// Keep the SIMD path for frequent small events and message snapshots. Large
// string-heavy frames (notably base64 chunks) favor sonic-rs in our benchmark.
pub(super) const LARGE_FRAME: usize = 256 * 1024;
const MAX_DEPTH: usize = 128;

fn parse_large_frame(line: &[u8]) -> Result<Value> {
    // sonic's Serde visitor has a deeper default limit than the old parser.
    // Bound nesting before deserializing to avoid exhausting small receiver
    // thread stacks. Skip string spans with SIMD search, including escapes,
    // so base64 payloads don't require a second byte-by-byte pass.
    let (mut cursor, mut depth) = (0, 0);
    while cursor < line.len() {
        match line[cursor] {
            b'{' | b'[' => {
                depth += 1;
                ensure!(depth < MAX_DEPTH, "OMP JSON nesting exceeds limit");
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            b'"' => {
                cursor += 1;
                while let Some(offset) = memchr::memchr2(b'"', b'\\', &line[cursor..]) {
                    cursor += offset;
                    if line[cursor] == b'"' {
                        break;
                    }
                    cursor = (cursor + 2).min(line.len());
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    Ok(sonic_rs::from_slice(line)?)
}

pub(super) fn parse_frame(line: &mut [u8], buffers: &mut simd_json::Buffers) -> Result<Value> {
    if line.len() >= LARGE_FRAME {
        parse_large_frame(line)
    } else {
        Ok(simd_json::serde::from_slice_with_buffers(line, buffers)?)
    }
}
/// Encode a logical message using the same protocol-v2 chunks as native OMP.
pub fn encode(frame: &Value) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(frame)?;
    ensure!(bytes.len() < MAX_MESSAGE, "OMP message exceeds limit");
    if bytes.len() < MAX_FRAME {
        bytes.push(b'\n');
        return Ok(bytes);
    }
    let id = crate::domain::id();
    let count = bytes.len().div_ceil(512 * 1024);
    let mut output = Vec::new();
    for (index, part) in bytes.chunks(512 * 1024).enumerate() {
        serde_json::to_writer(
            &mut output,
            &serde_json::json!({"type":"rpc_chunk","chunkId":id,"index":index,"count":count,"byteLength":bytes.len(),"data":STANDARD.encode(part)}),
        )?;
        output.push(b'\n');
    }
    Ok(output)
}
pub struct Decoder {
    chunk: Option<Chunk>,
    buffers: simd_json::Buffers,
}
impl Default for Decoder {
    fn default() -> Self {
        Self {
            chunk: None,
            // Match the previous parser's bounded nesting for ordinary frames.
            buffers: simd_json::Buffers::with_max_depth(128, MAX_DEPTH - 1),
        }
    }
}
struct Chunk {
    id: String,
    count: usize,
    next: usize,
    bytes: usize,
    data: Vec<u8>,
}
impl Decoder {
    pub fn decode(&mut self, line: &mut [u8]) -> Result<Option<Value>> {
        ensure!(line.len() <= MAX_FRAME, "OMP frame exceeds limit");
        let value = parse_frame(line, &mut self.buffers)?;
        if value["type"] != "rpc_chunk" {
            ensure!(self.chunk.is_none(), "interrupted chunk sequence");
            return Ok(Some(value));
        }
        let id = value["chunkId"]
            .as_str()
            .context("missing chunk id")?
            .to_string();
        let ix = value["index"].as_u64().context("missing chunk index")? as usize;
        let count = value["count"].as_u64().context("missing chunk count")? as usize;
        let bytes = value["byteLength"]
            .as_u64()
            .context("missing chunk length")? as usize;
        ensure!(
            count > 0 && count <= 100000 && bytes <= MAX_MESSAGE,
            "invalid chunk limits"
        );
        if ix == 0 {
            ensure!(self.chunk.is_none(), "interleaved chunks");
            self.chunk = Some(Chunk {
                id: id.clone(),
                count,
                next: 0,
                bytes,
                data: Vec::new(),
            });
        }
        let chunk = self.chunk.as_mut().context("chunk without start")?;
        ensure!(
            chunk.id == id && chunk.next == ix && chunk.count == count && chunk.bytes == bytes,
            "out of order chunk"
        );
        chunk
            .data
            .extend(STANDARD.decode(value["data"].as_str().context("missing chunk payload")?)?);
        ensure!(chunk.data.len() <= bytes, "chunk exceeds declared length");
        chunk.next += 1;
        if chunk.next == count {
            let c = self.chunk.take().unwrap();
            ensure!(c.data.len() == bytes, "chunk length mismatch");
            return Ok(Some(parse_large_frame(&c.data)?));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parser_switching_and_receive_buffer_reuse_preserve_owned_values() {
        let source = br#"{"type":"message_update","message":{"content":"\u4f60\u597d \ud83c\udf0d\n\"code\""},"max":18446744073709551615,"min":-9223372036854775808,"fraction":0.5,"ok":true,"missing":null}"#;
        let expected: Value = serde_json::from_slice(source).unwrap();
        let mut decoder = Decoder::default();
        let mut line = Vec::new();
        let mut values = Vec::new();
        for size in [
            source.len(),
            LARGE_FRAME - 1,
            LARGE_FRAME,
            LARGE_FRAME + 1,
            source.len(),
        ] {
            line.clear();
            line.extend_from_slice(source);
            line.resize(size, b' ');
            values.push(decoder.decode(&mut line).unwrap().unwrap());
        }
        // Neither parsed strings nor object keys may borrow the reused input
        // or the SIMD scratch storage after they cross the event channel.
        line.fill(b'x');
        assert!(values.iter().all(|value| value == &expected));
    }

    #[test]
    fn both_parser_paths_reject_malformed_frames_and_bounded_limits_hold() {
        let invalid: &[&[u8]] = &[
            b"",
            b"{",
            b"{} {}",
            b"{\"type\":\"ready\"} trailing",
            b"{\"text\":\"\xff\"}",
            br#"{"text":"\ud800"}"#,
            b"{\"text\":\"raw\nnewline\"}",
            b"{\"number\":NaN}",
        ];
        for source in invalid {
            for large in [false, true] {
                let mut line = source.to_vec();
                if large {
                    line.resize(LARGE_FRAME, b' ');
                }
                assert!(
                    Decoder::default().decode(&mut line).is_err(),
                    "accepted {source:?}, large={large}"
                );
            }
        }
        let nested = format!("{}0{}", "[".repeat(300), "]".repeat(300));
        for size in [nested.len(), LARGE_FRAME] {
            let mut line = nested.as_bytes().to_vec();
            line.resize(size, b' ');
            assert!(Decoder::default().decode(&mut line).is_err());
        }
        let mut oversized = vec![b' '; MAX_FRAME + 1];
        assert!(Decoder::default().decode(&mut oversized).is_err());
    }

    #[test]
    fn nesting_limit_matches_legacy_parser_and_ignores_escaped_string_contents() {
        for depth in [126, 127, 128, 129] {
            let source = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
            let expected = serde_json::from_str::<Value>(&source).is_ok();
            for size in [source.len(), LARGE_FRAME] {
                let mut line = source.as_bytes().to_vec();
                line.resize(size, b' ');
                assert_eq!(
                    Decoder::default().decode(&mut line).is_ok(),
                    expected,
                    "depth={depth}, size={size}"
                );
            }
        }
        let expected = json!({"type":"message_update", "text":"{[\\\"".repeat(LARGE_FRAME / 4)});
        let mut line = serde_json::to_vec(&expected).unwrap();
        assert!(line.len() >= LARGE_FRAME && line.len() <= MAX_FRAME);
        assert_eq!(
            Decoder::default().decode(&mut line).unwrap().unwrap(),
            expected
        );
    }

    #[test]
    fn large_integer_tool_fields_keep_the_previous_float_fallback() {
        let source = br#"{"type":"tool_execution_update","details":{"positive":18446744073709551616,"negative":-9223372036854775809}}"#;
        let expected: Value = serde_json::from_slice(source).unwrap();
        for size in [source.len(), LARGE_FRAME] {
            let mut line = source.to_vec();
            line.resize(size, b' ');
            assert_eq!(
                Decoder::default().decode(&mut line).unwrap().unwrap(),
                expected
            );
        }
    }

    #[test]
    fn reassembled_json_is_validated_and_declared_size_is_bounded() {
        for payload in [b"{} trailing".as_slice(), b"{\"text\":\"\xff\"}"] {
            let mut line = serde_json::to_vec(&json!({
                "type":"rpc_chunk", "chunkId":"bad", "index":0, "count":1,
                "byteLength":payload.len(), "data":STANDARD.encode(payload),
            }))
            .unwrap();
            assert!(Decoder::default().decode(&mut line).is_err());
        }
        let mut line = serde_json::to_vec(&json!({
            "type":"rpc_chunk", "chunkId":"oversized", "index":0, "count":1,
            "byteLength":MAX_MESSAGE + 1, "data":"",
        }))
        .unwrap();
        assert!(Decoder::default().decode(&mut line).is_err());
    }

    #[test]
    fn supervisor_encodes_large_unicode_messages_with_bounded_physical_frames() {
        let value = json!({"type":"message_update","text":"中文🌍".repeat(250000)});
        let encoded = encode(&value).unwrap();
        let mut decoder = Decoder::default();
        let mut restored = None;
        for line in encoded.split_inclusive(|byte| *byte == b'\n') {
            assert!(line.len() <= MAX_FRAME);
            if let Some(value) = decoder.decode(&mut line.to_vec()).unwrap() {
                restored = Some(value);
            }
        }
        assert_eq!(restored.unwrap(), value);
    }
    #[test]
    fn chunked_unicode_roundtrip() {
        let bytes = serde_json::to_vec(&json!({"type":"message_end","text":"你好 🌍"})).unwrap();
        let split = bytes.len() / 2;
        let mut d = Decoder::default();
        for (i, part) in [&bytes[..split], &bytes[split..]].iter().enumerate() {
            let frame = json!({"type":"rpc_chunk","chunkId":"a","count":2,"index":i,"byteLength":bytes.len(),"data":STANDARD.encode(part)});
            let v = d.decode(&mut serde_json::to_vec(&frame).unwrap()).unwrap();
            if i == 0 {
                assert!(v.is_none());
            } else {
                assert_eq!(v.unwrap()["text"], "你好 🌍");
            }
        }
    }
    #[test]
    fn rejects_interrupted_and_out_of_order_chunks() {
        let mut d = Decoder::default();
        let f = json!({"type":"rpc_chunk","chunkId":"a","count":2,"index":0,"byteLength":4,"data":STANDARD.encode(b"{}")});
        d.decode(&mut serde_json::to_vec(&f).unwrap()).unwrap();
        assert!(d.decode(&mut br#"{"type":"ready"}"#.to_vec()).is_err());
        let mut d = Decoder::default();
        let mut f = f;
        f["index"] = json!(1);
        assert!(d.decode(&mut serde_json::to_vec(&f).unwrap()).is_err());
    }
}
