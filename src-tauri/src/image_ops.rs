use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::{InvokeBody, Request, Response};

const MAX_CACHE_ENTRIES: usize = 4;
const MAX_CACHE_BYTES: usize = 160 * 1024 * 1024;

const WEB_DECODABLE_EXTS: &[&str] = &[
    "png", "apng", "jpg", "jpeg", "jpe", "jfif", "webp", "gif", "bmp", "dib", "ico", "cur", "svg",
    "avif",
];

static PREFETCH_GEN: AtomicU64 = AtomicU64::new(0);

pub enum CachedData {
    Raw(Vec<u8>),
    Rgba(Vec<u8>),
}

impl CachedData {
    fn len(&self) -> usize {
        match self {
            Self::Raw(b) | Self::Rgba(b) => b.len(),
        }
    }
}

#[derive(Default)]
pub struct LruImageCache {
    entries: VecDeque<(String, CachedData)>,
    total_bytes: usize,
}

impl LruImageCache {
    pub fn contains(&self, path: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == path)
    }

    pub fn remove(&mut self, path: &str) -> Option<CachedData> {
        let idx = self.entries.iter().position(|(k, _)| k == path)?;
        let (_, data) = self.entries.remove(idx)?;
        self.total_bytes = self.total_bytes.saturating_sub(data.len());
        Some(data)
    }

    pub fn take_raw(&mut self, path: &str) -> Option<Vec<u8>> {
        let idx = self
            .entries
            .iter()
            .position(|(k, v)| k == path && matches!(v, CachedData::Raw(_)))?;
        if let Some((_, CachedData::Raw(buf))) = self.entries.remove(idx) {
            self.total_bytes = self.total_bytes.saturating_sub(buf.len());
            Some(buf)
        } else {
            None
        }
    }

    pub fn take_rgba(&mut self, path: &str) -> Option<Vec<u8>> {
        let idx = self
            .entries
            .iter()
            .position(|(k, v)| k == path && matches!(v, CachedData::Rgba(_)))?;
        if let Some((_, CachedData::Rgba(buf))) = self.entries.remove(idx) {
            self.total_bytes = self.total_bytes.saturating_sub(buf.len());
            Some(buf)
        } else {
            None
        }
    }

    pub fn retain_window(&mut self, keep: &[String]) {
        let mut i = 0;
        while i < self.entries.len() {
            if !keep.iter().any(|k| k == &self.entries[i].0) {
                if let Some((_, evicted)) = self.entries.remove(i) {
                    self.total_bytes = self.total_bytes.saturating_sub(evicted.len());
                }
            } else {
                i += 1;
            }
        }
    }

    pub fn insert(&mut self, path: String, data: CachedData) {
        let size = data.len();
        if size > MAX_CACHE_BYTES {
            return;
        }
        let _ = self.remove(&path);
        while self.entries.len() >= MAX_CACHE_ENTRIES
            || (self.total_bytes + size > MAX_CACHE_BYTES && !self.entries.is_empty())
        {
            if let Some((_, old)) = self.entries.pop_front() {
                self.total_bytes = self.total_bytes.saturating_sub(old.len());
            }
        }
        self.total_bytes += size;
        self.entries.push_back((path, data));
    }
}

pub type ImageCache = Arc<Mutex<LruImageCache>>;

/// Fallback handler jika edit_animated_gif dipanggil lewat Tauri IPC (mis. WebKitGTK Linux tanpa WebCodecs ImageDecoder).
#[tauri::command]
pub async fn edit_animated_gif(request: Request<'_>) -> Result<Response, String> {
    let (ops_str, marks_b64, data) = {
        let InvokeBody::Raw(data) = request.body() else {
            return Err("expected raw body".into());
        };
        let h = request.headers();
        let ops_raw = h.get("ops").and_then(|v| v.to_str().ok()).unwrap_or("[]");
        let ops_str = urlencoding::decode(ops_raw)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| "[]".into());
        let marks_b64 = h
            .get("marks")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        (ops_str, marks_b64, data.clone())
    };
    drop(request);

    tauri::async_runtime::spawn_blocking(move || {
        use image::{
            AnimationDecoder, Frame, RgbaImage,
            codecs::gif::{GifDecoder, GifEncoder, Repeat},
            imageops,
        };
        use std::io::Cursor;

        let ops: Vec<serde_json::Value> = serde_json::from_str(&ops_str).unwrap_or_default();
        if ops.is_empty() && marks_b64.is_empty() {
            return Ok(Response::new(data));
        }

        let marks_img: Option<RgbaImage> = if !marks_b64.is_empty() {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(&marks_b64)
                .ok()
                .and_then(|bytes| image::load_from_memory(&bytes).ok())
                .map(|img| img.to_rgba8())
        } else {
            None
        };

        let decoder = GifDecoder::new(Cursor::new(&data)).map_err(|e| e.to_string())?;
        let frames = decoder
            .into_frames()
            .collect_frames()
            .map_err(|e| e.to_string())?;
        if frames.is_empty() {
            return Ok(Response::new(data));
        }

        let mut out_frames = Vec::with_capacity(frames.len());
        for frame in frames {
            let delay = frame.delay();
            let mut buf = frame.into_buffer();
            for op in &ops {
                match op.get("type").and_then(|v| v.as_str()) {
                    Some("rotate") => {
                        let deg = op
                            .get("deg")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0)
                            .rem_euclid(360);
                        buf = match deg {
                            90 => imageops::rotate90(&buf),
                            180 => imageops::rotate180(&buf),
                            270 => imageops::rotate270(&buf),
                            _ => buf,
                        };
                    }
                    Some("flipH") => {
                        imageops::flip_horizontal_in_place(&mut buf);
                    }
                    Some("crop") => {
                        let x = op.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0).max(0.0) as u32;
                        let y = op.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0).max(0.0) as u32;
                        let w = op.get("w").and_then(|v| v.as_f64()).unwrap_or(1.0).max(1.0) as u32;
                        let h = op.get("h").and_then(|v| v.as_f64()).unwrap_or(1.0).max(1.0) as u32;
                        let cw = w.min(buf.width().saturating_sub(x)).max(1);
                        let ch = h.min(buf.height().saturating_sub(y)).max(1);
                        buf = imageops::crop_imm(&buf, x, y, cw, ch).to_image();
                    }
                    _ => {}
                }
            }
            if let Some(ref m) = marks_img {
                if m.width() == buf.width() && m.height() == buf.height() {
                    imageops::overlay(&mut buf, m, 0, 0);
                } else {
                    let resized = imageops::resize(
                        m,
                        buf.width(),
                        buf.height(),
                        imageops::FilterType::Triangle,
                    );
                    imageops::overlay(&mut buf, &resized, 0, 0);
                }
            }
            out_frames.push(Frame::from_parts(buf, 0, 0, delay));
        }

        let mut out = Vec::new();
        {
            let mut encoder = GifEncoder::new_with_speed(&mut out, 10);
            let _ = encoder.set_repeat(Repeat::Infinite);
            encoder
                .encode_frames(out_frames)
                .map_err(|e| e.to_string())?;
        }
        Ok(Response::new(out))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(windows)]
mod win_wic {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};

    #[repr(C)]
    struct GUID {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    const CLSID_WIC_IMAGING_FACTORY: GUID = GUID {
        data1: 0xcacaf262,
        data2: 0x9370,
        data3: 0x4615,
        data4: [0xa1, 0x3b, 0x9f, 0x55, 0x39, 0xda, 0x4c, 0x0a],
    };

    const IID_IWIC_IMAGING_FACTORY: GUID = GUID {
        data1: 0xec5ec8a9,
        data2: 0xc395,
        data3: 0x4314,
        data4: [0x9c, 0x77, 0x54, 0xd7, 0xa9, 0x35, 0xff, 0x70],
    };

    const GUID_WIC_PIXEL_FORMAT_32BPP_RGBA: GUID = GUID {
        data1: 0xf5c7ad2d,
        data2: 0x6a8d,
        data3: 0x43dd,
        data4: [0xa7, 0xa8, 0xa2, 0x99, 0x35, 0x26, 0x1a, 0xe9],
    };

    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(pv_reserved: *mut c_void, dw_co_init: u32) -> i32;
        fn CoCreateInstance(
            rclsid: *const GUID,
            p_unk_outer: *mut c_void,
            dw_cls_context: u32,
            riid: *const GUID,
            ppv: *mut *mut c_void,
        ) -> i32;
    }

    #[link(name = "windowscodecs")]
    unsafe extern "system" {
        fn WICConvertBitmapSource(
            dst_format: *const GUID,
            p_isrc: *mut c_void,
            pp_idst: *mut *mut c_void,
        ) -> i32;
    }

    unsafe fn com_release(ptr: *mut c_void) {
        if !ptr.is_null() {
            unsafe {
                let vtbl = *(ptr as *const *const usize);
                let release: unsafe extern "system" fn(*mut c_void) -> u32 =
                    std::mem::transmute(*vtbl.add(2));
                release(ptr);
            }
        }
    }

    pub fn decode_to_rgba(path: &str) -> Result<Vec<u8>, String> {
        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let path_w: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            let _ = CoInitializeEx(null_mut(), 2); // COINIT_APARTMENTTHREADED

            let mut factory: *mut c_void = null_mut();
            let hr = CoCreateInstance(
                &CLSID_WIC_IMAGING_FACTORY,
                null_mut(),
                1,
                &IID_IWIC_IMAGING_FACTORY,
                &mut factory,
            );
            if hr < 0 || factory.is_null() {
                return Err(format!("Gagal inisialisasi WIC Factory (0x{hr:08X})"));
            }

            let factory_vtbl = *(factory as *const *const usize);
            let create_decoder: unsafe extern "system" fn(
                *mut c_void,
                *const u16,
                *const GUID,
                u32,
                u32,
                *mut *mut c_void,
            ) -> i32 = std::mem::transmute(*factory_vtbl.add(3));

            let mut decoder: *mut c_void = null_mut();
            let hr = create_decoder(
                factory,
                path_w.as_ptr(),
                null(),
                0x80000000,
                0,
                &mut decoder,
            );
            if hr < 0 || decoder.is_null() {
                com_release(factory);
                return Err(format!("Format gambar tidak didukung WIC (0x{hr:08X})"));
            }

            let decoder_vtbl = *(decoder as *const *const usize);
            let get_frame: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32 =
                std::mem::transmute(*decoder_vtbl.add(13));

            let mut frame: *mut c_void = null_mut();
            let hr = get_frame(decoder, 0, &mut frame);
            if hr < 0 || frame.is_null() {
                com_release(decoder);
                com_release(factory);
                return Err(format!("Gagal membaca frame gambar (0x{hr:08X})"));
            }

            let mut converted: *mut c_void = null_mut();
            let hr =
                WICConvertBitmapSource(&GUID_WIC_PIXEL_FORMAT_32BPP_RGBA, frame, &mut converted);
            com_release(frame);
            com_release(decoder);
            com_release(factory);

            if hr < 0 || converted.is_null() {
                return Err(format!("Gagal konversi piksel ke RGBA (0x{hr:08X})"));
            }

            let src_vtbl = *(converted as *const *const usize);
            let get_size: unsafe extern "system" fn(*mut c_void, *mut u32, *mut u32) -> i32 =
                std::mem::transmute(*src_vtbl.add(3));
            let copy_pixels: unsafe extern "system" fn(
                *mut c_void,
                *const c_void,
                u32,
                u32,
                *mut u8,
            ) -> i32 = std::mem::transmute(*src_vtbl.add(7));

            let mut w = 0u32;
            let mut h = 0u32;
            let hr = get_size(converted, &mut w, &mut h);
            if hr < 0 || w == 0 || h == 0 {
                com_release(converted);
                return Err("Ukuran gambar tidak valid".into());
            }

            let stride = w * 4;
            let pixel_bytes = (stride as usize) * (h as usize);
            let mut out = vec![0u8; 16 + pixel_bytes];
            out[0..4].copy_from_slice(&w.to_le_bytes());
            out[4..8].copy_from_slice(&h.to_le_bytes());
            out[8..16].copy_from_slice(&file_size.to_le_bytes());

            let hr = copy_pixels(
                converted,
                null(),
                stride,
                pixel_bytes as u32,
                out[16..].as_mut_ptr(),
            );
            com_release(converted);

            if hr < 0 {
                return Err(format!("Gagal menyalin piksel gambar (0x{hr:08X})"));
            }

            Ok(out)
        }
    }
}

#[cfg(not(windows))]
mod linux_img {
    pub fn decode_to_rgba(path: &str) -> Result<Vec<u8>, String> {
        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let img = image::open(path).or_else(|e| {
            // Fallback ke ImageMagick / ffmpeg untuk format RAW, HEIC, JXL, JXR, dll.
            let out = std::process::Command::new("magick")
                .args([path, "-auto-orient", "png:-"])
                .output()
                .ok()
                .filter(|o| o.status.success() && !o.stdout.is_empty())
                .or_else(|| {
                    std::process::Command::new("ffmpeg")
                        .args([
                            "-v",
                            "error",
                            "-i",
                            path,
                            "-frames:v",
                            "1",
                            "-f",
                            "image2pipe",
                            "-vcodec",
                            "png",
                            "-",
                        ])
                        .output()
                        .ok()
                        .filter(|o| o.status.success() && !o.stdout.is_empty())
                })
                .ok_or_else(|| format!("Format gambar tidak didukung ({e})"))?;
            image::load_from_memory(&out.stdout).map_err(|e2| e2.to_string())
        })?;

        let rgba = img.into_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        if w == 0 || h == 0 {
            return Err("Ukuran gambar tidak valid".into());
        }
        let raw = rgba.into_raw();
        let mut out = Vec::with_capacity(16 + raw.len());
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(&file_size.to_le_bytes());
        out.extend_from_slice(&raw);
        Ok(out)
    }
}

fn decode_path_to_rgba(path: &str) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        win_wic::decode_to_rgba(path)
    }
    #[cfg(not(windows))]
    {
        linux_img::decode_to_rgba(path)
    }
}

fn is_web_decodable(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| WEB_DECODABLE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

#[tauri::command]
pub async fn decode_image(
    path: String,
    cache: tauri::State<'_, ImageCache>,
) -> Result<Response, String> {
    if let Ok(mut map) = cache.lock() {
        if let Some(buf) = map.take_rgba(&path) {
            return Ok(Response::new(buf));
        }
    }
    let buf = tauri::async_runtime::spawn_blocking(move || decode_path_to_rgba(&path))
        .await
        .map_err(|e| e.to_string())??;
    Ok(Response::new(buf))
}

#[tauri::command]
pub async fn prefetch_image(
    path: String,
    window: Option<Vec<String>>,
    cache: tauri::State<'_, ImageCache>,
) -> Result<(), String> {
    let cache_ref = Arc::clone(&cache);
    let seq = if let Some(ref win) = window {
        let g = PREFETCH_GEN.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut map) = cache_ref.lock() {
            map.retain_window(win);
            if map.contains(&path) {
                return Ok(());
            }
        }
        g
    } else {
        if let Ok(map) = cache_ref.lock() {
            if map.contains(&path) {
                return Ok(());
            }
        }
        PREFETCH_GEN.load(Ordering::SeqCst)
    };

    tauri::async_runtime::spawn_blocking(move || {
        // Batalkan jika pengguna sudah berpindah ke indeks lain (rapid browsing)
        if PREFETCH_GEN.load(Ordering::SeqCst) != seq {
            return;
        }

        let data = if is_web_decodable(&path) {
            std::fs::read(&path).ok().map(CachedData::Raw)
        } else {
            decode_path_to_rgba(&path).ok().map(CachedData::Rgba)
        };

        let Some(cached) = data else { return };
        if PREFETCH_GEN.load(Ordering::SeqCst) != seq {
            return;
        }
        if let Ok(mut map) = cache_ref.lock() {
            map.insert(path, cached);
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}
