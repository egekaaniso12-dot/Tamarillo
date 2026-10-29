use std::{path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;
use std::io::Read;

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetAsyncKeyState(vkey: i32) -> i16;
    fn GetCursorPos(point: *mut WinPoint) -> i32;
    fn GetSystemMetrics(index: i32) -> i32;
}

const COPYRIGHT: &str = "Copyright EgeHS Medya 2026 - 2027";

#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct WinPoint { x: i32, y: i32 }

#[cfg(windows)]
static DESKTOP_DRAG_OVERLAY_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Serialize)]
struct AppInfo { name: &'static str, copyright: &'static str }
#[derive(Serialize)]
struct DropModifiers { shift: bool, control: bool, alt: bool }
#[derive(Serialize)]
struct JobResult { output: String, message: String, #[serde(skip_serializing_if = "Option::is_none")] content: Option<String> }

#[tauri::command]
fn app_info() -> AppInfo { AppInfo { name: "Tamarillo", copyright: COPYRIGHT } }

#[tauri::command]
fn drop_modifiers() -> DropModifiers {
    #[cfg(windows)]
    unsafe {
        return DropModifiers {
            shift: GetAsyncKeyState(0x10) < 0,
            control: GetAsyncKeyState(0x11) < 0,
            alt: GetAsyncKeyState(0x12) < 0,
        };
    }
    #[cfg(not(windows))]
    DropModifiers { shift: false, control: false, alt: false }
}

#[cfg(windows)]
fn set_drag_overlay(app: &AppHandle, visible: bool, cursor: Option<WinPoint>) {
    if let Some(window) = app.get_webview_window("drag-overlay") {
        if visible {
            unsafe {
                // Keep the transparent HWND small and beside the pointer. A full
                // virtual-screen layered window dims secondary displays and can
                // make protected video surfaces black while a drag is active.
                let x = GetSystemMetrics(76); // SM_XVIRTUALSCREEN
                let y = GetSystemMetrics(77); // SM_YVIRTUALSCREEN
                let width = GetSystemMetrics(78).max(740); // SM_CXVIRTUALSCREEN
                let height = GetSystemMetrics(79).max(570); // SM_CYVIRTUALSCREEN
                let point = cursor.unwrap_or(WinPoint { x: width / 2, y: height / 2 });
                let left = (point.x - 370).clamp(x, x + width - 740);
                let top = (point.y - 285).clamp(y, y + height - 570);
                let _ = window.set_position(PhysicalPosition::new(left, top));
                let _ = window.set_size(PhysicalSize::new(740, 570));
            }
            let _ = window.show();
        } else { let _ = window.hide(); }
    }
}

#[cfg(windows)]
fn arrange_main_window(app: &AppHandle, layout: &str) -> Result<(), String> {
    let width = unsafe { GetSystemMetrics(0).max(1) } as u32;
    let height = unsafe { GetSystemMetrics(1).max(1) } as u32;
    let third = width / 3;
    let (x, w) = match layout {
        "left-third" => (0, third),
        "middle-third" => (third as i32, third),
        "right-third" => ((third * 2) as i32, width - third * 2),
        "left-half" => (0, width / 2),
        "right-half" => ((width / 2) as i32, width - width / 2),
        "left-two-thirds" => (0, width * 2 / 3),
        "right-two-thirds" => ((width / 3) as i32, width - width / 3),
        _ => return Err("Pencere yerleşimi tanınmıyor".into()),
    };
    let window = app.get_webview_window("main").ok_or("Tamarillo ana penceresi bulunamadı")?;
    window.set_position(PhysicalPosition::new(x, 0)).map_err(|e| e.to_string())?;
    window.set_size(PhysicalSize::new(w.max(1), height)).map_err(|e| e.to_string())?;
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

#[cfg(windows)]
fn start_desktop_drag_monitor(app: AppHandle) {
    std::thread::spawn(move || {
        let mut origin: Option<WinPoint> = None;
        loop {
            let mut point = WinPoint::default();
            let cursor_ok = unsafe { GetCursorPos(&mut point) } != 0;
            let left_down = unsafe { GetAsyncKeyState(0x01) } < 0;
            let modifier_down = unsafe { GetAsyncKeyState(0x10) | GetAsyncKeyState(0x11) | GetAsyncKeyState(0x12) } < 0;
            if left_down && cursor_ok {
                let start = *origin.get_or_insert(point);
                let moved = (point.x - start.x).abs() + (point.y - start.y).abs() > 8;
                if moved && modifier_down && !DESKTOP_DRAG_OVERLAY_ACTIVE.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    set_drag_overlay(&app, true, Some(point));
                    let _ = app.emit("desktop-drag-session", "start");
                } else if !modifier_down && DESKTOP_DRAG_OVERLAY_ACTIVE.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    set_drag_overlay(&app, false, None);
                    let _ = app.emit("desktop-drag-session", "cancel");
                }
            } else {
                origin = None;
                if DESKTOP_DRAG_OVERLAY_ACTIVE.load(std::sync::atomic::Ordering::SeqCst) {
                    // Let OLE deliver Drop before the topmost overlay is hidden.
                    std::thread::sleep(std::time::Duration::from_millis(180));
                    if DESKTOP_DRAG_OVERLAY_ACTIVE.swap(false, std::sync::atomic::Ordering::SeqCst) {
                        set_drag_overlay(&app, false, None);
                        let _ = app.emit("desktop-drag-session", "end");
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    });
}

#[tauri::command]
async fn complete_desktop_drop(app: AppHandle, paths: Vec<String>, page: Option<String>, action: Option<String>) -> Result<(), String> {
    if paths.is_empty() { return Err("Sürüklenen dosya alınamadı".into()); }
    #[cfg(windows)] DESKTOP_DRAG_OVERLAY_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
    #[cfg(windows)] set_drag_overlay(&app, false, None);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    app.emit("desktop-file-dropped", serde_json::json!({"paths": paths, "page": page})).map_err(|e| e.to_string())?;
    if let Some(action) = action {
        if let Some(layout) = action.strip_prefix("layout:") {
            #[cfg(windows)] let result = arrange_main_window(&app, layout);
            #[cfg(not(windows))] let result: Result<(), String> = Err("Pencere yerleşimi yalnızca Windows masaüstünde kullanılabilir".into());
            match result {
                Ok(()) => { app.emit("desktop-action-result", serde_json::json!({"ok":true,"message":"Pencere yerleşimi uygulandı","outputs":[]})).map_err(|e| e.to_string())?; }
                Err(error) => { app.emit("desktop-action-result", serde_json::json!({"ok":false,"message":error,"outputs":[]})).map_err(|e| e.to_string())?; }
            }
            return Ok(());
        }
        app.emit("desktop-video-action-request", serde_json::json!({"paths":paths,"action":action})).map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn video_wheel_action(app: &AppHandle, input: &str, action: &str) -> Result<Vec<JobResult>, String> {
    let source = PathBuf::from(input).canonicalize().map_err(|_| format!("Video bulunamadı: {input}"))?;
    if !source.is_file() { return Err("Seçilen yol bir dosya değil".into()); }
    let ext = source.extension().and_then(|value| value.to_str()).unwrap_or("mp4").to_ascii_lowercase();
    if !["mp4","mkv","mov","webm","avi","m4v"].contains(&ext.as_str()) { return Err("Video çarkı yalnızca video dosyalarında çalışır".into()); }
    let mut outputs = Vec::new();
    match action {
        "compress" => {
            let out = safe_output(&source, "compressed", "mp4")?;
            sidecar_capture(app, "ffmpeg", vec!["-hide_banner".into(),"-y".into(),"-i".into(),source.to_string_lossy().into_owned(),"-c:v".into(),"libx264".into(),"-crf".into(),"26".into(),"-preset".into(),"fast".into(),"-c:a".into(),"aac".into(),"-b:a".into(),"128k".into(),out.to_string_lossy().into_owned()]).await?;
            outputs.push(out);
        }
        "metadata" => {
            let out = safe_output(&source, "metadata-removed", &ext)?;
            sidecar_capture(app, "ffmpeg", vec!["-hide_banner".into(),"-y".into(),"-i".into(),source.to_string_lossy().into_owned(),"-map_metadata".into(),"-1".into(),"-c".into(),"copy".into(),out.to_string_lossy().into_owned()]).await?;
            outputs.push(out);
        }
        "mute" => {
            let out = safe_output(&source, "muted", &ext)?;
            sidecar_capture(app, "ffmpeg", vec!["-hide_banner".into(),"-y".into(),"-i".into(),source.to_string_lossy().into_owned(),"-an".into(),"-c:v".into(),"copy".into(),out.to_string_lossy().into_owned()]).await?;
            outputs.push(out);
        }
        "crop" => {
            let out = safe_output(&source, "crop-16x9", "mp4")?;
            sidecar_capture(app, "ffmpeg", vec!["-hide_banner".into(),"-y".into(),"-i".into(),source.to_string_lossy().into_owned(),"-vf".into(),"scale=1920:1080:force_original_aspect_ratio=increase,crop=1920:1080".into(),"-c:v".into(),"libx264".into(),"-crf".into(),"23".into(),"-c:a".into(),"aac".into(),out.to_string_lossy().into_owned()]).await?;
            outputs.push(out);
        }
        "speed" => {
            let out = safe_output(&source, "speed-125", "mp4")?;
            sidecar_capture(app, "ffmpeg", vec!["-hide_banner".into(),"-y".into(),"-i".into(),source.to_string_lossy().into_owned(),"-vf".into(),"setpts=0.8*PTS".into(),"-af".into(),"atempo=1.25".into(),"-c:v".into(),"libx264".into(),"-crf".into(),"23".into(),"-c:a".into(),"aac".into(),out.to_string_lossy().into_owned()]).await?;
            outputs.push(out);
        }
        "screenshot" => {
            let out = safe_output(&source, "frame-1s", "png")?;
            sidecar_capture(app, "ffmpeg", vec!["-hide_banner".into(),"-y".into(),"-ss".into(),"1".into(),"-i".into(),source.to_string_lossy().into_owned(),"-frames:v".into(),"1".into(),out.to_string_lossy().into_owned()]).await?;
            outputs.push(out);
        }
        "split" => {
            let probe = sidecar_capture(app, "ffprobe", vec!["-v".into(),"error".into(),"-show_entries".into(),"format=duration".into(),"-of".into(),"default=noprint_wrappers=1:nokey=1".into(),source.to_string_lossy().into_owned()]).await?;
            let duration: f64 = probe.0.parse().map_err(|_| "Video süresi okunamadı".to_string())?;
            if duration < 2.0 { return Err("Videoyu ikiye bölmek için video en az 2 saniye olmalı".into()); }
            let middle = duration / 2.0;
            for (index, args) in [
                vec!["-ss".into(),"0".into(),"-t".into(),format!("{middle:.3}")],
                vec!["-ss".into(),format!("{middle:.3}")],
            ].into_iter().enumerate() {
                let out = safe_output(&source, if index == 0 { "part-1" } else { "part-2" }, &ext)?;
                let mut command = vec!["-hide_banner".into(),"-y".into(),"-i".into(),source.to_string_lossy().into_owned()];
                command.extend(args); command.extend(["-c".into(),"copy".into(),out.to_string_lossy().into_owned()]);
                sidecar_capture(app, "ffmpeg", command).await?;
                outputs.push(out);
            }
        }
        "trim" => {
            let out = safe_output(&source, "first-30s", "mp4")?;
            sidecar_capture(app, "ffmpeg", vec!["-hide_banner".into(),"-y".into(),"-i".into(),source.to_string_lossy().into_owned(),"-t".into(),"30".into(),"-c:v".into(),"libx264".into(),"-crf".into(),"23".into(),"-c:a".into(),"aac".into(),out.to_string_lossy().into_owned()]).await?;
            outputs.push(out);
        }
        "blur" => {
            let out = safe_output(&source, "blurred", "mp4")?;
            sidecar_capture(app, "ffmpeg", vec!["-hide_banner".into(),"-y".into(),"-i".into(),source.to_string_lossy().into_owned(),"-vf".into(),"boxblur=10:1".into(),"-c:v".into(),"libx264".into(),"-crf".into(),"23".into(),"-c:a".into(),"aac".into(),out.to_string_lossy().into_owned()]).await?;
            outputs.push(out);
        }
        _ => return Err("Video çarkı işlemi tanınmıyor".into()),
    }
    Ok(outputs.into_iter().map(|path| JobResult { output: path.to_string_lossy().into_owned(), message: "Video çarkı işlemi tamamlandı".into(), content: None }).collect())
}

#[tauri::command]
async fn process_video_action(
    app: AppHandle, input: String, action: String, output_dir: Option<String>,
    quality: Option<u8>, preset: Option<String>, start: Option<f64>, duration: Option<f64>,
    crop_width: Option<u32>, crop_height: Option<u32>, crop_x: Option<u32>, crop_y: Option<u32>,
    speed: Option<f64>, frame_time: Option<f64>, split_parts: Option<u8>, blur_radius: Option<u8>,
) -> Result<Vec<JobResult>, String> {
    let source = PathBuf::from(&input).canonicalize().map_err(|_| "Video dosyası bulunamadı".to_string())?;
    if !source.is_file() { return Err("Seçilen yol bir dosya değil".into()); }
    let ext = source.extension().and_then(|v| v.to_str()).unwrap_or("mp4").to_ascii_lowercase();
    if !["mp4","mkv","mov","webm","avi","m4v"].contains(&ext.as_str()) { return Err("Desteklenmeyen video biçimi".into()); }
    let folder = if let Some(dir) = output_dir.filter(|s| !s.trim().is_empty()) {
        let path = PathBuf::from(dir);
        std::fs::create_dir_all(&path).map_err(|e| format!("Çıktı klasörü oluşturulamadı: {e}"))?;
        path.canonicalize().map_err(|e| format!("Çıktı klasörü açılamadı: {e}"))?
    } else { source.parent().ok_or("Kaynak klasörü bulunamadı")?.to_path_buf() };
    let make_out = |suffix: &str, extension: &str| -> Result<PathBuf, String> {
        let stem = source.file_stem().and_then(|s| s.to_str()).ok_or("Dosya adı okunamadı")?;
        let mut output = folder.join(format!("{stem}_{suffix}.{extension}")); let mut i = 1;
        while output.exists() { output = folder.join(format!("{stem}_{suffix}_{i}.{extension}")); i += 1; }
        Ok(output)
    };
    let mut outputs = Vec::new();
    let input_path = source.to_string_lossy().into_owned();
    match action.as_str() {
        "compress" => {
            let crf = quality.unwrap_or(26).clamp(16, 35).to_string();
            let allowed = ["ultrafast","superfast","veryfast","faster","fast","medium","slow","slower"];
            let speed = preset.as_deref().filter(|p| allowed.contains(p)).unwrap_or("fast");
            let out = run_video_ffmpeg(&app,&source,&folder,"compressed","mp4",vec!["-c:v".into(),"libx264".into(),"-crf".into(),crf,"-preset".into(),speed.into(),"-c:a".into(),"aac".into(),"-b:a".into(),"128k".into()]).await?; outputs.push(out);
        }
        "metadata" => {
            let out = make_out("metadata-removed", &ext)?;
            sidecar_capture(&app,"ffmpeg",vec!["-hide_banner".into(),"-y".into(),"-i".into(),input_path,"-map_metadata".into(),"-1".into(),"-c".into(),"copy".into(),out.to_string_lossy().into_owned()]).await?; outputs.push(out);
        }
        "mute" => {
            let out = make_out("muted", &ext)?;
            sidecar_capture(&app,"ffmpeg",vec!["-hide_banner".into(),"-y".into(),"-i".into(),input_path,"-an".into(),"-c:v".into(),"copy".into(),out.to_string_lossy().into_owned()]).await?; outputs.push(out);
        }
        "crop" => {
            let probe = sidecar_capture(&app,"ffprobe",vec!["-v".into(),"error".into(),"-select_streams".into(),"v:0".into(),"-show_entries".into(),"stream=width,height".into(),"-of".into(),"csv=s=x:p=0".into(),input_path.clone()]).await?;
            let mut dimensions = probe.0.trim().split('x');
            let source_width: u32 = dimensions.next().and_then(|s|s.parse().ok()).ok_or("Video genişliği okunamadı")?;
            let source_height: u32 = dimensions.next().and_then(|s|s.parse().ok()).ok_or("Video yüksekliği okunamadı")?;
            let w = (crop_width.unwrap_or(1920).clamp(16,7680) & !1).min(source_width & !1);
            let h = (crop_height.unwrap_or(1080).clamp(16,7680) & !1).min(source_height & !1);
            let x = crop_x.unwrap_or(0) & !1; let y = crop_y.unwrap_or(0) & !1;
            if w < 2 || h < 2 || x.saturating_add(w) > source_width || y.saturating_add(h) > source_height { return Err(format!("Kırpma alanı kaynak video sınırlarının dışında ({source_width}×{source_height})")); }
            let out = run_video_ffmpeg(&app,&source,&folder,"cropped","mp4",vec!["-vf".into(),format!("crop={w}:{h}:{x}:{y}"),"-c:v".into(),"libx264".into(),"-crf".into(),quality.unwrap_or(23).clamp(16,35).to_string(),"-preset".into(),"fast".into(),"-c:a".into(),"aac".into()]).await?; outputs.push(out);
        }
        "speed" => {
            let value = speed.unwrap_or(1.25).clamp(0.25,4.0);
            let mut remaining = value; let mut filters = Vec::new();
            while remaining > 2.0 { filters.push("atempo=2.0".to_string()); remaining /= 2.0; }
            while remaining < 0.5 { filters.push("atempo=0.5".to_string()); remaining /= 0.5; }
            filters.push(format!("atempo={remaining:.3}"));
            let out = run_video_ffmpeg(&app,&source,&folder,"speed","mp4",vec!["-vf".into(),format!("setpts={:.6}*PTS",1.0/value),"-af".into(),filters.join(","),"-c:v".into(),"libx264".into(),"-crf".into(),quality.unwrap_or(23).clamp(16,35).to_string(),"-preset".into(),"fast".into(),"-c:a".into(),"aac".into()]).await?; outputs.push(out);
        }
        "screenshot" => {
            let at = frame_time.unwrap_or(1.0).max(0.0).to_string(); let out = make_out("frame", "png")?;
            sidecar_capture(&app,"ffmpeg",vec!["-hide_banner".into(),"-y".into(),"-ss".into(),at,"-i".into(),input_path,"-frames:v".into(),"1".into(),out.to_string_lossy().into_owned()]).await?; outputs.push(out);
        }
        "split" => {
            let probe = sidecar_capture(&app,"ffprobe",vec!["-v".into(),"error".into(),"-show_entries".into(),"format=duration".into(),"-of".into(),"default=noprint_wrappers=1:nokey=1".into(),input_path.clone()]).await?;
            let total: f64 = probe.0.parse().map_err(|_| "Video süresi okunamadı".to_string())?;
            let parts = split_parts.unwrap_or(2).clamp(2,20); let chunk = total / parts as f64;
            for i in 0..parts { let out = make_out(&format!("part-{:02}",i+1),"mp4")?;
                sidecar_capture(&app,"ffmpeg",vec!["-hide_banner".into(),"-y".into(),"-ss".into(),format!("{:.3}",i as f64*chunk),"-i".into(),input_path.clone(),"-t".into(),format!("{:.3}",if i+1==parts { total-i as f64*chunk } else { chunk }),"-c:v".into(),"libx264".into(),"-crf".into(),quality.unwrap_or(23).clamp(16,35).to_string(),"-preset".into(),"fast".into(),"-c:a".into(),"aac".into(),out.to_string_lossy().into_owned()]).await?; outputs.push(out); }
        }
        "trim" => {
            let from = start.unwrap_or(0.0).max(0.0); let length = duration.unwrap_or(30.0).clamp(0.1,86400.0);
            let out = make_out("trimmed", "mp4")?;
            sidecar_capture(&app,"ffmpeg",vec!["-hide_banner".into(),"-y".into(),"-ss".into(),format!("{from:.3}"),"-i".into(),input_path,"-t".into(),format!("{length:.3}"),"-c:v".into(),"libx264".into(),"-crf".into(),quality.unwrap_or(23).clamp(16,35).to_string(),"-preset".into(),"fast".into(),"-c:a".into(),"aac".into(),out.to_string_lossy().into_owned()]).await?; outputs.push(out);
        }
        "blur" => {
            let radius = blur_radius.unwrap_or(10).clamp(1,30).to_string();
            let out = run_video_ffmpeg(&app,&source,&folder,"blurred","mp4",vec!["-vf".into(),format!("boxblur={radius}:1"),"-c:v".into(),"libx264".into(),"-crf".into(),quality.unwrap_or(23).clamp(16,35).to_string(),"-preset".into(),"fast".into(),"-c:a".into(),"aac".into()]).await?; outputs.push(out);
        }
        _ => return Err("Video işlemi tanınmıyor".into()),
    }
    Ok(outputs.into_iter().map(|path| JobResult { output: path.to_string_lossy().into_owned(), message: "Video işlemi tamamlandı".into(), content: None }).collect())
}

fn safe_output(input: &Path, suffix: &str, ext: &str) -> Result<PathBuf, String> {
    let stem = input.file_stem().and_then(|s| s.to_str()).ok_or("Dosya adı okunamadı")?;
    let parent = input.parent().ok_or("Kaynak klasörü bulunamadı")?;
    let mut output = parent.join(format!("{stem}_{suffix}.{ext}"));
    let mut n = 1;
    while output.exists() { output = parent.join(format!("{stem}_{suffix}_{n}.{ext}")); n += 1; }
    Ok(output)
}

async fn run_video_ffmpeg(app: &AppHandle, source: &Path, folder: &Path, suffix: &str, ext: &str, args: Vec<String>) -> Result<PathBuf, String> {
    let stem = source.file_stem().and_then(|s| s.to_str()).ok_or("Dosya adı okunamadı")?;
    let mut output = folder.join(format!("{stem}_{suffix}.{ext}")); let mut index = 1;
    while output.exists() { output = folder.join(format!("{stem}_{suffix}_{index}.{ext}")); index += 1; }
    let mut command = vec!["-hide_banner".into(), "-y".into(), "-i".into(), source.to_string_lossy().into_owned()];
    command.extend(args); command.extend(["-movflags".into(), "+faststart".into()]); command.push(output.to_string_lossy().into_owned());
    sidecar_capture(app,"ffmpeg",command).await?;
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

fn bundled_ffmpeg_path() -> Result<String, String> {
    let executable = std::env::current_exe().map_err(|e| format!("Uygulama yolu okunamadı: {e}"))?;
    let directory = executable.parent().ok_or("Uygulama klasörü bulunamadı")?;
    Ok(directory.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }).to_string_lossy().into_owned())
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
    sidecar_capture(&app, "ffmpeg", command_args).await?;
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
    let (_, diagnostic) = sidecar_capture(&app, "ffmpeg", vec!["-hide_banner".into(), "-i".into(), source.to_string_lossy().into_owned(), "-af".into(), format!("silencedetect=noise={limit}dB:d={duration}"), "-f".into(), "null".into(), "-".into()]).await?;
    let probe = sidecar_capture(&app, "ffprobe", vec!["-v".into(), "error".into(), "-show_entries".into(), "format=duration".into(), "-of".into(), "default=noprint_wrappers=1:nokey=1".into(), source.to_string_lossy().into_owned()]).await?;
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
    sidecar_capture(&app, "ffmpeg", vec!["-hide_banner".into(), "-y".into(), "-i".into(), source.to_string_lossy().into_owned(), "-filter_complex".into(), filters.join(";").replace(" ", ""), "-map".into(), "[v]".into(), "-map".into(), "[a]".into(), "-c:v".into(), "libx264".into(), "-crf".into(), "23".into(), "-c:a".into(), "aac".into(), out.to_string_lossy().into_owned()]).await?;
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
        sidecar_capture(&app, "ffmpeg", args).await?;
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
    let audio = sidecar_capture(app, "ffmpeg", vec!["-hide_banner".into(), "-y".into(), "-i".into(), source.to_string_lossy().into_owned(), "-vn".into(), "-ac".into(), "1".into(), "-ar".into(), "16000".into(), "-c:a".into(), "pcm_s16le".into(), wav.to_string_lossy().into_owned()]).await;
    if let Err(error) = audio { let _ = std::fs::remove_file(&wav); return Err(error); }
    let model = match ensure_whisper_model(app).await { Ok(path) => path, Err(error) => { let _ = std::fs::remove_file(&wav); return Err(error); } };
    app.emit("job-progress", serde_json::json!({"phase":"transcribing","message":"Konuşma metne çevriliyor"})).ok();
    let prefix = output.with_extension("");
    let result = sidecar_capture(app, "whisper-cli", vec!["-m".into(), model.to_string_lossy().into_owned(), "-f".into(), wav.to_string_lossy().into_owned(), "-l".into(), "auto".into(), "-otxt".into(), "-of".into(), prefix.to_string_lossy().into_owned(), "-np".into()]).await;
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
    if let Err(error) = sidecar_capture(&app, "yt-dlp", args).await { let _ = std::fs::remove_dir_all(&temp); return Err(error); }
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
    let mut args = vec!["--no-playlist".into(), "--newline".into(), "--ffmpeg-location".into(), bundled_ffmpeg_path()?, "-o".into(), template.to_string_lossy().into_owned()];
    if format.as_deref() == Some("mp3") { args.extend(["-x".into(), "--audio-format".into(), "mp3".into(), "--audio-quality".into(), "0".into()]); }
    else { args.extend(["-f".into(), "bestvideo*+bestaudio/best".into(), "--merge-output-format".into(), "mp4".into()]); }
    args.push(trimmed.into());
    app.emit("job-progress", serde_json::json!({"phase":"running","message":"Video indiriliyor"})).ok();
    let (output, _) = sidecar_capture(&app, "yt-dlp", args).await?;
    Ok(JobResult { output: folder.to_string_lossy().into_owned(), message: output.lines().last().unwrap_or("İndirme tamamlandı").to_string(), content: None })
}

#[tauri::command]
async fn download_captions(app: AppHandle, url: String, language: Option<String>) -> Result<JobResult, String> {
    let trimmed = url.trim(); if !valid_web_url(trimmed) { return Err("Geçerli bir http/https bağlantısı gir".into()); }
    let folder = app.path().download_dir().map_err(|e| format!("İndirme klasörü bulunamadı: {e}"))?;
    let template = folder.join("%(title).180B [%(id)s].%(ext)s");
    let lang = language.unwrap_or_else(|| "tr,en".into());
    let args = vec!["--no-playlist".into(), "--skip-download".into(), "--write-subs".into(), "--write-auto-subs".into(), "--sub-langs".into(), lang, "--sub-format".into(), "vtt".into(), "-o".into(), template.to_string_lossy().into_owned(), trimmed.into()];
    let (output, _) = sidecar_capture(&app, "yt-dlp", args).await?;
    let newest = std::fs::read_dir(&folder).ok().into_iter().flatten().filter_map(Result::ok).map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e|e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("vtt")))
        .max_by_key(|path| std::fs::metadata(path).and_then(|m|m.modified()).unwrap_or(UNIX_EPOCH));
    let content = newest.and_then(|path| std::fs::read_to_string(path).ok());
    Ok(JobResult { output: folder.to_string_lossy().into_owned(), message: output.lines().last().unwrap_or("Altyazı dosyası indirildi").to_string(), content })
}

#[tauri::command]
async fn probe_media(app: AppHandle, input: String) -> Result<String, String> {
    let source = PathBuf::from(input).canonicalize().map_err(|_| "Dosya bulunamadı".to_string())?;
    let (result, _) = sidecar_capture(&app, "ffprobe", vec!["-v".into(), "error".into(), "-show_entries".into(), "format=duration:stream=codec_name,width,height,codec_type".into(), "-of".into(), "default=noprint_wrappers=1".into(), source.to_string_lossy().into_owned()]).await?;
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
            #[cfg(windows)] {
                WebviewWindowBuilder::new(app, "drag-overlay", WebviewUrl::App("index.html".into()))
                    .title("Tamarillo drag wheel")
                    .decorations(false).transparent(true).always_on_top(true)
                    .skip_taskbar(true).focusable(false).shadow(false).resizable(false)
                    .inner_size(1.0, 1.0).visible(false).build()?;
                start_desktop_drag_monitor(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![app_info, drop_modifiers, complete_desktop_drop, process_video_action, convert_media, trim_video_silence, process_images, transcribe_media, transcribe_url, download_video, download_captions, probe_media])
        .run(tauri::generate_context!()).expect("Tamarillo başlatılamadı");
}
