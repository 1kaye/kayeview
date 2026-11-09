use tauri::ipc::{InvokeBody, Request};

#[derive(serde::Serialize)]
pub struct OcrWordBox {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(serde::Serialize)]
pub struct OcrLineBox {
    pub words: Vec<OcrWordBox>,
}

#[cfg(windows)]
mod win_ocr {
    use super::{OcrLineBox, OcrWordBox};
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;

    pub fn recognize_rgba(
        width: u32,
        height: u32,
        mut bgra: Vec<u8>,
    ) -> Result<Vec<OcrLineBox>, String> {
        let expected = (width as usize) * (height as usize) * 4;
        if bgra.len() < expected {
            return Err("Buffer gambar OCR tidak valid".into());
        }
        bgra.truncate(expected);

        for px in bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        let ibuf = CryptographicBuffer::CreateFromByteArray(&bgra).map_err(|e| e.to_string())?;
        let bmp = SoftwareBitmap::CreateCopyFromBuffer(
            &ibuf,
            BitmapPixelFormat::Bgra8,
            width as i32,
            height as i32,
        )
        .map_err(|e| e.to_string())?;

        let engine = OcrEngine::TryCreateFromUserProfileLanguages().or_else(|_| {
            let langs = OcrEngine::AvailableRecognizerLanguages().map_err(|e| e.to_string())?;
            let first = langs.GetAt(0).map_err(|e| e.to_string())?;
            OcrEngine::TryCreateFromLanguage(&first).map_err(|e| e.to_string())
        })?;

        let result = engine
            .RecognizeAsync(&bmp)
            .map_err(|e| e.to_string())?
            .join()
            .map_err(|e| e.to_string())?;

        let lines = result.Lines().map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for line in lines {
            let words_vec = line.Words().map_err(|e| e.to_string())?;
            let mut words = Vec::new();
            for word in words_vec {
                let r = word.BoundingRect().map_err(|e| e.to_string())?;
                let text = word.Text().map_err(|e| e.to_string())?.to_string();
                if !text.is_empty() {
                    words.push(OcrWordBox {
                        text,
                        x: r.X,
                        y: r.Y,
                        w: r.Width,
                        h: r.Height,
                    });
                }
            }
            if !words.is_empty() {
                out.push(OcrLineBox { words });
            }
        }
        Ok(out)
    }
}

#[cfg(not(windows))]
mod linux_ocr {
    use super::{OcrLineBox, OcrWordBox};
    use image::{ImageFormat, RgbaImage};
    use std::io::{Cursor, Write};
    use std::process::{Command, Stdio};

    pub fn recognize_rgba(
        width: u32,
        height: u32,
        mut rgba: Vec<u8>,
    ) -> Result<Vec<OcrLineBox>, String> {
        let expected = (width as usize) * (height as usize) * 4;
        if rgba.len() < expected {
            return Err("Buffer gambar OCR tidak valid".into());
        }
        rgba.truncate(expected);

        let img = RgbaImage::from_raw(width, height, rgba)
            .ok_or("Gagal membuat buffer gambar OCR")?;
        let mut png_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
            .map_err(|e| e.to_string())?;

        let langs = Command::new("tesseract")
            .arg("--list-langs")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| {
                let avail: Vec<&str> = s
                    .lines()
                    .skip(1)
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && *l != "osd" && *l != "equ")
                    .collect();
                if avail.contains(&"ind") && avail.contains(&"eng") {
                    "ind+eng".to_string()
                } else if !avail.is_empty() {
                    avail.join("+")
                } else {
                    "eng".to_string()
                }
            })
            .unwrap_or_else(|| "eng".to_string());

        let mut child = Command::new("tesseract")
            .args(["stdin", "stdout", "-l", &langs, "tsv"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| {
                "Tesseract OCR belum terinstal (instal paket `tesseract` di Linux)".to_string()
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(&png_bytes);
        }

        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Tesseract OCR gagal: {}", err.trim()));
        }

        let tsv = String::from_utf8_lossy(&output.stdout);
        let mut out: Vec<OcrLineBox> = Vec::new();
        let mut cur_line_key: Option<(i32, i32, i32)> = None;
        let mut cur_words: Vec<OcrWordBox> = Vec::new();

        for line in tsv.lines().skip(1) {
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() < 12 {
                continue;
            }
            let level: i32 = cols[0].parse().unwrap_or(0);
            if level != 5 {
                continue;
            }
            let text = cols[11].trim();
            if text.is_empty() {
                continue;
            }
            let conf: f32 = cols[10].parse().unwrap_or(-1.0);
            if conf < 0.0 {
                continue;
            }
            let block_num: i32 = cols[2].parse().unwrap_or(0);
            let par_num: i32 = cols[3].parse().unwrap_or(0);
            let line_num: i32 = cols[4].parse().unwrap_or(0);
            let key = (block_num, par_num, line_num);

            if cur_line_key != Some(key) {
                if !cur_words.is_empty() {
                    out.push(OcrLineBox {
                        words: std::mem::take(&mut cur_words),
                    });
                }
                cur_line_key = Some(key);
            }

            let x: f32 = cols[6].parse().unwrap_or(0.0);
            let y: f32 = cols[7].parse().unwrap_or(0.0);
            let w: f32 = cols[8].parse().unwrap_or(0.0);
            let h: f32 = cols[9].parse().unwrap_or(0.0);
            if w > 0.0 && h > 0.0 {
                cur_words.push(OcrWordBox {
                    text: text.to_string(),
                    x,
                    y,
                    w,
                    h,
                });
            }
        }
        if !cur_words.is_empty() {
            out.push(OcrLineBox { words: cur_words });
        }
        Ok(out)
    }
}

#[tauri::command]
pub async fn ocr_image(request: Request<'_>) -> Result<Vec<OcrLineBox>, String> {
    let (width, height, raw_buf) = {
        let InvokeBody::Raw(data) = request.body() else {
            return Err("expected raw body".into());
        };
        let h = request.headers();
        let get_u32 = |k: &str| -> Result<u32, String> {
            h.get(k)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| format!("missing header {k}"))?
                .parse()
                .map_err(|_| format!("invalid {k}"))
        };
        let width = get_u32("width")?;
        let height = get_u32("height")?;
        (width, height, data.clone())
    };
    drop(request);

    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || win_ocr::recognize_rgba(width, height, raw_buf))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        tauri::async_runtime::spawn_blocking(move || linux_ocr::recognize_rgba(width, height, raw_buf))
            .await
            .map_err(|e| e.to_string())?
    }
}
