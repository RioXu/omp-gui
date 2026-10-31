//! Validate protocol frames and reconstruct chunked UTF-8 payloads.
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;

pub(super) const MAX_FRAME: usize = 1024 * 1024;
pub(super) const MAX_MESSAGE: usize = 64 * 1024 * 1024;
#[derive(Default)]
pub(crate) struct Decoder {
    chunk: Option<Chunk>,
}
struct Chunk {
    id: String,
    count: usize,
    next: usize,
    bytes: usize,
    data: Vec<u8>,
}
impl Decoder {
    pub fn decode(&mut self, line: &[u8]) -> Result<Option<Value>> {
        ensure!(line.len() <= MAX_FRAME, "OMP frame exceeds limit");
        let value: Value = serde_json::from_slice(line)?;
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
            return Ok(Some(serde_json::from_slice(&c.data)?));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn chunked_unicode_roundtrip() {
        let bytes = serde_json::to_vec(&json!({"type":"message_end","text":"你好 🌍"})).unwrap();
        let split = bytes.len() / 2;
        let mut d = Decoder::default();
        for (i, part) in [&bytes[..split], &bytes[split..]].iter().enumerate() {
            let frame = json!({"type":"rpc_chunk","chunkId":"a","count":2,"index":i,"byteLength":bytes.len(),"data":STANDARD.encode(part)});
            let v = d.decode(&serde_json::to_vec(&frame).unwrap()).unwrap();
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
        d.decode(&serde_json::to_vec(&f).unwrap()).unwrap();
        assert!(d.decode(br#"{"type":"ready"}"#).is_err());
        let mut d = Decoder::default();
        let mut f = f;
        f["index"] = json!(1);
        assert!(d.decode(&serde_json::to_vec(&f).unwrap()).is_err());
    }
}
