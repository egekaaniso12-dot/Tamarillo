#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo { name: "Tamarillo", copyright: "Copyright EgeHS Medya 2026 - 2027" }
}

#[derive(serde::Serialize)]
struct AppInfo { name: &'static str, copyright: &'static str }

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!())
        .expect("Tamarillo başlatılamadı");
}
