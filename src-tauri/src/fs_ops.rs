use crate::image_ops::ImageCache;
use std::path::Path;
use tauri::ipc::{InvokeBody, Request, Response};

const IMAGE_EXTS: &[&str] = &[
    "png", "apng", "jpg", "jpeg", "jpe", "jfif", "webp", "gif", "bmp", "dib", "rle", "ico", "cur",
    "svg", "avif", "heic", "heif", "hif", "jxl", "tif", "tiff", "jxr", "wdp", "hdp", "dds", "tga",
    "raw", "dng", "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "orf", "rw2", "pef",
    "raf", "srw", "erf", "kdc", "dcr", "mef", "mrw", "mos", "3fr", "fff", "iiq", "x3f", "rwl",
];

fn is_image(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

#[tauri::command(async)]
pub fn read_file(path: String, cache: tauri::State<'_, ImageCache>) -> Result<Response, String> {
    if let Ok(mut map) = cache.lock() {
        if let Some(buf) = map.take_raw(&path) {
            return Ok(Response::new(buf));
        }
    }
    std::fs::read(&path)
        .map(Response::new)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn delete_file(path: String, cache: tauri::State<'_, ImageCache>) -> Result<(), String> {
    if let Ok(mut map) = cache.lock() {
        let _ = map.remove(&path);
    }
    std::fs::remove_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn download_url(url: String) -> Result<Response, String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL tidak valid".into());
    }

    tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = std::process::Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
        cmd.args([
            "-sSL",
            "--max-time",
            "15",
            "-A",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            "--",
            &url,
        ]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let output = cmd.output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!("Gagal mengunduh: {}", output.status));
        }
        if output.stdout.is_empty() {
            return Err("Data gambar kosong".into());
        }
        Ok(Response::new(output.stdout))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn read_clipboard_text() -> Option<String> {
    #[cfg(not(windows))]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        glib::MainContext::default().invoke(move || {
            let cb = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
            let txt = cb.wait_for_text().map(|s| s.to_string());
            let _ = tx.send(txt);
        });
        if let Some(s) = rx.recv().ok().flatten() {
            if !s.trim().is_empty() {
                return Some(s);
            }
        }
        // Fallback ke wl-paste / xclip jika sesi Wayland/X11 memisahkan selection
        std::process::Command::new("wl-paste")
            .args(["--no-newline"])
            .output()
            .ok()
            .filter(|o| o.status.success() && !o.stdout.is_empty())
            .and_then(|o| String::from_utf8(o.stdout).ok())
    }
    #[cfg(windows)]
    {
        None
    }
}

#[tauri::command]
pub async fn read_clipboard_image() -> Result<Response, String> {
    #[cfg(not(windows))]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        glib::MainContext::default().invoke(move || {
            let cb = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
            let res = cb
                .wait_for_image()
                .and_then(|pixbuf| pixbuf.save_to_bufferv("png", &[]).ok());
            let _ = tx.send(res);
        });
        if let Some(buf) = rx.recv().ok().flatten() {
            if !buf.is_empty() {
                return Ok(Response::new(buf));
            }
        }
        Err("Tidak ada gambar di clipboard".into())
    }
    #[cfg(windows)]
    {
        Err("Tidak ada gambar di clipboard".into())
    }
}

/// Body = raw file bytes, header `path` = URI-encoded destination path.
#[tauri::command]
pub async fn write_file(
    request: Request<'_>,
    cache: tauri::State<'_, ImageCache>,
) -> Result<(), String> {
    let (path, data) = {
        let InvokeBody::Raw(data) = request.body() else {
            return Err("expected raw body".into());
        };
        let raw_path = request
            .headers()
            .get("path")
            .and_then(|v| v.to_str().ok())
            .ok_or("missing path header")?;
        let decoded = urlencoding::decode(raw_path)
            .map(|s| s.into_owned())
            .map_err(|e| e.to_string())?;
        (decoded, data.clone())
    };
    drop(request);

    if let Ok(mut map) = cache.lock() {
        let _ = map.remove(&path);
    }

    tauri::async_runtime::spawn_blocking(move || {
        std::fs::write(&path, &data).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// All images in the same folder as `path`, sorted by name.
#[tauri::command(async)]
pub fn list_images(path: String) -> Result<Vec<String>, String> {
    let dir = Path::new(&path).parent().ok_or("no parent dir")?;
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && is_image(p))
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    files.sort_by_key(|s| s.to_lowercase());
    Ok(files)
}

/// File passed on the command line (Windows "Open with" / file association).
#[tauri::command]
pub fn initial_file() -> Option<String> {
    std::env::args().skip(1).find(|a| {
        let p = Path::new(a);
        p.is_file() && is_image(p)
    })
}
