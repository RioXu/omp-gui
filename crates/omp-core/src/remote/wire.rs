//! Application transport only: validated rkyv archives with a versioned length prefix.
//! Native OMP and on-disk JSON formats are deliberately independent.
use anyhow::{Context, Result, ensure};
use serde::{Serialize, de::DeserializeOwned};
#[cfg(test)]
use serde_json::Value;
mod serde_codec;
use std::io::{Read, Write};

pub const MAX_RESPONSE: usize = 256 * 1024 * 1024;
const MAGIC: [u8; 4] = *b"OMR5";
const MAX_DEPTH: usize = 128;

// A flat archive avoids recursive archive validation and unbounded recursive
// allocation on hostile input. Containers carry their exact child count.
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
enum Node {
    Null,
    Bool(bool),
    Signed(i64),
    Unsigned(u64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    Array(u32),
    Object(u32),
}

pub(super) fn archive<T: Serialize>(value: &T, limit: usize) -> Result<rkyv::util::AlignedVec> {
    let mut encoder = serde_codec::Encoder::new();
    value.serialize(&mut encoder)?;
    let nodes = encoder.nodes;
    let storage = nodes
        .iter()
        .try_fold(
            nodes
                .len()
                .checked_mul(std::mem::size_of::<ArchivedNode>())
                .and_then(|n| n.checked_add(16))
                .context("远端数据长度溢出")?,
            |size, node| {
                size.checked_add(match node {
                    Node::Text(s) => s.len(),
                    Node::Bytes(b) => b.len(),
                    _ => 0,
                })
            },
        )
        .context("远端数据长度溢出")?;
    ensure!(storage <= limit.min(MAX_RESPONSE), "远端数据超过传输上限");
    let archive =
        rkyv::to_bytes::<rkyv::rancor::Error>(&nodes).map_err(|e| anyhow::anyhow!("{e}"))?;
    ensure!(archive.len() <= limit, "远端数据超过传输上限");
    Ok(archive)
}

pub fn encode<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>> {
    let archive = archive(value, limit)?;
    let mut bytes = Vec::with_capacity(8 + archive.len());
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&(archive.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&archive);
    Ok(bytes)
}
pub(super) fn write_archive(output: &mut impl Write, archive: &[u8]) -> Result<()> {
    output.write_all(&MAGIC)?;
    output.write_all(&(archive.len() as u32).to_le_bytes())?;
    output.write_all(archive)?;
    output.flush()?;
    Ok(())
}
pub fn write<T: Serialize>(output: &mut impl Write, value: &T, limit: usize) -> Result<()> {
    write_archive(output, &archive(value, limit)?)
}

pub fn read<T: DeserializeOwned>(input: &mut impl Read, limit: usize) -> Result<Option<T>> {
    let mut header = [0; 8];
    if input.read(&mut header[..1])? == 0 {
        return Ok(None);
    }
    input
        .read_exact(&mut header[1..])
        .context("远端帧头不完整")?;
    ensure!(header[..4] == MAGIC, "远端二进制协议不兼容，请更新服务");
    let len = u32::from_le_bytes(header[4..].try_into()?) as usize;
    ensure!(len > 0 && len <= limit, "远端帧长度超限");
    let mut bytes = vec![0; len];
    input.read_exact(&mut bytes).context("远端帧不完整")?;
    let nodes = rkyv::access::<rkyv::Archived<Vec<Node>>, rkyv::rancor::Error>(&bytes)
        .map_err(|e| anyhow::anyhow!("无效远端归档：{e}"))?;
    let mut decoder = serde_codec::Decoder::new(nodes);
    let value = T::deserialize(&mut decoder)?;
    ensure!(decoder.at == nodes.len(), "远端归档含多余数据");
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    enum Shape {
        Empty,
        Newtype(String),
        Tuple(u64, bool),
        Struct { value: Option<Vec<i64>> },
    }
    #[test]
    fn typed_enums_requests_and_malformed_containers() {
        for value in [
            Shape::Empty,
            Shape::Newtype("中文".into()),
            Shape::Tuple(u64::MAX, true),
            Shape::Struct {
                value: Some(vec![i64::MIN, 7]),
            },
        ] {
            let bytes = encode(&value, 4096).unwrap();
            assert_eq!(read::<Shape>(&mut &bytes[..], 4096).unwrap(), Some(value));
        }
        let request = crate::remote::protocol::Request {
            protocol: crate::remote::protocol::VERSION,
            id: "delete".into(),
            operation: crate::remote::protocol::Operation::DeleteSession {
                omp: "omp".into(),
                session: Box::new(crate::domain::Session::new(std::path::PathBuf::from(
                    "/project",
                ))),
                empty_only: true,
            },
        };
        let bytes = encode(&request, MAX_RESPONSE).unwrap();
        let decoded: crate::remote::protocol::Request =
            read(&mut &bytes[..], MAX_RESPONSE).unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            serde_json::to_value(request).unwrap()
        );
        for nodes in [
            vec![Node::Array(u32::MAX)],
            vec![Node::Object(1), Node::Null, Node::Null],
            vec![Node::Null, Node::Null],
        ] {
            let archive = rkyv::to_bytes::<rkyv::rancor::Error>(&nodes).unwrap();
            let mut frame = MAGIC.to_vec();
            frame.extend_from_slice(&(archive.len() as u32).to_le_bytes());
            frame.extend_from_slice(&archive);
            assert!(read::<Value>(&mut &frame[..], 4096).is_err());
        }
    }
    #[test]
    fn binary_roundtrip_boundaries_and_channel_reuse() {
        let value = serde_json::json!({"中文":"\0\n🌍", "numbers":[u64::MAX, i64::MIN, 0.5], "a":[null,true,{}]});
        let mut bytes = encode(&value, 4096).unwrap();
        bytes.extend(encode(&false, 4096).unwrap());
        let mut cursor = std::io::Cursor::new(bytes);
        assert_eq!(read::<Value>(&mut cursor, 4096).unwrap(), Some(value));
        assert_eq!(read::<bool>(&mut cursor, 4096).unwrap(), Some(false));
        assert_eq!(read::<Value>(&mut cursor, 4096).unwrap(), None);
        for bytes in [
            b"OMR5\xff\xff\xff\xff".to_vec(),
            b"OMR5\x04\0\0\0xxxx".to_vec(),
            b"{}\n".to_vec(),
        ] {
            assert!(read::<Value>(&mut std::io::Cursor::new(bytes), 4096).is_err());
        }
        let bytes = encode(&"text", 4096).unwrap();
        for end in 1..bytes.len() {
            assert!(read::<Value>(&mut &bytes[..end], 4096).is_err());
        }
    }
}

#[cfg(test)]
mod performance {
    use super::*;
    #[test]
    #[ignore = "manual release-mode application transport comparison"]
    fn transport_roundtrip_benchmark() {
        use std::{hint::black_box, time::Instant};
        for (name, value, rounds) in [
            (
                "small",
                serde_json::json!({"id":"abc","type":"message_update","text":"中文 delta","sequence":42}),
                20000,
            ),
            (
                "history",
                serde_json::json!({"messages": (0..2000).map(|i| serde_json::json!({"id":i,"role":"assistant","text":"中文大历史正文".repeat(100)})).collect::<Vec<_>>()}),
                20,
            ),
        ] {
            let json = serde_json::to_vec(&value).unwrap();
            let binary = encode(&value, MAX_RESPONSE).unwrap();
            let begin = Instant::now();
            for _ in 0..rounds {
                let bytes = serde_json::to_vec(black_box(&value)).unwrap();
                black_box(serde_json::from_slice::<Value>(&bytes).unwrap());
            }
            let json_time = begin.elapsed();
            let begin = Instant::now();
            for _ in 0..rounds {
                let bytes = encode(black_box(&value), MAX_RESPONSE).unwrap();
                black_box(read::<Value>(&mut &bytes[..], MAX_RESPONSE).unwrap());
            }
            eprintln!(
                "{name}: rounds={rounds} json_bytes={} rkyv_bytes={} json={json_time:?} rkyv={:?}",
                json.len(),
                binary.len(),
                begin.elapsed()
            );
        }
    }
}
