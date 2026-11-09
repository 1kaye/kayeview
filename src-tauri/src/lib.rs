mod fs_ops;
mod image_ops;
mod ocr_ops;
mod print_ops;

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
        .manage(image_ops::ImageCache::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            show_window,
            fs_ops::read_file,
            fs_ops::delete_file,
            fs_ops::write_file,
            fs_ops::list_images,
            fs_ops::initial_file,
            fs_ops::download_url,
            fs_ops::read_clipboard_text,
            fs_ops::read_clipboard_image,
            image_ops::edit_animated_gif,
            image_ops::decode_image,
            image_ops::prefetch_image,
            print_ops::list_printers,
            print_ops::open_printer_props,
            print_ops::print_page,
            ocr_ops::ocr_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
