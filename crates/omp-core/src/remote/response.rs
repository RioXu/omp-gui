//! Bounded physical frames for large, explicitly requested management responses.
use super::protocol::{Response, VERSION};
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Read, Write};

pub const MAX_FRAME: usize = 1024 * 1024;
pub const MAX_RESPONSE: usize = 256 * 1024 * 1024;
const CHUNK: usize = 512 * 1024;
struct Buffer {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::ErrorKind::FileTooLarge.into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
struct Part {
    protocol: u32,
    id: String,
    index: usize,
    count: usize,
    byte_length: usize,
    data: String,
}

pub fn write(output: &mut impl Write, response: &Response) -> Result<()> {
    write_bounded(output, response, MAX_RESPONSE)
}
fn write_bounded(output: &mut impl Write, response: &Response, limit: usize) -> Result<()> {
    let mut buffer = Buffer {
        bytes: Vec::new(),
        limit,
    };
    let result = serde_json::to_writer(&mut buffer, response);
    let overflow = result
        .as_ref()
        .err()
        .is_some_and(|error| error.io_error_kind() == Some(std::io::ErrorKind::FileTooLarge));
    let bytes = buffer.bytes;
    if !overflow {
        result?;
    }
    if overflow {
        serde_json::to_writer(
            &mut *output,
            &Response {
                protocol: VERSION,
                id: response.id.clone(),
                result: Err("所请求的会话历史超过单次读取上限；会话索引仍可使用".into()),
            },
        )?;
        output.write_all(b"\n")?;
    } else if bytes.len() < MAX_FRAME {
        output.write_all(&bytes)?;
        output.write_all(b"\n")?;
    } else {
        let count = bytes.len().div_ceil(CHUNK);
        for (index, part) in bytes.chunks(CHUNK).enumerate() {
            serde_json::to_writer(
                &mut *output,
                &Part {
                    protocol: VERSION,
                    id: response.id.clone(),
                    index,
                    count,
                    byte_length: bytes.len(),
                    data: STANDARD.encode(part),
                },
            )?;
            output.write_all(b"\n")?;
        }
    }
    output.flush()?;
    Ok(())
}
fn line(input: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let count = input
        .take((MAX_FRAME + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    ensure!(count > 0, "SSH 连接已关闭");
    ensure!(
        count <= MAX_FRAME && bytes.ends_with(b"\n"),
        "远端响应帧超限或不完整"
    );
    Ok(bytes)
}
pub fn read(input: &mut impl BufRead, id: &str) -> Result<Response> {
    let first = line(input)?;
    let value: serde_json::Value =
        serde_json::from_slice(&first).context("远端输出不是有效协议响应")?;
    if value.get("index").is_none() {
        return Ok(serde_json::from_value(value)?);
    }
    let first: Part = serde_json::from_value(value)?;
    ensure!(
        first.protocol == VERSION
            && first.id == id
            && first.index == 0
            && first.byte_length <= MAX_RESPONSE
            && first.count == first.byte_length.div_ceil(CHUNK)
            && first.count > 0,
        "无效远端响应分片"
    );
    let total = first.byte_length;
    let count = first.count;
    let mut bytes = Vec::new();
    for index in 0..count {
        let part = if index == 0 {
            Part {
                protocol: first.protocol,
                id: first.id.clone(),
                index: first.index,
                count,
                byte_length: total,
                data: first.data.clone(),
            }
        } else {
            serde_json::from_slice::<Part>(&line(input)?)?
        };
        ensure!(
            part.protocol == VERSION
                && part.id == id
                && part.index == index
                && part.count == count
                && part.byte_length == total,
            "远端响应分片身份或顺序不匹配"
        );
        let decoded = STANDARD.decode(part.data)?;
        ensure!(
            decoded.len() == CHUNK.min(total - bytes.len()),
            "远端响应分片长度不匹配"
        );
        bytes.extend(decoded);
    }
    ensure!(bytes.len() == total, "远端响应未完整传输");
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunked_unicode_roundtrip_and_subsequent_response_remain_independent() {
        let response = Response {
            protocol: VERSION,
            id: "large".into(),
            result: Ok(serde_json::json!({"text":"中文🌍".repeat(300000)})),
        };
        let mut wire = Vec::new();
        write(&mut wire, &response).unwrap();
        for line in wire.split_inclusive(|byte| *byte == b'\n') {
            assert!(line.len() <= MAX_FRAME);
        }
        write(
            &mut wire,
            &Response {
                protocol: VERSION,
                id: "next".into(),
                result: Ok(serde_json::Value::Null),
            },
        )
        .unwrap();
        let mut input = std::io::Cursor::new(wire);
        assert_eq!(
            read(&mut input, "large").unwrap().result.unwrap(),
            response.result.unwrap()
        );
        assert!(read(&mut input, "next").unwrap().result.unwrap().is_null());
    }
    #[test]
    fn oversized_response_returns_an_error_without_closing_the_channel() {
        let mut wire = Vec::new();
        write_bounded(
            &mut wire,
            &Response {
                protocol: VERSION,
                id: "large".into(),
                result: Ok(serde_json::json!("x".repeat(2048))),
            },
            1024,
        )
        .unwrap();
        write(
            &mut wire,
            &Response {
                protocol: VERSION,
                id: "next".into(),
                result: Ok(serde_json::Value::Null),
            },
        )
        .unwrap();
        let mut input = std::io::Cursor::new(wire);
        assert!(read(&mut input, "large").unwrap().result.is_err());
        assert!(read(&mut input, "next").unwrap().result.is_ok());
    }
    #[test]
    fn rejects_unbounded_and_out_of_order_transfers() {
        for first in [
            Part {
                protocol: VERSION,
                id: "x".into(),
                index: 0,
                count: 1000,
                byte_length: MAX_RESPONSE + 1,
                data: String::new(),
            },
            Part {
                protocol: VERSION,
                id: "x".into(),
                index: 1,
                count: 1,
                byte_length: 1,
                data: "eA==".into(),
            },
        ] {
            let mut bytes = serde_json::to_vec(&first).unwrap();
            bytes.push(b'\n');
            assert!(read(&mut std::io::Cursor::new(bytes), "x").is_err());
        }
    }
}
