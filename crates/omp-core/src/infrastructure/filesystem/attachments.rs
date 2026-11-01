//! Snapshot files and normalize clipboard images before native OMP submission.
use crate::domain::{Attachment, hash, id};
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

pub const MAX_ATTACHMENT: usize = 16 * 1024 * 1024;
pub const MAX_TOTAL: u64 = 32 * 1024 * 1024;
pub const MAX_COUNT: usize = 16;

pub fn remove_session(root: &Path, session: &str) -> Result<()> {
    let dir = root.join(hash(session));
    if !dir.try_exists()? {
        return Ok(());
    }
    let base = fs::canonicalize(root)?;
    let resolved = fs::canonicalize(&dir)?;
    ensure!(
        resolved.parent() == Some(base.as_path()) && resolved.file_name() == dir.file_name(),
        "附件缓存目录身份已改变，未清理"
    );
    fs::remove_dir_all(dir).context("清理会话附件缓存失败")
}

pub enum Source {
    File(PathBuf),
    Image { bytes: Vec<u8>, format: String },
}

pub fn import(root: &Path, session: &str, source: Source) -> Result<Attachment> {
    let (name, original_path, bytes, format) = match source {
        Source::File(path) => {
            let path =
                fs::canonicalize(&path).with_context(|| format!("无法打开 {}", path.display()))?;
            let file = fs::File::open(&path)?;
            ensure!(file.metadata()?.is_file(), "仅支持拖入文件，不能添加目录");
            ensure!(
                file.metadata()?.len() <= MAX_ATTACHMENT as u64,
                "单个附件不能超过 16 MiB"
            );
            let mut bytes = Vec::new();
            file.take((MAX_ATTACHMENT + 1) as u64)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= MAX_ATTACHMENT, "单个附件不能超过 16 MiB");
            let format = image::guess_format(&bytes).ok();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            (name, Some(path), bytes, format)
        }
        Source::Image { bytes, format } => {
            ensure!(bytes.len() <= MAX_ATTACHMENT, "单张图片不能超过 16 MiB");
            let format =
                image::ImageFormat::from_mime_type(&format).context("不支持此剪贴板图片格式")?;
            (
                format!("粘贴图片.{}", format.extensions_str()[0]),
                None,
                bytes,
                Some(format),
            )
        }
    };
    let (bytes, mime, extension) = if let Some(format) = format {
        // Decode with limits before previewing; BMP/TIFF/ICO are normalized to
        // PNG because provider image blocks support PNG/JPEG/GIF/WebP.
        let mut reader = image::ImageReader::with_format(Cursor::new(&bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16_384);
        limits.max_image_height = Some(16_384);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let decoded = reader.decode().context("图片损坏或尺寸过大")?;
        match format {
            image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::Gif
            | image::ImageFormat::WebP => (bytes, Some(format.to_mime_type().to_owned()), None),
            _ => {
                let mut output = Cursor::new(Vec::new());
                decoded.write_to(&mut output, image::ImageFormat::Png)?;
                (output.into_inner(), Some("image/png".into()), Some("png"))
            }
        }
    } else {
        (bytes, None, None)
    };
    ensure!(bytes.len() <= MAX_ATTACHMENT, "转换后的附件超过 16 MiB");
    let identity = id();
    let dir = root.join(hash(session)).join(&identity);
    fs::create_dir_all(&dir)?;
    let mut path = dir.join(&name);
    if let Some(extension) = extension {
        path.set_extension(extension);
    }
    let name = if original_path.is_none() {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        name
    };
    fs::write(&path, &bytes)?;
    let path = fs::canonicalize(path)?;
    Ok(Attachment {
        id: identity,
        name,
        path,
        original_path,
        mime,
        bytes: bytes.len() as u64,
    })
}

/// Image payloads use the native `images` field. Ordinary files use OMP's
/// quoted file mentions, so its own readers process text/PDF/other formats.
pub fn prompt(text: &str, attachments: &[Attachment]) -> Result<(String, Vec<serde_json::Value>)> {
    ensure!(attachments.len() <= MAX_COUNT, "最多添加 16 个附件");
    ensure!(
        attachments.iter().map(|a| a.bytes).sum::<u64>() <= MAX_TOTAL,
        "附件总量不能超过 32 MiB"
    );
    let mut prompt = text.to_owned();
    let mut images = Vec::new();
    for attachment in attachments {
        let bytes = fs::read(&attachment.path)
            .with_context(|| format!("附件已丢失：{}，请重新添加", attachment.name))?;
        ensure!(bytes.len() <= MAX_ATTACHMENT, "附件超过 16 MiB");
        ensure!(
            bytes.len() as u64 == attachment.bytes,
            "附件已发生变化，请重新添加：{}",
            attachment.name
        );
        if let Some(mime) = &attachment.mime {
            images.push(
                serde_json::json!({"type":"image","mimeType":mime,"data":STANDARD.encode(bytes)}),
            );
            prompt.push_str(&format!(
                "\n\n图片附件 {}：{}",
                images.len(),
                serde_json::to_string(&attachment.name)?
            ));
        } else {
            let path = crate::infrastructure::omp::sessions::plain_path(&attachment.path)
                .to_string_lossy()
                .replace('\\', "/");
            ensure!(
                !path.contains(['"', '\n', '\r']),
                "附件路径包含不支持的字符"
            );
            prompt.push_str(&format!(
                "\n\n文件附件 {}：\n@\"{path}\"",
                serde_json::to_string(&attachment.name)?
            ));
        }
    }
    Ok((prompt, images))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("omp-gui-attachments-{}", id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            assert!(
                self.0.is_absolute()
                    && self.0.parent() == Some(std::env::temp_dir().as_path())
                    && self
                        .0
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("omp-gui-attachments-")
            );
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn snapshots_unicode_files_and_quotes_the_snapshot_for_native_reading() {
        let fixture = Fixture::new();
        let original = fixture.0.join("中文 file.txt");
        fs::write(&original, "fixture secret 1729\n").unwrap();
        let attachment = import(
            &fixture.0.join("cache"),
            "../unsafe-session",
            Source::File(original.clone()),
        )
        .unwrap();
        fs::write(&original, "changed after dropping").unwrap();
        let (message, images) = prompt("请读取", std::slice::from_ref(&attachment)).unwrap();
        assert!(message.starts_with("请读取\n\n文件附件"));
        assert!(message.contains("@\"") && message.contains("中文 file.txt\""));
        assert!(
            message.contains("\n@\""),
            "OMP file mentions require a token boundary before @"
        );
        assert!(images.is_empty());
        assert!(
            attachment
                .path
                .starts_with(fs::canonicalize(fixture.0.join("cache")).unwrap())
        );
        assert_eq!(
            fs::read_to_string(&attachment.path).unwrap(),
            "fixture secret 1729\n"
        );
        fs::remove_file(&attachment.path).unwrap();
        assert!(
            prompt("retry", &[attachment]).is_err(),
            "never send a missing attachment as only its name"
        );
        let other = import(
            &fixture.0.join("cache"),
            "other",
            Source::File(original.clone()),
        )
        .unwrap();
        remove_session(&fixture.0.join("cache"), "../unsafe-session").unwrap();
        assert!(
            !fixture
                .0
                .join("cache")
                .join(hash("../unsafe-session"))
                .exists()
        );
        assert!(
            other.path.exists() && original.exists(),
            "cleanup only removes the deleted session's cache"
        );
    }
    #[test]
    fn clipboard_bmp_becomes_a_real_png_image_block_and_roundtrips_pixels() {
        let fixture = Fixture::new();
        let pixels = image::RgbImage::from_pixel(3, 2, image::Rgb([12, 34, 56]));
        let mut bmp = Cursor::new(Vec::new());
        pixels.write_to(&mut bmp, image::ImageFormat::Bmp).unwrap();
        let attachment = import(
            &fixture.0,
            "session",
            Source::Image {
                bytes: bmp.into_inner(),
                format: "image/bmp".into(),
            },
        )
        .unwrap();
        assert_eq!(attachment.path.extension().unwrap(), "png");
        let (message, images) = prompt("", &[attachment]).unwrap();
        assert!(message.contains("图片附件 1"));
        assert_eq!(images[0]["mimeType"], "image/png");
        assert_eq!(images[0]["type"], "image");
        let bytes = STANDARD
            .decode(images[0]["data"].as_str().unwrap())
            .unwrap();
        assert_eq!(image::load_from_memory(&bytes).unwrap().to_rgb8(), pixels);
    }
    #[test]
    fn rejects_directories_invalid_images_and_oversize_files_without_cache_entries() {
        let fixture = Fixture::new();
        let cache = fixture.0.join("cache");
        assert!(import(&cache, "s", Source::File(fixture.0.clone())).is_err());
        assert!(
            import(
                &cache,
                "s",
                Source::Image {
                    bytes: b"not png".to_vec(),
                    format: "image/png".into()
                }
            )
            .is_err()
        );
        let file = fixture.0.join("large.bin");
        fs::File::create(&file)
            .unwrap()
            .set_len(MAX_ATTACHMENT as u64 + 1)
            .unwrap();
        assert!(import(&cache, "s", Source::File(file)).is_err());
        assert!(!cache.exists());
    }
}
