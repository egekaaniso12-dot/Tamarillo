use std::{path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;
use std::io::Read;

const COPYRIGHT: &str = "Copyright EgeHS Medya 2026 - 2027";

#[derive(Serialize)]
struct AppInfo { name: &'static str, copyright: &'static str }
#[derive(Serialize)]
struct JobResult { output: String, message: String, #[serde(skip_serializing_if = "Option::is_none")] content: Option<String> }

#[tauri::command]
fn app_info() -> AppInfo { AppInfo { name: "Tamarillo", copyright: COPYRIGHT } }

fn safe_output(input: &Path, suffix: &str, ext: &str) -> Result<PathBuf, String> {
    let stem = input.file_stem().and_then(|s| s.to_str()).ok_or("Dosya adı okunamadı")?;
    let parent = input.parent().ok_or("Kaynak klasörü bulunamadı")?;
    let mut output = parent.join(format!("{stem}_{suffix}.{ext}"));
    let mut n = 1;
    while output.exists() { output = parent.join(format!("{stem}_{suffix}_{n}.{ext}")); n += 1; }
    Ok(output)
}

async fn sidecar_capture(app: &AppHandle, name: &str, args: Vec<String>) -> Result<(String, String), String> {
    let result = app.shell().sidecar(name).map_err(|e| format!("{name} aracı açılamadı: {e}"))?.args(args)
        .output().await.map_err(|e| format!("{name} çalıştırılamadı: {e}"))?;
    if !result.status.success() {
        let detail = String::from_utf8_lossy(&result.stderr).trim().to_string();
        return Err(if detail.is_empty() { format!("İşlem başarısız (kod {:?})", result.status.code()) } else { detail });
    }
    Ok((String::from_utf8_lossy(&result.stdout).trim().to_string(), String::from_utf8_lossy(&result.stderr).to_string()))
}

fn valid_web_url(value: &str) -> bool {
    (value.starts_with("https://") || value.starts_with("http://")) && !value.chars().any(char::is_control)
}

#[tauri::command]
async fn convert_media(app: AppHandle, input: String, mode: String, quality: Option<u8>) -> Result<JobResult, String> {
    let source = PathBuf::from(&input).canonicalize().map_err(|_| "Kaynak dosya bulunamadı veya okunamıyor".to_string())?;
    if !source.is_file() { return Err("Seçilen yol bir dosya değil".into()); }
    let q = quality.unwrap_or(23).clamp(10, 40).to_string();
    let (suffix, ext, args): (&str, &str, Vec<String>) = match mode.as_str() {
        "compress" => ("compressed", "mp4", vec!["-c:v".into(), "libx264".into(), "-crf".into(), q, "-preset".into(), "medium".into(), "-c:a".into(), "aac".into(), "-b:a".into(), "160k".into()]),
        "mp3" => ("audio", "mp3", vec!["-vn".into(), "-codec:a".into(), "libmp3lame".into(), "-q:a".into(), "2".into()]),
        "wav" => ("audio", "wav", vec!["-vn".into(), "-c:a".into(), "pcm_s16le".into()]),
        "mp4" => ("converted", "mp4", vec!["-c:v".into(), "libx264".into(), "-c:a".into(), "aac".into(), "-movflags".into(), "+faststart".into()]),
        "mov" => ("converted", "mov", vec!["-c:v".into(), "libx264".into(), "-c:a".into(), "aac".into()]),
        "webm" => ("converted", "webm", vec!["-c:v".into(), "libvpx-vp9".into(), "-c:a".into(), "libopus".into()]),
        "mkv" => ("converted", "mkv", vec!["-c:v".into(), "libx264".into(), "-c:a".into(), "aac".into()]),
        "silence-audio" => ("no-silence", "mp3", vec!["-vn".into(), "-af".into(), "silenceremove=start_periods=1:start_duration=0.15:start_threshold=-38dB:stop_periods=-1:stop_duration=0.5:stop_threshold=-38dB".into(), "-codec:a".into(), "libmp3lame".into(), "-q:a".into(), "2".into()]),
        _ => return Err("Desteklenmeyen dönüştürme türü".into()),
    };
    let output = safe_output(&source, suffix, ext)?;
    let mut command_args = vec!["-hide_banner".into(), "-y".into(), "-i".into(), source.to_string_lossy().into_owned()];
    command_args.extend(args); command_args.push(output.to_string_lossy().into_owned());
    app.emit("job-progress", serde_json::json!({"phase":"running","message":"Medya dönüştürülüyor"})).ok();
    sidecar_capture(&app, "binaries/ffmpeg", command_args).await?;
    Ok(JobResult { output: output.to_string_lossy().into_owned(), message: "İşlem tamamlandı".into(), content: None })
}

fn parse_time(line: &str, marker: &str) -> Option<f64> {
    let rest = line.split_once(marker)?.1.trim();
    let value = rest.split_whitespace().next()?.trim_end_matches(',');
    value.parse().ok()
}

#[tauri::command]
async fn trim_video_silence(app: AppHandle, input: String, threshold: Option<i16>, min_silence: Option<f32>) -> Result<JobResult, String> {
    let source = PathBuf::from(&input).canonicalize().map_err(|_| "Kaynak dosya bulunamadı".to_string())?;
    let out = safe_output(&source, "silence-trimmed", "mp4")?;
    let limit = threshold.unwrap_or(-38).clamp(-65, -15).to_string();
    let duration = min_silence.unwrap_or(0.45).clamp(0.15, 3.0).to_string();
    let (_, diagnostic) = sidecar_capture(&app, "binaries/ffmpeg", vec!["-hide_banner".into(), "-i".into(), source.to_string_lossy().into_owned(), "-af".into(), format!("silencedetect=noise={limit}dB:d={duration}"), "-f".into(), "null".into(), "-".into()]).await?;
    let probe = sidecar_capture(&app, "binaries/ffprobe", vec!["-v".into(), "error".into(), "-show_entries".into(), "format=duration".into(), "-of".into(), "default=noprint_wrappers=1:nokey=1".into(), source.to_string_lossy().into_owned()]).await?;
    let total: f64 = probe.0.trim().parse().map_err(|_| "Videonun süresi okunamadı".to_string())?;
    let mut silences: Vec<(f64, f64)> = Vec::new(); let mut pending: Option<f64> = None;
    for line in diagnostic.lines() {
        if let Some(t) = parse_time(line, "silence_start:") { pending = Some(t); }
        if let Some(t) = parse_time(line, "silence_end:") { if let Some(start) = pending.take() { silences.push((start.max(0.0), t.min(total))); } }
    }
    if let Some(start) = pending { silences.push((start.max(0.0), total)); }
    if silences.is_empty() { return Err("Bu videoda eşik üzerindeki sessiz bölüm bulunamadı".into()); }
    let mut ranges: Vec<(f64, f64)> = Vec::new(); let mut cursor = 0.0;
    for (start, end) in silences { if start - cursor > 0.08 { ranges.push((cursor, start)); } cursor = end.max(cursor); }
    if total - cursor > 0.08 { ranges.push((cursor, total)); }
    if ranges.is_empty() { return Err("Sessizlik ayarları videodaki tüm bölümleri kapsıyor".into()); }
    let mut filters = Vec::new();
    for (i, (start, end)) in ranges.iter().enumerate() {
        filters.push(format!("[0:v]trim=start={start:.3}:end={end:.3},setpts=PTS-STARTPTS[v{i}]"));
        filters.push(format!("[0:a]atrim=start={start:.3}:end={end:.3},asetpts=PTS-STARTPTS[a{i}]"));
    }
    let concat = ranges.iter().enumerate().map(|(i,_)| format!("[v{i}][a{i}]")).collect::<String>();
    filters.push(format!("{concat}concat=n={}:v=1:a=1[v][a]", ranges.len()));
    sidecar_capture(&app, "binaries/ffmpeg", vec!["-hide_banner".into(), "-y".into(), "-i".into(), source.to_string_lossy().into_owned(), "-filter_complex".into(), filters.join(";").replace(" ", ""), "-map".into(), "[v]".into(), "-map".into(), "[a]".into(), "-c:v".into(), "libx264".into(), "-crf".into(), "23".into(), "-c:a".into(), "aac".into(), out.to_string_lossy().into_owned()]).await?;
    Ok(JobResult { output: out.to_string_lossy().into_owned(), message: "Sessiz bölümler kesildi".into(), content: None })
}

#[tauri::command]
async fn process_images(app: AppHandle, inputs: Vec<String>, action: String, width: Option<u32>, height: Option<u32>, format: Option<String>, quality: Option<u8>, watermark: Option<String>) -> Result<Vec<JobResult>, String> {
    if inputs.is_empty() { return Err("Önce görsel seç".into()); }
    let mut results = Vec::new();
    for item in inputs {
        let source = PathBuf::from(&item).canonicalize().map_err(|_| format!("Görsel bulunamadı: {item}"))?;
        let selected_format = match format.as_deref().unwrap_or("png").to_ascii_lowercase().as_str() { "png" => "png", "jpg" | "jpeg" => "jpg", "webp" => "webp", _ => return Err("Desteklenmeyen görsel formatı".into()) };
        let (suffix, ext) = match action.as_str() { "resize" => ("resized", source.extension().and_then(|v|v.to_str()).unwrap_or("png")), "compress" => ("compressed", "jpg"), "convert" => ("converted", selected_format), "watermark" => ("watermarked", source.extension().and_then(|v|v.to_str()).unwrap_or("png")), _ => return Err("Görsel işlemi tanınmadı".into()) };
        let output = safe_output(&source, suffix, ext)?;
        let w = width.unwrap_or(1920).clamp(64, 7680).to_string(); let h = height.unwrap_or(1080).clamp(64, 7680).to_string();
        let q = quality.unwrap_or(85).clamp(10, 100).to_string();
        let mut args = vec!["-hide_banner".into(), "-y".into(), "-i".into(), source.to_string_lossy().into_owned()];
        if action == "watermark" {
            let mark = PathBuf::from(watermark.as_deref().ok_or("Filigran görselini seç")?).canonicalize().map_err(|_| "Filigran görseli bulunamadı")?;
            args.extend(["-i".into(), mark.to_string_lossy().into_owned(), "-filter_complex".into(), format!("[1:v]scale=iw*0.22:-1[wm];[0:v][wm]overlay=W-w-24:H-h-24")]);
        } else if action == "resize" { args.extend(["-vf".into(), format!("scale={w}:{h}:force_original_aspect_ratio=decrease")]); }
        if ext == "jpg" || ext == "jpeg" { args.extend(["-q:v".into(), ((100 - q.parse::<u8>().unwrap_or(85)) / 4 + 2).to_string()]); }
        args.push(output.to_string_lossy().into_owned());
        sidecar_capture(&app, "binaries/ffmpeg", args).await?;
        results.push(JobResult { output: output.to_string_lossy().into_owned(), message: "Görsel hazır".into(), content: None });
    }
    Ok(results)
}

fn verify_whisper_model(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false; };
    if meta.len() < 140_000_000 { return false; }
    let Ok(mut file) = std::fs::File::open(path) else { return false; };
    let mut hasher = Sha1::new(); let mut buffer = [0_u8; 1024 * 1024];
    loop { match file.read(&mut buffer) { Ok(0) => break, Ok(n) => hasher.update(&buffer[..n]), Err(_) => return false } }
    format!("{:x}", hasher.finalize()) == "465707469ff3a37a2b9b8d8f89f2f99de7299dac"
}

async fn ensure_whisper_model(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_local_data_dir().map_err(|e| format!("Uygulama veri klasörü açılamadı: {e}"))?.join("models");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Model klasörü açılamadı: {e}"))?;
    let model = dir.join("ggml-base.bin");
    if verify_whisper_model(&model) { return Ok(model); }
    app.emit("job-progress", serde_json::json!({"phase":"model","message":"İlk kullanım: Türkçe konuşma modeli indiriliyor (~142 MB)"})).ok();
    let response = reqwest::Client::new().get("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin").send().await
        .map_err(|e| format!("Konuşma modeli indirilemedi: {e}"))?.error_for_status().map_err(|e| format!("Konuşma modeli sunucusu hata verdi: {e}"))?;
    let mut response = response;
    let mut file = tokio::fs::File::create(&model).await.map_err(|e| format!("Model dosyası açılamadı: {e}"))?;
    let mut total: u64 = 0; let mut hasher = Sha1::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| format!("Model indirme yarıda kesildi: {e}"))? {
        total += chunk.len() as u64;
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(|e| format!("Model diske yazılamadı: {e}"))?;
        if total % 8_000_000 < chunk.len() as u64 { app.emit("job-progress", serde_json::json!({"phase":"model","bytes":total})).ok(); }
    }
    file.flush().await.map_err(|e| format!("Model diske yazılamadı: {e}"))?;
    let digest = format!("{:x}", hasher.finalize());
    if total < 140_000_000 || digest != "465707469ff3a37a2b9b8d8f89f2f99de7299dac" { let _ = std::fs::remove_file(&model); return Err("İndirilen Whisper modelinin doğrulaması başarısız; tekrar dene".into()); }
    Ok(model)
}

async fn transcribe_source(app: &AppHandle, source: &Path, output: PathBuf) -> Result<JobResult, String> {
    let wav = safe_output(&source, "transcription-temp", "wav")?;
    let audio = sidecar_capture(app, "binaries/ffmpeg", vec!["-hide_banner".into(), "-y".into(), "-i".into(), source.to_string_lossy().into_owned(), "-vn".into(), "-ac".into(), "1".into(), "-ar".into(), "16000".into(), "-c:a".into(), "pcm_s16le".into(), wav.to_string_lossy().into_owned()]).await;
    if let Err(error) = audio { let _ = std::fs::remove_file(&wav); return Err(error); }
    let model = match ensure_whisper_model(app).await { Ok(path) => path, Err(error) => { let _ = std::fs::remove_file(&wav); return Err(error); } };
    app.emit("job-progress", serde_json::json!({"phase":"transcribing","message":"Konuşma metne çevriliyor"})).ok();
    let prefix = output.with_extension("");
    let result = sidecar_capture(app, "binaries/whisper-cli", vec!["-m".into(), model.to_string_lossy().into_owned(), "-f".into(), wav.to_string_lossy().into_owned(), "-l".into(), "auto".into(), "-otxt".into(), "-of".into(), prefix.to_string_lossy().into_owned(), "-np".into()]).await;
    let _ = std::fs::remove_file(&wav);
    result?;
    let content = std::fs::read_to_string(&output).map_err(|e| format!("Transkript okunamadı: {e}"))?;
    Ok(JobResult { output: output.to_string_lossy().into_owned(), message: "Transkript hazır".into(), content: Some(content) })
}

#[tauri::command]
async fn transcribe_media(app: AppHandle, input: String) -> Result<JobResult, String> {
    let source = PathBuf::from(&input).canonicalize().map_err(|_| "Ses veya video dosyası bulunamadı".to_string())?;
    if !source.is_file() { return Err("Seçilen yol bir dosya değil".into()); }
    let output = safe_output(&source, "transcript", "txt")?;
    transcribe_source(&app, &source, output).await
}

#[tauri::command]
async fn transcribe_url(app: AppHandle, url: String) -> Result<JobResult, String> {
    let trimmed = url.trim(); if !valid_web_url(trimmed) { return Err("Geçerli bir http/https video bağlantısı gir".into()); }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let temp = app.path().temp_dir().map_err(|e| format!("Geçici klasör bulunamadı: {e}"))?.join(format!("TamarilloAudio{stamp}"));
    std::fs::create_dir_all(&temp).map_err(|e| format!("Geçici klasör açılamadı: {e}"))?;
    let template = temp.join("source.%(ext)s");
    let args = vec!["--no-playlist".into(), "-f".into(), "bestaudio/best".into(), "-o".into(), template.to_string_lossy().into_owned(), trimmed.into()];
    if let Err(error) = sidecar_capture(&app, "binaries/yt-dlp", args).await { let _ = std::fs::remove_dir_all(&temp); return Err(error); }
    let source = std::fs::read_dir(&temp).ok().into_iter().flatten().filter_map(Result::ok).map(|entry| entry.path()).find(|path| path.is_file() && !path.to_string_lossy().ends_with(".part"));
    let Some(source) = source else { let _ = std::fs::remove_dir_all(&temp); return Err("Bağlantıdan ses akışı alınamadı".into()); };
    let folder = app.path().download_dir().map_err(|e| format!("İndirme klasörü bulunamadı: {e}"))?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("İndirme klasörü açılamadı: {e}"))?;
    let mut output = folder.join(format!("Tamarillo_transcript_{stamp}.txt")); let mut index = 1;
    while output.exists() { output = folder.join(format!("Tamarillo_transcript_{stamp}_{index}.txt")); index += 1; }
    let result = transcribe_source(&app, &source, output).await;
    let _ = std::fs::remove_dir_all(&temp);
    result
}

#[tauri::command]
async fn download_video(app: AppHandle, url: String, format: Option<String>) -> Result<JobResult, String> {
    let trimmed = url.trim(); if !valid_web_url(trimmed) { return Err("Geçerli bir http/https video bağlantısı gir".into()); }
    let folder = app.path().download_dir().map_err(|e| format!("İndirme klasörü bulunamadı: {e}"))?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("İndirme klasörü açılamadı: {e}"))?;
    let template = folder.join("%(title).180B [%(id)s].%(ext)s");
    let mut args = vec!["--no-playlist".into(), "--newline".into(), "-o".into(), template.to_string_lossy().into_owned()];
    if format.as_deref() == Some("mp3") { args.extend(["-x".into(), "--audio-format".into(), "mp3".into(), "--audio-quality".into(), "0".into()]); }
    else { args.extend(["-f".into(), "bestvideo*+bestaudio/best".into(), "--merge-output-format".into(), "mp4".into()]); }
    args.push(trimmed.into());
    app.emit("job-progress", serde_json::json!({"phase":"running","message":"Video indiriliyor"})).ok();
    let (output, _) = sidecar_capture(&app, "binaries/yt-dlp", args).await?;
    Ok(JobResult { output: folder.to_string_lossy().into_owned(), message: output.lines().last().unwrap_or("İndirme tamamlandı").to_string(), content: None })
}

#[tauri::command]
async fn download_captions(app: AppHandle, url: String, language: Option<String>) -> Result<JobResult, String> {
    let trimmed = url.trim(); if !valid_web_url(trimmed) { return Err("Geçerli bir http/https bağlantısı gir".into()); }
    let folder = app.path().download_dir().map_err(|e| format!("İndirme klasörü bulunamadı: {e}"))?;
    let template = folder.join("%(title).180B [%(id)s].%(ext)s");
    let lang = language.unwrap_or_else(|| "tr,en".into());
    let args = vec!["--no-playlist".into(), "--skip-download".into(), "--write-subs".into(), "--write-auto-subs".into(), "--sub-langs".into(), lang, "--sub-format".into(), "vtt".into(), "-o".into(), template.to_string_lossy().into_owned(), trimmed.into()];
    let (output, _) = sidecar_capture(&app, "binaries/yt-dlp", args).await?;
    let newest = std::fs::read_dir(&folder).ok().into_iter().flatten().filter_map(Result::ok).map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e|e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("vtt")))
        .max_by_key(|path| std::fs::metadata(path).and_then(|m|m.modified()).unwrap_or(UNIX_EPOCH));
    let content = newest.and_then(|path| std::fs::read_to_string(path).ok());
    Ok(JobResult { output: folder.to_string_lossy().into_owned(), message: output.lines().last().unwrap_or("Altyazı dosyası indirildi").to_string(), content })
}

#[tauri::command]
async fn probe_media(app: AppHandle, input: String) -> Result<String, String> {
    let source = PathBuf::from(input).canonicalize().map_err(|_| "Dosya bulunamadı".to_string())?;
    let (result, _) = sidecar_capture(&app, "binaries/ffprobe", vec!["-v".into(), "error".into(), "-show_entries".into(), "format=duration:stream=codec_name,width,height,codec_type".into(), "-of".into(), "default=noprint_wrappers=1".into(), source.to_string_lossy().into_owned()]).await?;
    Ok(result)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let quick_download = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD);
            app.handle().plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(move |handle, shortcut, event| {
                if shortcut == &quick_download && event.state() == ShortcutState::Pressed {
                    if let Some(window) = handle.get_webview_window("main") { let _ = window.show(); let _ = window.set_focus(); }
                    let _ = handle.emit("shortcut-download-link", ());
                }
            }).build())?;
            app.global_shortcut().register(quick_download)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![app_info, convert_media, trim_video_silence, process_images, transcribe_media, transcribe_url, download_video, download_captions, probe_media])
        .run(tauri::generate_context!()).expect("Tamarillo başlatılamadı");
}
