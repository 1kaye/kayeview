use tauri::ipc::{InvokeBody, Request};

#[derive(serde::Serialize, Clone)]
pub struct PrinterProps {
    pub accepted: bool,
    pub orientation: String,
    pub paper_code: i16,
    pub width_mm: f64,
    pub height_mm: f64,
    pub color_mode: String,
    pub dpi: i32,
}

#[derive(serde::Serialize)]
pub struct PrinterList {
    pub printers: Vec<String>,
    pub default: String,
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
            let count = DeviceCapabilitiesW(printer_w, null(), 2, null_mut(), null());
            if count > 0 {
                let mut ids = vec![0u16; count as usize];
                let mut sizes = vec![0i32; (count as usize) * 2];
                if DeviceCapabilitiesW(printer_w, null(), 2, ids.as_mut_ptr() as *mut u8, null())
                    > 0
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

    unsafe fn get_or_init_devmode(printer: &str, h_printer: isize, printer_w: &[u16]) -> Vec<u8> {
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
            let mut fields =
                u32::from_ne_bytes(devmode_buf[72..76].try_into().unwrap()) | 0x1 | 0x2 | 0x800;

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
                let phys_w = GetDeviceCaps(hdc, 110);
                let phys_h = GetDeviceCaps(hdc, 111);
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

            let dpi_x = GetDeviceCaps(hdc, 88).max(72) as f64;
            let dpi_y = GetDeviceCaps(hdc, 90).max(72) as f64;
            let off_x = GetDeviceCaps(hdc, 112);
            let off_y = GetDeviceCaps(hdc, 113);

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
                bi_height: -(height as i32),
                bi_planes: 1,
                bi_bit_count: 32,
                bi_compression: 0,
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

            SetStretchBltMode(hdc, 4);
            SetBrushOrgEx(hdc, 0, 0, null_mut());

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

#[cfg(not(windows))]
mod linux_print {
    use gtk::prelude::*;
    use image::{ColorType, ImageEncoder, codecs::jpeg::JpegEncoder};
    use std::collections::HashMap;
    use std::process::Command;
    use std::sync::{LazyLock, Mutex};

    static SAVED_CUPS_OPTS: LazyLock<Mutex<HashMap<String, HashMap<String, String>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    #[derive(Clone)]
    struct PpdOption {
        key: String,
        default_val: String,
        choices: Vec<(String, String)>,
    }

    pub fn get_printers() -> (Vec<String>, String) {
        let mut printers = Vec::new();
        let mut default_name = String::new();

        if let Ok(out) = Command::new("lpstat")
            .args(["-p", "-d"])
            .env("LC_ALL", "C")
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("printer ") {
                    if let Some(name) = rest.split_whitespace().next() {
                        if !name.is_empty() && !printers.contains(&name.to_string()) {
                            printers.push(name.to_string());
                        }
                    }
                } else if let Some(def) = line.strip_prefix("system default destination:") {
                    default_name = def.trim().to_string();
                }
            }
        }

        if printers.is_empty() {
            if let Ok(out) = Command::new("lpstat").arg("-a").env("LC_ALL", "C").output() {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    if let Some(name) = line.split_whitespace().next() {
                        if !name.is_empty() && !printers.contains(&name.to_string()) {
                            printers.push(name.to_string());
                        }
                    }
                }
            }
        }

        if default_name.is_empty() && !printers.is_empty() {
            default_name = printers[0].clone();
        }
        (printers, default_name)
    }

    fn load_printer_ppd_options(printer: &str) -> HashMap<String, PpdOption> {
        let mut map: HashMap<String, PpdOption> = HashMap::new();
        let ppd_path = format!("/etc/cups/ppd/{printer}.ppd");

        let ppd_content = std::fs::read_to_string(&ppd_path).ok().or_else(|| {
            Command::new("curl")
                .args([
                    "-sS",
                    "--max-time",
                    "2",
                    &format!("http://localhost:631/printers/{printer}.ppd"),
                ])
                .output()
                .ok()
                .filter(|o| o.status.success() && !o.stdout.is_empty())
                .and_then(|o| String::from_utf8(o.stdout).ok())
        });

        if let Some(content) = ppd_content {
            let mut current_key: Option<String> = None;
            for raw_line in content.lines() {
                let line = raw_line.trim();
                if let Some(rest) = line.strip_prefix("*OpenUI *") {
                    let key = rest
                        .split(|c| c == '/' || c == ':')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if !key.is_empty() {
                        current_key = Some(key.clone());
                        map.entry(key.clone()).or_insert_with(|| PpdOption {
                            key,
                            default_val: String::new(),
                            choices: Vec::new(),
                        });
                    }
                } else if line.starts_with("*CloseUI") {
                    current_key = None;
                } else if let Some(rest) = line.strip_prefix("*Default") {
                    if let Some((k, v)) = rest.split_once(':') {
                        if let Some(opt) = map.get_mut(k.trim()) {
                            opt.default_val = v.trim().to_string();
                        }
                    }
                } else if let Some(ref key) = current_key {
                    let prefix = format!("*{key} ");
                    if let Some(rest) = line.strip_prefix(&prefix) {
                        if let Some((choice_part, _)) = rest.split_once(':') {
                            let (val, label) = choice_part
                                .split_once('/')
                                .map(|(v, l)| (v.trim().to_string(), l.trim().to_string()))
                                .unwrap_or_else(|| {
                                    let v = choice_part.trim().to_string();
                                    (v.clone(), v)
                                });
                            if let Some(opt) = map.get_mut(key) {
                                opt.choices.push((val, label));
                            }
                        }
                    }
                }
            }
        }

        if map.is_empty() {
            if let Ok(out) = Command::new("lpoptions")
                .args(["-p", printer, "-l"])
                .env("LC_ALL", "C")
                .output()
            {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    if let Some((left, right)) = line.split_once(':') {
                        let key = left.split('/').next().unwrap_or("").trim().to_string();
                        if key.is_empty() {
                            continue;
                        }
                        let mut default_val = String::new();
                        let mut choices = Vec::new();
                        for token in right.split_whitespace() {
                            let val = if let Some(stripped) = token.strip_prefix('*') {
                                default_val = stripped.to_string();
                                stripped.to_string()
                            } else {
                                token.to_string()
                            };
                            choices.push((val.clone(), val));
                        }
                        map.insert(
                            key.clone(),
                            PpdOption {
                                key,
                                default_val,
                                choices,
                            },
                        );
                    }
                }
            }
        }

        map
    }

    fn paper_dims(code: i16, fallback_w: f64, fallback_h: f64) -> (f64, f64, &'static str) {
        match code {
            8 => (297.0, 420.0, "A3"),
            9 => (210.0, 297.0, "A4"),
            11 => (148.0, 210.0, "A5"),
            70 => (105.0, 148.0, "A6"),
            1 => (215.9, 279.4, "Letter"),
            5 => (215.9, 355.6, "Legal"),
            14 => (215.0, 330.0, "8.5x13in"),
            _ => (fallback_w, fallback_h, "Custom"),
        }
    }

    fn show_single_menu_dialog(
        printer_name: &str,
        init_map: &HashMap<String, String>,
        page_w_mm: f64,
        page_h_mm: f64,
        paper_code: i16,
        color_code: i16,
    ) -> Option<(HashMap<String, String>, super::PrinterProps)> {
        let ppd_opts = load_printer_ppd_options(printer_name);
        let (tx, rx) = std::sync::mpsc::channel();
        let printer_owned = printer_name.to_string();
        let cur_owned = init_map.clone();
        let short_mm = page_w_mm.min(page_h_mm);
        let long_mm = page_w_mm.max(page_h_mm);
        let is_landscape = page_w_mm > page_h_mm;

        glib::MainContext::default().invoke(move || {
            let dialog = gtk::Dialog::with_buttons(
                Some(&format!("Printer Properties - {printer_owned}")),
                None::<&gtk::Window>,
                gtk::DialogFlags::MODAL,
                &[
                    ("Cancel", gtk::ResponseType::Cancel),
                    ("OK", gtk::ResponseType::Ok),
                ],
            );
            dialog.set_resizable(false);

            let grid = gtk::Grid::new();
            grid.set_row_spacing(10);
            grid.set_column_spacing(14);
            grid.set_margin_top(16);
            grid.set_margin_bottom(16);
            grid.set_margin_start(18);
            grid.set_margin_end(18);

            let add_row = |row: i32, title: &str, combo: &gtk::ComboBoxText| {
                let lbl = gtk::Label::new(Some(title));
                lbl.set_halign(gtk::Align::Start);
                combo.set_hexpand(true);
                combo.set_width_request(240);
                grid.attach(&lbl, 0, row, 1, 1);
                grid.attach(combo, 1, row, 1, 1);
            };

            // 1. Paper Size
            let cb_paper = gtk::ComboBoxText::new();
            let papers = [
                ("9", "A4 (210 × 297 mm)"),
                ("1", "Letter (215.9 × 279.4 mm)"),
                ("5", "Legal (215.9 × 355.6 mm)"),
                ("14", "Folio / F4 (215 × 330 mm)"),
                ("8", "A3 (297 × 420 mm)"),
                ("11", "A5 (148 × 210 mm)"),
                ("70", "A6 (105 × 148 mm)"),
            ];
            for (id, label) in papers {
                cb_paper.append(Some(id), label);
            }
            if paper_code == 256 {
                cb_paper.append(
                    Some("256"),
                    &format!("Custom ({:.1} × {:.1} mm)", short_mm, long_mm),
                );
            }
            if !cb_paper.set_active_id(Some(&paper_code.to_string())) {
                cb_paper.set_active_id(Some("9"));
            }
            add_row(0, "Paper Size:", &cb_paper);

            // 2. Orientation
            let cb_orient = gtk::ComboBoxText::new();
            cb_orient.append(Some("portrait"), "Portrait");
            cb_orient.append(Some("landscape"), "Landscape");
            cb_orient.set_active_id(Some(if is_landscape {
                "landscape"
            } else {
                "portrait"
            }));
            add_row(1, "Orientation:", &cb_orient);

            // 3. Paper Type (MediaType dari PPD)
            let cb_media = gtk::ComboBoxText::new();
            let media_opt = ["MediaType", "Media", "BRMediaType", "InputSlot"]
                .iter()
                .find_map(|k| ppd_opts.get(*k));
            let media_key = media_opt
                .map(|o| o.key.clone())
                .unwrap_or_else(|| "MediaType".to_string());

            if let Some(opt) = media_opt
                && !opt.choices.is_empty()
            {
                for (id, label) in &opt.choices {
                    cb_media.append(Some(id), label);
                }
                let saved_m = cur_owned.get(&media_key).unwrap_or(&opt.default_val);
                if !cb_media.set_active_id(Some(saved_m)) {
                    cb_media.set_active(Some(0));
                }
            } else {
                cb_media.append(Some("Plain"), "Plain Paper");
                cb_media.append(Some("Glossy"), "Photo Paper Glossy");
                cb_media.append(Some("Matte"), "Photo Paper Matte");
                cb_media.append(Some("Envelope"), "Envelope");
                let saved_m = cur_owned
                    .get("MediaType")
                    .map(|s| s.as_str())
                    .unwrap_or("Plain");
                if !cb_media.set_active_id(Some(saved_m)) {
                    cb_media.set_active_id(Some("Plain"));
                }
            }
            add_row(2, "Paper Type:", &cb_media);

            // 4. Print Quality (Quality / Resolution dari PPD)
            let cb_quality = gtk::ComboBoxText::new();
            let qual_opt = [
                "Quality",
                "PrintQuality",
                "cupsPrintQuality",
                "Resolution",
                "HWResolution",
                "OutputMode",
                "StpQuality",
            ]
            .iter()
            .find_map(|k| ppd_opts.get(*k));
            let qual_key = qual_opt
                .map(|o| o.key.clone())
                .unwrap_or_else(|| "Quality".to_string());

            if let Some(opt) = qual_opt
                && !opt.choices.is_empty()
            {
                for (id, label) in &opt.choices {
                    cb_quality.append(Some(id), label);
                }
                let saved_q = cur_owned.get(&qual_key).unwrap_or(&opt.default_val);
                if !cb_quality.set_active_id(Some(saved_q)) {
                    cb_quality.set_active(Some(0));
                }
            } else {
                cb_quality.append(Some("Draft"), "Draft (Fast)");
                cb_quality.append(Some("Normal"), "Standard (Normal)");
                cb_quality.append(Some("High"), "High (Best)");
                let saved_q = cur_owned
                    .get("Quality")
                    .map(|s| s.as_str())
                    .unwrap_or("Normal");
                if !cb_quality.set_active_id(Some(saved_q)) {
                    cb_quality.set_active_id(Some("Normal"));
                }
            }
            add_row(3, "Quality:", &cb_quality);

            // 5. Color Mode
            let cb_color = gtk::ComboBoxText::new();
            cb_color.append(Some("color"), "Color");
            cb_color.append(Some("grayscale"), "Grayscale (Monochrome)");
            cb_color.set_active_id(Some(if color_code == 1 {
                "grayscale"
            } else {
                "color"
            }));
            add_row(4, "Color Mode:", &cb_color);

            // 6. Copies
            let lbl_copies = gtk::Label::new(Some("Copies:"));
            lbl_copies.set_halign(gtk::Align::Start);
            let spin_copies = gtk::SpinButton::with_range(1.0, 999.0, 1.0);
            spin_copies.set_digits(0);
            spin_copies.set_numeric(true);
            spin_copies.set_hexpand(true);
            let init_copies = cur_owned
                .get("Copies")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(1.0)
                .clamp(1.0, 999.0);
            spin_copies.set_value(init_copies);
            grid.attach(&lbl_copies, 0, 5, 1, 1);
            grid.attach(&spin_copies, 1, 5, 1, 1);

            // 7. Borderless (jika didukung driver printer)
            let borderless_opt = ppd_opts.get("Borderless");
            let cb_borderless = gtk::ComboBoxText::new();
            let mut has_borderless = false;
            if let Some(opt) = borderless_opt
                && !opt.choices.is_empty()
            {
                for (id, label) in &opt.choices {
                    cb_borderless.append(Some(id), label);
                }
                let saved_b = cur_owned.get("Borderless").unwrap_or(&opt.default_val);
                if !cb_borderless.set_active_id(Some(saved_b)) {
                    cb_borderless.set_active_id(Some("Off"));
                }
                add_row(6, "Borderless:", &cb_borderless);
                has_borderless = true;
            }

            dialog.content_area().add(&grid);
            dialog.show_all();

            let resp = dialog.run();
            let res = if resp == gtk::ResponseType::Ok {
                let mut opts = cur_owned.clone();

                let out_code: i16 = cb_paper
                    .active_id()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(paper_code);
                let (w_mm, h_mm, cups_size) = paper_dims(out_code, short_mm, long_mm);
                opts.insert("PageSize".to_string(), cups_size.to_string());

                let out_orient = cb_orient
                    .active_id()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "portrait".to_string());

                if let Some(m_val) = cb_media.active_id() {
                    opts.insert(media_key, m_val.to_string());
                }

                let mut out_dpi = 600;
                if let Some(q_val) = cb_quality.active_id() {
                    let q_str = q_val.to_string();
                    let ql = q_str.to_ascii_lowercase();
                    if ql.contains("draft")
                        || ql.contains("fast")
                        || ql.contains("300")
                        || ql.contains("360")
                    {
                        out_dpi = 360;
                    } else if ql.contains("high")
                        || ql.contains("best")
                        || ql.contains("photo")
                        || ql.contains("1200")
                        || ql.contains("1440")
                    {
                        out_dpi = 1200;
                    }
                    opts.insert(qual_key, q_str);
                }

                let out_color = cb_color
                    .active_id()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "color".to_string());

                let copies = spin_copies.value_as_int().clamp(1, 999);
                opts.insert("Copies".to_string(), copies.to_string());

                if has_borderless {
                    if let Some(b_val) = cb_borderless.active_id() {
                        opts.insert("Borderless".to_string(), b_val.to_string());
                    }
                }

                Some((
                    opts,
                    super::PrinterProps {
                        accepted: true,
                        orientation: out_orient,
                        paper_code: out_code,
                        width_mm: (w_mm * 10.0).round() / 10.0,
                        height_mm: (h_mm * 10.0).round() / 10.0,
                        color_mode: out_color,
                        dpi: out_dpi,
                    },
                ))
            } else {
                None
            };

            dialog.close();
            unsafe {
                dialog.destroy();
            }
            let _ = tx.send(res);
        });

        rx.recv().ok().flatten()
    }

    pub fn open_printer_props(
        printer: &str,
        page_w_mm: f64,
        page_h_mm: f64,
        paper_code: i16,
        color_code: i16,
        prompt: bool,
    ) -> Result<super::PrinterProps, String> {
        let short_mm = page_w_mm.min(page_h_mm);
        let long_mm = page_w_mm.max(page_h_mm);
        let orientation = if page_w_mm > page_h_mm {
            "landscape".to_string()
        } else {
            "portrait".to_string()
        };
        let color_mode = if color_code == 1 {
            "grayscale".to_string()
        } else {
            "color".to_string()
        };

        if !prompt {
            return Ok(super::PrinterProps {
                accepted: true,
                orientation,
                paper_code,
                width_mm: short_mm,
                height_mm: long_mm,
                color_mode,
                dpi: 600,
            });
        }

        let init_map = SAVED_CUPS_OPTS
            .lock()
            .ok()
            .and_then(|m| m.get(printer).cloned())
            .unwrap_or_default();

        if let Some((selected_options, props)) = show_single_menu_dialog(
            printer, &init_map, page_w_mm, page_h_mm, paper_code, color_code,
        ) {
            if let Ok(mut saved) = SAVED_CUPS_OPTS.lock() {
                saved.insert(printer.to_string(), selected_options);
            }
            Ok(props)
        } else {
            Ok(super::PrinterProps {
                accepted: false,
                orientation,
                paper_code,
                width_mm: short_mm,
                height_mm: long_mm,
                color_mode,
                dpi: 600,
            })
        }
    }

    fn build_print_pdf(
        jpg_bytes: &[u8],
        width: u32,
        height: u32,
        page_w_mm: f64,
        page_h_mm: f64,
        crop_x_mm: f64,
        crop_y_mm: f64,
        crop_w_mm: f64,
        crop_h_mm: f64,
    ) -> Vec<u8> {
        let mm_to_pt = 72.0 / 25.4;
        let pw = page_w_mm * mm_to_pt;
        let ph = page_h_mm * mm_to_pt;
        let cw = crop_w_mm * mm_to_pt;
        let ch = crop_h_mm * mm_to_pt;
        let cx = crop_x_mm * mm_to_pt;
        let cy = (page_h_mm - crop_y_mm - crop_h_mm) * mm_to_pt;

        let content = format!("q {cw:.4} 0 0 {ch:.4} {cx:.4} {cy:.4} cm /Im0 Do Q\n");
        let mut pdf = Vec::with_capacity(jpg_bytes.len() + 1024);
        pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

        let mut offsets = [0usize; 6];

        offsets[1] = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        offsets[2] = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

        offsets[3] = pdf.len();
        pdf.extend_from_slice(
            format!(
                "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.4} {ph:.4}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>\nendobj\n"
            )
            .as_bytes(),
        );

        offsets[4] = pdf.len();
        pdf.extend_from_slice(
            format!(
                "4 0 obj\n<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
                jpg_bytes.len()
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(jpg_bytes);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");

        offsets[5] = pdf.len();
        pdf.extend_from_slice(
            format!("5 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes(),
        );
        pdf.extend_from_slice(content.as_bytes());
        pdf.extend_from_slice(b"endstream\nendobj\n");

        let xref_pos = pdf.len();
        pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
        for off in &offsets[1..=5] {
            pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n")
                .as_bytes(),
        );
        pdf
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
        rgba: Vec<u8>,
    ) -> Result<(), String> {
        let pixel_count = (width as usize) * (height as usize);
        let expected = pixel_count * 4;
        if rgba.len() < expected {
            return Err("Buffer gambar tidak valid".into());
        }

        // Konversi langsung area crop RGBA ke RGB (tanpa alokasi kanvas halaman penuh raksasa)
        let mut rgb = Vec::with_capacity(pixel_count * 3);
        for px in rgba[..expected].chunks_exact(4) {
            rgb.push(px[0]);
            rgb.push(px[1]);
            rgb.push(px[2]);
        }
        drop(rgba);

        let mut jpg_bytes = Vec::with_capacity(pixel_count / 2);
        JpegEncoder::new_with_quality(&mut jpg_bytes, 95)
            .write_image(&rgb, width, height, ColorType::Rgb8.into())
            .map_err(|e| e.to_string())?;
        drop(rgb);

        let pdf_bytes = build_print_pdf(
            &jpg_bytes, width, height, page_w_mm, page_h_mm, crop_x_mm, crop_y_mm, crop_w_mm,
            crop_h_mm,
        );
        drop(jpg_bytes);

        let tmp_path =
            std::env::temp_dir().join(format!("kayeview_print_{}.pdf", std::process::id()));
        std::fs::write(&tmp_path, &pdf_bytes).map_err(|e| e.to_string())?;

        let short_mm = page_w_mm.min(page_h_mm);
        let long_mm = page_w_mm.max(page_h_mm);
        let saved_opts = SAVED_CUPS_OPTS
            .lock()
            .ok()
            .and_then(|m| m.get(printer).cloned())
            .unwrap_or_default();

        let (_, _, default_media) = paper_dims(paper_code, short_mm, long_mm);
        let media = saved_opts.get("PageSize").cloned().unwrap_or_else(|| {
            if default_media == "Custom" {
                format!(
                    "Custom.{}x{}mm",
                    short_mm.round() as i32,
                    long_mm.round() as i32
                )
            } else {
                default_media.to_string()
            }
        });

        let copies = saved_opts
            .get("Copies")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1)
            .clamp(1, 999);

        let mut cmd = Command::new("lp");
        cmd.args([
            "-d",
            printer,
            "-t",
            "kayeView Print",
            "-n",
            &copies.to_string(),
            "-o",
            &format!("PageSize={media}"),
            "-o",
            &format!("media={media}"),
            "-o",
            "print-scaling=none",
            "-o",
            "fit-to-page=false",
            "-o",
            "nofit-to-page",
            "-o",
            "fitplot=false",
            "-o",
            "scaling=100",
            "-o",
            "ReduceEnlarge=Off",
        ]);

        if color_code == 1 {
            cmd.args([
                "-o",
                "ColorModel=Gray",
                "-o",
                "Color=Grayscale",
                "-o",
                "Ink=MONO",
            ]);
        } else {
            cmd.args([
                "-o",
                "ColorModel=RGB",
                "-o",
                "Color=Color",
                "-o",
                "Ink=COLOR",
            ]);
        }

        for (k, v) in &saved_opts {
            if !matches!(
                k.as_str(),
                "PageSize" | "ColorModel" | "Color" | "Ink" | "Copies"
            ) {
                cmd.args(["-o", &format!("{k}={v}")]);
            }
        }
        cmd.arg(&tmp_path);

        let output = cmd.output();
        let _ = std::fs::remove_file(&tmp_path);

        let output = output.map_err(|e| format!("Gagal menjalankan CUPS `lp`: {e}"))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Gagal mencetak via CUPS: {}", err.trim()));
        }
        Ok(())
    }
}

#[tauri::command]
pub async fn list_printers() -> PrinterList {
    #[cfg(windows)]
    {
        let (printers, default) = tauri::async_runtime::spawn_blocking(win_print::get_printers)
            .await
            .unwrap_or_default();
        PrinterList { printers, default }
    }
    #[cfg(not(windows))]
    {
        let (printers, default) = tauri::async_runtime::spawn_blocking(linux_print::get_printers)
            .await
            .unwrap_or_default();
        PrinterList { printers, default }
    }
}

#[tauri::command]
pub async fn open_printer_props(
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
                &printer, page_w_mm, page_h_mm, paper_code, color_code, prompt,
            )
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        tauri::async_runtime::spawn_blocking(move || {
            linux_print::open_printer_props(
                &printer, page_w_mm, page_h_mm, paper_code, color_code, prompt,
            )
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

#[tauri::command]
pub async fn print_page(request: Request<'_>) -> Result<(), String> {
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
                &printer, width, height, page_w_mm, page_h_mm, crop_x_mm, crop_y_mm, crop_w_mm,
                crop_h_mm, paper_code, color_code, raw_buf,
            )
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        tauri::async_runtime::spawn_blocking(move || {
            linux_print::print_rgba(
                &printer, width, height, page_w_mm, page_h_mm, crop_x_mm, crop_y_mm, crop_w_mm,
                crop_h_mm, paper_code, color_code, raw_buf,
            )
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
