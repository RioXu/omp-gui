//! Binary management responses; a bounded error leaves the connection usable.
use super::{
    protocol::{Response, VERSION},
    wire,
};
use anyhow::{Context, Result};
use std::io::{Read, Write};
pub const MAX_RESPONSE: usize = wire::MAX_RESPONSE;
pub fn write(output: &mut impl Write, response: &Response) -> Result<()> {
    write_bounded(output, response, MAX_RESPONSE)
}
fn write_bounded(output: &mut impl Write, response: &Response, limit: usize) -> Result<()> {
    match wire::archive(response, limit) {
        Ok(bytes) => wire::write_archive(output, &bytes)?,
        Err(error) => wire::write(
            output,
            &Response {
                protocol: VERSION,
                id: response.id.clone(),
                result: Err(format!("远端响应无法编码：{error:#}")),
            },
            MAX_RESPONSE,
        )?,
    }
    output.flush()?;
    Ok(())
}
pub fn read(input: &mut impl Read, _id: &str) -> Result<Response> {
    wire::read(input, MAX_RESPONSE)?.context("SSH 连接已关闭")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_unicode_roundtrip_and_subsequent_response_remain_independent() {
        let response = Response {
            protocol: VERSION,
            id: "large".into(),
            result: Ok(serde_json::json!({"text":"中文🌍".repeat(300000)})),
        };
        let mut wire = Vec::new();
        write(&mut wire, &response).unwrap();
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
}
