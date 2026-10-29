use std::{path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::ShellExt;

const COPYRIGHT: &str = "Copyright EgeHS Medya 2026 - 2027";

#[derive(Serialize)]
struct AppInfo { name: &'static str, copyright: &'static str }

#[derive(Serialize)]
struct JobResult { output: String, message: String }

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo { name: "Tamarillo", copyright: COPYRIGHT }
}

fn safe_output(input: &Path, suffix: &str, ext: &str) -> Result<PathBuf, String> {
    let stem = input.file_stem().and_then(|s| s.to_str()).ok_or("Dosya adı okunamadı")?;
    let parent = input.parent().ok_or("Kaynak klasörü bulunamadı")?;
    let mut output = parent.join(format!("{stem}_{suffix}.{ext}"));
    let mut n = 1;
    while output.exists() {
        output = parent.join(format!("{stem}_{suffix}_{n}.{ext}"));
        n += 1;
    }
    Ok(output)
}

async fn sidecar_output(app: &AppHandle, name: &str, args: Vec<String>) -> Result<String, String> {
    let command = app.shell().sidecar(name).map_err(|e| format!("{name} aracı açılamadı: {e}"))?.args(args);
    let result = command.output().await.map_err(|e| format!("{name} çalıştırılamadı: {e}"))?;
    if !result.status.success() {
        let detail = String::from_utf8_lossy(&result.stderr).trim().to_string();
        return Err(if detail.is_empty() { format!("İşlem başarısız (kod {:?})", result.status.code()) } else { detail });
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().to_string())
}

#[tauri::command]
async fn convert_media(app: AppHandle, input: String, mode: String) -> Result<JobResult, String> {
    let source = PathBuf::from(&input).canonicalize().map_err(|_| "Kaynak dosya bulunamadı veya okunamıyor".to_string())?;
    if !source.is_file() { return Err("Seçilen yol bir dosya değil".into()); }
    let (suffix, ext, args): (&str, &str, Vec<String>) = match mode.as_str() {
        "compress" => ("compressed", "mp4", vec!["-c:v".into(), "mpeg4".into(), "-q:v".into(), "4".into(), "-c:a".into(), "aac".into()]),
        "mp3" => ("audio", "mp3", vec!["-vn".into(), "-codec:a".into(), "libmp3lame".into(), "-q:a".into(), "2".into()]),
        "wav" => ("audio", "wav", vec!["-vn".into(), "-c:a".into(), "pcm_s16le".into()]),
        "mp4" => ("converted", "mp4", vec!["-c:v".into(), "mpeg4".into(), "-c:a".into(), "aac".into()]),
        _ => return Err("Desteklenmeyen dönüştürme türü".into()),
    };
    let output = safe_output(&source, suffix, ext)?;
    let mut command_args = vec!["-y".into(), "-i".into(), source.to_string_lossy().into_owned()];
    command_args.extend(args);
    command_args.push(output.to_string_lossy().into_owned());
    app.emit("job-progress", serde_json::json!({"phase":"running","message":"Medya dönüştürülüyor"})).ok();
    sidecar_output(&app, "binaries/ffmpeg", command_args).await?;
    Ok(JobResult { output: output.to_string_lossy().into_owned(), message: "Dönüştürme tamamlandı".into() })
}

#[tauri::command]
async fn download_video(app: AppHandle, url: String) -> Result<JobResult, String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) || trimmed.chars().any(char::is_control) {
        return Err("Geçerli bir http/https video bağlantısı gir".into());
    }
    let folder = app.path().download_dir().map_err(|e| format!("İndirme klasörü bulunamadı: {e}"))?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("İndirme klasörü açılamadı: {e}"))?;
    let template = folder.join("%(title).180B [%(id)s].%(ext)s");
    let args = vec!["--no-playlist".into(), "--newline".into(), "-f".into(), "bestvideo*+bestaudio/best".into(), "--merge-output-format".into(), "mp4".into(), "-o".into(), template.to_string_lossy().into_owned(), trimmed.into()];
    app.emit("job-progress", serde_json::json!({"phase":"running","message":"Video indiriliyor"})).ok();
    let output = sidecar_output(&app, "binaries/yt-dlp", args).await?;
    app.emit("job-progress", serde_json::json!({"phase":"completed","message":"İndirme tamamlandı"})).ok();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    Ok(JobResult { output: folder.to_string_lossy().into_owned(), message: if output.is_empty() { format!("İndirme tamamlandı ({stamp})") } else { output.lines().last().unwrap_or("İndirme tamamlandı").to_string() } })
}

#[tauri::command]
async fn probe_media(app: AppHandle, input: String) -> Result<String, String> {
    let source = PathBuf::from(input).canonicalize().map_err(|_| "Dosya bulunamadı".to_string())?;
    sidecar_output(&app, "binaries/ffprobe", vec!["-v".into(), "error".into(), "-show_entries".into(), "format=duration:stream=codec_name,width,height".into(), "-of".into(), "default=noprint_wrappers=1".into(), source.to_string_lossy().into_owned()]).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![app_info, convert_media, download_video, probe_media])
        .run(tauri::generate_context!())
        .expect("Tamarillo başlatılamadı");
}
