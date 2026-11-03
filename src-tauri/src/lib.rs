// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
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
fn read_file(path: String) -> Result<Response, String> {
    std::fs::read(&path)
        .map(Response::new)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn delete_file(path: String) -> Result<(), String> {
    std::fs::remove_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn download_url(url: String) -> Result<Response, String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL tidak valid".into());
    }

    tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("curl.exe");
        cmd.args(["-sSL", "--max-time", "15", "--", &url]);
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

/// Body = raw file bytes, header `path` = URI-encoded destination path.
#[tauri::command]
async fn write_file(request: Request<'_>) -> Result<(), String> {
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

    tauri::async_runtime::spawn_blocking(move || {
        std::fs::write(&path, &data).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Fallback handler jika edit_animated_gif dipanggil lewat Tauri IPC.
#[tauri::command]
async fn edit_animated_gif(request: Request<'_>) -> Result<Response, String> {
    let InvokeBody::Raw(data) = request.body() else {
        return Err("expected raw body".into());
    };
    Ok(Response::new(data.clone()))
}

/// All images in the same folder as `path`, sorted by name.
#[tauri::command(async)]
fn list_images(path: String) -> Result<Vec<String>, String> {
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
fn initial_file() -> Option<String> {
    std::env::args().skip(1).find(|a| {
        let p = Path::new(a);
        p.is_file() && is_image(p)
    })
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
            // CLSCTX_INPROC_SERVER = 1
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

            // IWICImagingFactory::CreateDecoderFromFilename = vtable slot 3
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
            // GENERIC_READ = 0x80000000, WICDecodeMetadataCacheOnDemand = 0
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

            // IWICBitmapDecoder::GetFrame = vtable slot 13
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

            // IWICBitmapSource::GetSize = slot 3, CopyPixels = slot 7
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

#[tauri::command]
async fn decode_image(path: String) -> Result<Response, String> {
    #[cfg(windows)]
    {
        let buf = tauri::async_runtime::spawn_blocking(move || win_wic::decode_to_rgba(&path))
            .await
            .map_err(|e| e.to_string())??;
        Ok(Response::new(buf))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Dekoder WIC hanya didukung di Windows".into())
    }
}

#[derive(serde::Serialize)]
struct PrinterProps {
    accepted: bool,
    orientation: String,
    paper_code: i16,
    width_mm: f64,
    height_mm: f64,
    color_mode: String,
    dpi: i32,
}

#[derive(serde::Serialize)]
struct PrinterList {
    printers: Vec<String>,
    default: String,
}

#[derive(serde::Serialize)]
struct OcrWordBox {
    text: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(serde::Serialize)]
struct OcrLineBox {
    words: Vec<OcrWordBox>,
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

        // Convert RGBA to BGRA in-place (tanpa alokasi buffer baru)
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

#[cfg(windows)]
mod win_print {
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::sync::{LazyLock, Mutex};

    static SAVED_DEVMODES: LazyLock<Mutex<HashMap<String, Vec<u8>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    #[repr(C)]
    struct PRINTER_INFO_4W {
        p_printer_name: *mut u16,
        p_server_name: *mut u16,
        attributes: u32,
    }

    #[repr(C)]
    struct DOCINFOW {
        cb_size: i32,
        lpsz_doc_name: *const u16,
        lpsz_output: *const u16,
        lpsz_datatype: *const u16,
        fw_type: u32,
    }

    #[repr(C)]
    struct BITMAPINFOHEADER {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels_per_meter: i32,
        bi_y_pels_per_meter: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    #[link(name = "winspool")]
    unsafe extern "system" {
        fn EnumPrintersW(
            flags: u32,
            name: *const u16,
            level: u32,
            p_printer_enum: *mut u8,
            cb_buf: u32,
            pcb_needed: *mut u32,
            pc_returned: *mut u32,
        ) -> i32;
        fn GetDefaultPrinterW(psz_buffer: *mut u16, pcch_buffer: *mut u32) -> i32;
        fn OpenPrinterW(
            p_printer_name: *const u16,
            ph_printer: *mut isize,
            p_default: *const c_void,
        ) -> i32;
        fn DocumentPropertiesW(
            hwnd: isize,
            h_printer: isize,
            p_device_name: *const u16,
            p_dev_mode_output: *mut u8,
            p_dev_mode_input: *const u8,
            f_mode: u32,
        ) -> i32;
        fn DeviceCapabilitiesW(
            p_device: *const u16,
            p_port: *const u16,
            fw_capability: u16,
            p_output: *mut u8,
            p_dev_mode: *const u8,
        ) -> i32;
        fn ClosePrinter(h_printer: isize) -> i32;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateDCW(
            pwsz_driver: *const u16,
            pwsz_device: *const u16,
            psz_port: *const u16,
            pdm: *const u8,
        ) -> isize;
        fn DeleteDC(hdc: isize) -> i32;
        fn GetDeviceCaps(hdc: isize, index: i32) -> i32;
        fn StartDocW(hdc: isize, lpdi: *const DOCINFOW) -> i32;
        fn EndDoc(hdc: isize) -> i32;
        fn AbortDoc(hdc: isize) -> i32;
        fn StartPage(hdc: isize) -> i32;
        fn EndPage(hdc: isize) -> i32;
        fn SetStretchBltMode(hdc: isize, mode: i32) -> i32;
        fn SetBrushOrgEx(hdc: isize, x: i32, y: i32, lppt: *mut c_void) -> i32;
        fn StretchDIBits(
            hdc: isize,
            x_dest: i32,
            y_dest: i32,
            dest_width: i32,
            dest_height: i32,
            x_src: i32,
            y_src: i32,
            src_width: i32,
            src_height: i32,
            lp_bits: *const u8,
            lpbmi: *const BITMAPINFOHEADER,
            i_usage: u32,
            dw_rop: u32,
        ) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> isize;
    }

    unsafe fn wstr_to_string(ptr: *const u16) -> String {
        if ptr.is_null() {
            return String::new();
        }
        let mut len = 0;
        unsafe {
            while *ptr.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
        }
    }

    fn to_wstr(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn get_printers() -> (Vec<String>, String) {
        unsafe {
            let mut needed = 0u32;
            let mut returned = 0u32;
            // PRINTER_ENUM_LOCAL (2) | PRINTER_ENUM_CONNECTIONS (4) = 6, Level = 4
            let _ = EnumPrintersW(6, null(), 4, null_mut(), 0, &mut needed, &mut returned);
            let mut printers = Vec::new();
            if needed > 0 {
                let mut buf = vec![0u8; needed as usize];
                if EnumPrintersW(
                    6,
                    null(),
                    4,
                    buf.as_mut_ptr(),
                    needed,
                    &mut needed,
                    &mut returned,
                ) != 0
                {
                    let infos = std::slice::from_raw_parts(
                        buf.as_ptr() as *const PRINTER_INFO_4W,
                        returned as usize,
                    );
                    for info in infos {
                        let name = wstr_to_string(info.p_printer_name);
                        if !name.is_empty() {
                            printers.push(name);
                        }
                    }
                }
            }

            let mut def_len = 0u32;
            let _ = GetDefaultPrinterW(null_mut(), &mut def_len);
            let mut default_name = String::new();
            if def_len > 0 {
                let mut def_buf = vec![0u16; def_len as usize];
                if GetDefaultPrinterW(def_buf.as_mut_ptr(), &mut def_len) != 0 {
                    default_name = wstr_to_string(def_buf.as_ptr());
                }
            }
            if default_name.is_empty() && !printers.is_empty() {
                default_name = printers[0].clone();
            }
            (printers, default_name)
        }
    }

    unsafe fn match_driver_paper(
        printer_w: *const u16,
        short_mm: i16,
        long_mm: i16,
        default_code: i16,
    ) -> i16 {
        unsafe {
            // DC_PAPERS = 2 (u16 each), DC_PAPERSIZE = 3 ((i32, i32) in 0.1mm each)
            let count = DeviceCapabilitiesW(printer_w, null(), 2, null_mut(), null());
            if count > 0 {
                let mut ids = vec![0u16; count as usize];
                let mut sizes = vec![0i32; (count as usize) * 2];
                if DeviceCapabilitiesW(
                    printer_w,
                    null(),
                    2,
                    ids.as_mut_ptr() as *mut u8,
                    null(),
                ) > 0
                    && DeviceCapabilitiesW(
                        printer_w,
                        null(),
                        3,
                        sizes.as_mut_ptr() as *mut u8,
                        null(),
                    ) > 0
                {
                    if default_code != 256 && ids.contains(&(default_code as u16)) {
                        return default_code;
                    }
                    let mut best_id = default_code;
                    let mut best_diff = 25i32;
                    for i in 0..(count as usize) {
                        let pw = sizes[i * 2].min(sizes[i * 2 + 1]);
                        let ph = sizes[i * 2].max(sizes[i * 2 + 1]);
                        let diff = (pw - short_mm as i32).abs() + (ph - long_mm as i32).abs();
                        if diff <= 20 && diff < best_diff {
                            best_diff = diff;
                            best_id = ids[i] as i16;
                        }
                    }
                    return best_id;
                }
            }
            default_code
        }
    }

    unsafe fn get_or_init_devmode(
        printer: &str,
        h_printer: isize,
        printer_w: &[u16],
    ) -> Vec<u8> {
        if let Ok(map) = SAVED_DEVMODES.lock()
            && let Some(saved) = map.get(printer)
            && saved.len() >= 200
        {
            return saved.clone();
        }
        unsafe {
            let dm_size =
                DocumentPropertiesW(0, h_printer, printer_w.as_ptr(), null_mut(), null(), 0);
            if dm_size > 0 {
                let mut buf = vec![0u8; dm_size as usize];
                if DocumentPropertiesW(
                    0,
                    h_printer,
                    printer_w.as_ptr(),
                    buf.as_mut_ptr(),
                    null(),
                    2,
                ) >= 0
                    && buf.len() >= 200
                {
                    return buf;
                }
            }
        }
        Vec::new()
    }

    unsafe fn apply_ui_to_devmode(
        printer: &str,
        h_printer: isize,
        printer_w: &[u16],
        devmode_buf: &mut Vec<u8>,
        page_w_mm: f64,
        page_h_mm: f64,
        paper_code: i16,
        color_code: i16,
    ) {
        if devmode_buf.len() < 200 {
            return;
        }
        unsafe {
            // Only modify DM_ORIENTATION (0x1), DM_PAPERSIZE (0x2), and DM_COLOR (0x800).
            let mut fields = u32::from_ne_bytes(devmode_buf[72..76].try_into().unwrap())
                | 0x1
                | 0x2
                | 0x800;

            let orient: i16 = if page_w_mm > page_h_mm { 2 } else { 1 };
            devmode_buf[76..78].copy_from_slice(&orient.to_ne_bytes());

            let short_mm = (page_w_mm.min(page_h_mm) * 10.0).round() as i16;
            let long_mm = (page_w_mm.max(page_h_mm) * 10.0).round() as i16;
            let actual_paper =
                match_driver_paper(printer_w.as_ptr(), short_mm, long_mm, paper_code);
            devmode_buf[78..80].copy_from_slice(&actual_paper.to_ne_bytes());
            if actual_paper == 256 {
                fields |= 0x4 | 0x8;
                devmode_buf[80..82].copy_from_slice(&long_mm.to_ne_bytes());
                devmode_buf[82..84].copy_from_slice(&short_mm.to_ne_bytes());
            } else {
                fields &= !(0x4 | 0x8);
            }

            devmode_buf[92..94].copy_from_slice(&color_code.to_ne_bytes());
            devmode_buf[72..76].copy_from_slice(&fields.to_ne_bytes());

            // Validate with DM_IN_BUFFER (8) | DM_OUT_BUFFER (2) = 10
            let _ = DocumentPropertiesW(
                0,
                h_printer,
                printer_w.as_ptr(),
                devmode_buf.as_mut_ptr(),
                devmode_buf.as_ptr(),
                10,
            );
            if let Ok(mut map) = SAVED_DEVMODES.lock() {
                map.insert(printer.to_string(), devmode_buf.clone());
            }
        }
    }

    pub fn open_printer_props(
        printer: &str,
        page_w_mm: f64,
        page_h_mm: f64,
        paper_code: i16,
        color_code: i16,
        prompt: bool,
    ) -> Result<super::PrinterProps, String> {
        let driver_w = to_wstr("WINSPOOL");
        let printer_w = to_wstr(printer);
        unsafe {
            let mut h_printer: isize = 0;
            if OpenPrinterW(printer_w.as_ptr(), &mut h_printer, null()) == 0 || h_printer == 0 {
                return Err(format!("Gagal membuka printer '{}'", printer));
            }
            let mut devmode_buf = get_or_init_devmode(printer, h_printer, &printer_w);
            if devmode_buf.len() < 200 {
                ClosePrinter(h_printer);
                return Err("Gagal membaca DEVMODE printer".into());
            }

            if prompt {
                apply_ui_to_devmode(
                    printer,
                    h_printer,
                    &printer_w,
                    &mut devmode_buf,
                    page_w_mm,
                    page_h_mm,
                    paper_code,
                    color_code,
                );
                let mut out_buf = devmode_buf.clone();
                // DM_IN_PROMPT (4) | DM_IN_BUFFER (8) | DM_OUT_BUFFER (2) = 14
                let res = DocumentPropertiesW(
                    GetForegroundWindow(),
                    h_printer,
                    printer_w.as_ptr(),
                    out_buf.as_mut_ptr(),
                    devmode_buf.as_ptr(),
                    14,
                );
                if res != 1 {
                    ClosePrinter(h_printer);
                    return Ok(super::PrinterProps {
                        accepted: false,
                        orientation: if page_w_mm > page_h_mm {
                            "landscape".into()
                        } else {
                            "portrait".into()
                        },
                        paper_code,
                        width_mm: page_w_mm.min(page_h_mm),
                        height_mm: page_w_mm.max(page_h_mm),
                        color_mode: if color_code == 1 {
                            "grayscale".into()
                        } else {
                            "color".into()
                        },
                        dpi: 600,
                    });
                }
                devmode_buf = out_buf;
                if let Ok(mut map) = SAVED_DEVMODES.lock() {
                    map.insert(printer.to_string(), devmode_buf.clone());
                }
            }
            ClosePrinter(h_printer);

            let orient_raw = i16::from_ne_bytes(devmode_buf[76..78].try_into().unwrap());
            let out_paper_code = i16::from_ne_bytes(devmode_buf[78..80].try_into().unwrap());
            let paper_len = i16::from_ne_bytes(devmode_buf[80..82].try_into().unwrap());
            let paper_wid = i16::from_ne_bytes(devmode_buf[82..84].try_into().unwrap());
            let out_color = i16::from_ne_bytes(devmode_buf[92..94].try_into().unwrap());

            let mut dpi = 600;
            let mut w_mm = (paper_wid.min(paper_len) as f64) / 10.0;
            let mut h_mm = (paper_wid.max(paper_len) as f64) / 10.0;

            let hdc = CreateDCW(
                driver_w.as_ptr(),
                printer_w.as_ptr(),
                null(),
                devmode_buf.as_ptr(),
            );
            if hdc != 0 {
                let dx = GetDeviceCaps(hdc, 88).max(72);
                let dy = GetDeviceCaps(hdc, 90).max(72);
                dpi = dx.max(dy);
                let phys_w = GetDeviceCaps(hdc, 110); // PHYSICALWIDTH in device units
                let phys_h = GetDeviceCaps(hdc, 111); // PHYSICALHEIGHT in device units
                if phys_w > 0 && phys_h > 0 {
                    let mw = (phys_w as f64 * 25.4) / (dx as f64);
                    let mh = (phys_h as f64 * 25.4) / (dy as f64);
                    w_mm = mw.min(mh);
                    h_mm = mw.max(mh);
                }
                DeleteDC(hdc);
            }

            if w_mm <= 0.0 || h_mm <= 0.0 {
                w_mm = page_w_mm.min(page_h_mm);
                h_mm = page_w_mm.max(page_h_mm);
            }

            Ok(super::PrinterProps {
                accepted: true,
                orientation: if orient_raw == 2 {
                    "landscape".into()
                } else {
                    "portrait".into()
                },
                paper_code: out_paper_code,
                width_mm: (w_mm * 10.0).round() / 10.0,
                height_mm: (h_mm * 10.0).round() / 10.0,
                color_mode: if out_color == 1 {
                    "grayscale".into()
                } else {
                    "color".into()
                },
                dpi,
            })
        }
    }

    pub fn print_rgba(
        printer: &str,
        width: u32,
        height: u32,
        page_w_mm: f64,
        page_h_mm: f64,
        crop_x_mm: f64,
        crop_y_mm: f64,
        crop_w_mm: f64,
        crop_h_mm: f64,
        paper_code: i16,
        color_code: i16,
        mut bgra: Vec<u8>,
    ) -> Result<(), String> {
        let expected = (width as usize) * (height as usize) * 4;
        if bgra.len() < expected {
            return Err("Buffer gambar tidak valid".into());
        }
        bgra.truncate(expected);

        // Convert RGBA to BGRA in-place for Win32 32-bit DIB (zero extra allocation)
        for px in bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        let driver_w = to_wstr("WINSPOOL");
        let printer_w = to_wstr(printer);
        let doc_w = to_wstr("kayeView Print");

        unsafe {
            let mut h_printer: isize = 0;
            let mut devmode_buf: Vec<u8> = Vec::new();
            if OpenPrinterW(printer_w.as_ptr(), &mut h_printer, null()) != 0 && h_printer != 0 {
                devmode_buf = get_or_init_devmode(printer, h_printer, &printer_w);
                apply_ui_to_devmode(
                    printer,
                    h_printer,
                    &printer_w,
                    &mut devmode_buf,
                    page_w_mm,
                    page_h_mm,
                    paper_code,
                    color_code,
                );
                ClosePrinter(h_printer);
            }

            let pdm = if devmode_buf.is_empty() {
                null()
            } else {
                devmode_buf.as_ptr()
            };

            let hdc = CreateDCW(driver_w.as_ptr(), printer_w.as_ptr(), null(), pdm);
            if hdc == 0 {
                return Err(format!("Gagal membuka printer '{}'", printer));
            }

            // LOGPIXELSX = 88, LOGPIXELSY = 90, PHYSICALOFFSETX = 112, PHYSICALOFFSETY = 113
            let dpi_x = GetDeviceCaps(hdc, 88).max(72) as f64;
            let dpi_y = GetDeviceCaps(hdc, 90).max(72) as f64;
            let off_x = GetDeviceCaps(hdc, 112);
            let off_y = GetDeviceCaps(hdc, 113);

            // Map cropped content region (crop_x_mm, crop_y_mm, crop_w_mm, crop_h_mm) to physical paper coordinates
            let dest_x = ((crop_x_mm * dpi_x) / 25.4).round() as i32 - off_x;
            let dest_y = ((crop_y_mm * dpi_y) / 25.4).round() as i32 - off_y;
            let dest_w = ((crop_w_mm * dpi_x) / 25.4).round() as i32;
            let dest_h = ((crop_h_mm * dpi_y) / 25.4).round() as i32;

            let doc_info = DOCINFOW {
                cb_size: std::mem::size_of::<DOCINFOW>() as i32,
                lpsz_doc_name: doc_w.as_ptr(),
                lpsz_output: null(),
                lpsz_datatype: null(),
                fw_type: 0,
            };

            let bmi = BITMAPINFOHEADER {
                bi_size: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                bi_width: width as i32,
                bi_height: -(height as i32), // top-down DIB
                bi_planes: 1,
                bi_bit_count: 32,
                bi_compression: 0, // BI_RGB
                bi_size_image: bgra.len() as u32,
                bi_x_pels_per_meter: (dpi_x * 39.3701) as i32,
                bi_y_pels_per_meter: (dpi_y * 39.3701) as i32,
                bi_clr_used: 0,
                bi_clr_important: 0,
            };

            if StartDocW(hdc, &doc_info) <= 0 {
                DeleteDC(hdc);
                return Err("Gagal memulai dokumen cetak (StartDocW)".into());
            }

            if StartPage(hdc) <= 0 {
                AbortDoc(hdc);
                DeleteDC(hdc);
                return Err("Gagal memulai halaman cetak (StartPage)".into());
            }

            // HALFTONE = 4 for smooth driver-grade pixel interpolation
            SetStretchBltMode(hdc, 4);
            SetBrushOrgEx(hdc, 0, 0, null_mut());

            // DIB_RGB_COLORS = 0, SRCCOPY = 0x00CC0020
            let stretch_res = StretchDIBits(
                hdc,
                dest_x,
                dest_y,
                dest_w,
                dest_h,
                0,
                0,
                width as i32,
                height as i32,
                bgra.as_ptr(),
                &bmi,
                0,
                0x00CC0020,
            );

            drop(bgra);

            if stretch_res <= 0 {
                AbortDoc(hdc);
                DeleteDC(hdc);
                return Err("Gagal merender gambar ke printer (StretchDIBits)".into());
            }

            if EndPage(hdc) <= 0 {
                AbortDoc(hdc);
                DeleteDC(hdc);
                return Err("Gagal menyelesaikan halaman cetak (EndPage)".into());
            }

            EndDoc(hdc);
            DeleteDC(hdc);
            Ok(())
        }
    }
}

#[tauri::command]
async fn list_printers() -> PrinterList {
    #[cfg(windows)]
    {
        let (printers, default) =
            tauri::async_runtime::spawn_blocking(win_print::get_printers)
                .await
                .unwrap_or_default();
        PrinterList { printers, default }
    }
    #[cfg(not(windows))]
    {
        PrinterList {
            printers: vec![],
            default: String::new(),
        }
    }
}

#[tauri::command]
async fn open_printer_props(
    printer: String,
    page_w_mm: f64,
    page_h_mm: f64,
    paper_code: i16,
    color_code: i16,
    prompt: bool,
) -> Result<PrinterProps, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            win_print::open_printer_props(
                &printer,
                page_w_mm,
                page_h_mm,
                paper_code,
                color_code,
                prompt,
            )
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = (printer, page_w_mm, page_h_mm, paper_code, color_code, prompt);
        Err("Printer Properties hanya didukung di Windows".into())
    }
}

#[tauri::command]
async fn print_page(request: Request<'_>) -> Result<(), String> {
    let (
        printer,
        width,
        height,
        page_w_mm,
        page_h_mm,
        crop_x_mm,
        crop_y_mm,
        crop_w_mm,
        crop_h_mm,
        paper_code,
        color_code,
        raw_buf,
    ) = {
        let InvokeBody::Raw(data) = request.body() else {
            return Err("expected raw body".into());
        };
        let h = request.headers();
        let get_str = |k: &str| -> Result<String, String> {
            let raw = h
                .get(k)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| format!("missing header {k}"))?;
            urlencoding::decode(raw)
                .map(|s| s.into_owned())
                .map_err(|e| e.to_string())
        };
        let printer = get_str("printer")?;
        let width: u32 = get_str("width")?.parse().map_err(|_| "invalid width")?;
        let height: u32 = get_str("height")?.parse().map_err(|_| "invalid height")?;
        let page_w_mm: f64 = get_str("page_w_mm")?
            .parse()
            .map_err(|_| "invalid page_w_mm")?;
        let page_h_mm: f64 = get_str("page_h_mm")?
            .parse()
            .map_err(|_| "invalid page_h_mm")?;
        let crop_x_mm: f64 = get_str("crop_x_mm")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        let crop_y_mm: f64 = get_str("crop_y_mm")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        let crop_w_mm: f64 = get_str("crop_w_mm")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(page_w_mm);
        let crop_h_mm: f64 = get_str("crop_h_mm")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(page_h_mm);
        let paper_code: i16 = get_str("paper_code")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(9);
        let color_code: i16 = get_str("color_code")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2);
        (
            printer,
            width,
            height,
            page_w_mm,
            page_h_mm,
            crop_x_mm,
            crop_y_mm,
            crop_w_mm,
            crop_h_mm,
            paper_code,
            color_code,
            data.clone(),
        )
    };
    drop(request);

    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            win_print::print_rgba(
                &printer,
                width,
                height,
                page_w_mm,
                page_h_mm,
                crop_x_mm,
                crop_y_mm,
                crop_w_mm,
                crop_h_mm,
                paper_code,
                color_code,
                raw_buf,
            )
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = (
            printer,
            width,
            height,
            page_w_mm,
            page_h_mm,
            crop_x_mm,
            crop_y_mm,
            crop_w_mm,
            crop_h_mm,
            paper_code,
            color_code,
            raw_buf,
        );
        Err("Direct printing hanya didukung di Windows".into())
    }
}

#[tauri::command]
async fn ocr_image(request: Request<'_>) -> Result<Vec<OcrLineBox>, String> {
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
        let _ = (width, height, raw_buf);
        Err("Windows Native OCR hanya didukung di Windows".into())
    }
}

#[tauri::command]
fn show_window(window: tauri::Window) -> Result<(), String> {
    window
        .show()
        .map_err(|e| format!("Failed to show window: {}", e))?;
    window
        .set_focus()
        .map_err(|e| format!("Failed to set focus: {}", e))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            show_window,
            read_file,
            delete_file,
            write_file,
            edit_animated_gif,
            decode_image,
            list_images,
            initial_file,
            list_printers,
            open_printer_props,
            print_page,
            ocr_image,
            download_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}