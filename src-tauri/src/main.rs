#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod clipboard;
mod desktop;
mod ocr;
mod snip;
mod storage;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{
    Emitter, Manager, State,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;

struct Runtime {
    captures: Mutex<VecDeque<String>>,
    enabled: AtomicBool,
}
fn collection_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    #[cfg(debug_assertions)]
    if let Some(directory) = std::env::var_os("DARKROOM_QA_DIR") {
        return Ok(directory.into());
    }
    app.path().app_data_dir().map_err(|e| e.to_string())
}
#[cfg(debug_assertions)]
fn record_qa_visibility(app: &tauri::AppHandle, phase: &str) {
    if let Some(directory) = std::env::var_os("DARKROOM_QA_DIR") {
        let directory = std::path::PathBuf::from(directory);
        let _ = std::fs::create_dir_all(&directory);
        if let Some(w) = app.get_webview_window("main") {
            let data = serde_json::json!({"visible":w.is_visible().unwrap_or(true),"fullscreen":w.is_fullscreen().unwrap_or(true),"backgroundRequested":std::env::args().any(|a|a=="--background"),"screenSizes":capture_screen_sizes(app.clone()).unwrap_or_default()});
            let _ = std::fs::write(
                directory.join(format!("visibility-{phase}.json")),
                data.to_string(),
            );
        }
    }
}
fn reveal(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn publish_capture(app: &tauri::AppHandle, image: String) {
    if let Ok(mut queue) = app.state::<Runtime>().captures.lock() {
        if queue.len() >= 8 {
            queue.pop_front();
        }
        queue.push_back(image);
    }
    let _ = app.emit_to("main", "capture-ready", ());
    #[cfg(debug_assertions)]
    record_qa_visibility(app, "capture");
}

#[cfg(debug_assertions)]
fn inject_capture_fixture(app: tauri::AppHandle) {
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--capture-fixture") {
        if let Some(path) = args.get(index + 1).cloned() {
            let second_path = args.get(index + 2).cloned();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(2));
                if let Ok(bytes) = std::fs::read(path) {
                    let image = format!("data:image/png;base64,{}", STANDARD.encode(bytes));
                    publish_capture(&app, image.clone());
                    // Exercise a second screenshot while the first save is queued.
                    std::thread::sleep(Duration::from_millis(100));
                    let second = second_path
                        .and_then(|path| std::fs::read(path).ok())
                        .map(|bytes| format!("data:image/png;base64,{}", STANDARD.encode(bytes)))
                        .unwrap_or(image);
                    publish_capture(&app, second);
                }
            });
        }
    }
}

#[tauri::command]
fn take_captures(state: State<Runtime>) -> Vec<String> {
    state
        .captures
        .lock()
        .map(|mut q| q.drain(..).collect())
        .unwrap_or_default()
}
#[tauri::command]
fn set_monitor(app: tauri::AppHandle, state: State<Runtime>, enabled: bool) -> Result<(), String> {
    let dir = collection_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("monitor.json"),
        if enabled { "true" } else { "false" },
    )
    .map_err(|e| e.to_string())?;
    state.enabled.store(enabled, Ordering::Relaxed);
    let _ = app.emit("monitor-changed", enabled);
    Ok(())
}
#[tauri::command]
fn get_monitor(state: State<Runtime>) -> bool {
    state.enabled.load(Ordering::Relaxed)
}
#[tauri::command]
fn hide_window(window: tauri::WebviewWindow) {
    let _ = window.hide();
}
#[tauri::command]
fn capture_screen_sizes(app: tauri::AppHandle) -> Result<Vec<(u32, u32)>, String> {
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let mut sizes = Vec::new();
    let (mut left, mut top, mut right, mut bottom) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
    for monitor in monitors {
        let size = monitor.size();
        sizes.push((size.width, size.height));
        left = left.min(monitor.position().x as i64);
        top = top.min(monitor.position().y as i64);
        right = right.max(monitor.position().x as i64 + size.width as i64);
        bottom = bottom.max(monitor.position().y as i64 + size.height as i64);
    }
    if right > left && bottom > top {
        sizes.push(((right - left) as u32, (bottom - top) as u32));
    }
    sizes.sort_unstable();
    sizes.dedup();
    Ok(sizes)
}
#[tauri::command]
fn minimize_window(window: tauri::WebviewWindow) {
    let _ = window.minimize();
}
#[tauri::command]
fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}
#[tauri::command]
fn copy_text(window: tauri::WebviewWindow, text: String) -> Result<(), String> {
    use windows::Win32::{
        Foundation::{GlobalFree, HANDLE},
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        },
    };
    if text.len() > 200_000 {
        return Err("文字太长，请分段复制".into());
    }
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let mem = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).map_err(|e| e.to_string())?;
        let ptr = GlobalLock(mem);
        if ptr.is_null() {
            let _ = GlobalFree(Some(mem));
            return Err("暂时无法复制".into());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr.cast::<u16>(), wide.len());
        let _ = GlobalUnlock(mem);
        let owner = window.hwnd().map_err(|e| e.to_string())?;
        if let Err(e) = OpenClipboard(Some(owner)) {
            let _ = GlobalFree(Some(mem));
            return Err(e.to_string());
        }
        let result = EmptyClipboard().and_then(|_| SetClipboardData(13, Some(HANDLE(mem.0))));
        let _ = CloseClipboard();
        if let Err(e) = result {
            let _ = GlobalFree(Some(mem));
            return Err(e.to_string());
        }
    }
    Ok(())
}
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://"))
        || url.len() > 8000
        || url.chars().any(|c| c.is_control())
    {
        return Err("仅支持 http / https 网页链接".into());
    }
    use windows::{
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        core::{PCWSTR, w},
    };
    let wide: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err("无法打开浏览器，请复制链接后打开".into());
    }
    Ok(())
}
#[tauri::command]
async fn paste_image() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(clipboard::read_image)
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn recognize_image(data_url: String, language: String) -> Result<ocr::OcrOutput, String> {
    if data_url.len() > 48_000_000 {
        return Err("选区过大，请缩小后重试".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let encoded = data_url
            .strip_prefix("data:image/png;base64,")
            .ok_or("需要 PNG 格式图片")?;
        let bytes = STANDARD.decode(encoded).map_err(|e| e.to_string())?;
        ocr::recognize(&bytes, &language)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn ocr_languages() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(ocr::languages)
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn load_state(app: tauri::AppHandle) -> Result<Option<serde_json::Value>, String> {
    storage::load(&collection_dir(&app)?)
}
#[tauri::command]
async fn save_state(app: tauri::AppHandle, data: serde_json::Value) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = collection_dir(&app)?;
        storage::save(&dir, &data)?;
        let _ = app.emit_to("desktop-line", "desktop-photos-changed", ());
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn export_file(
    app: tauri::AppHandle,
    content: String,
    kind: String,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let name = if kind == "json" {
            "截图冲洗室备份.json"
        } else {
            "截图冲洗室便签.md"
        };
        let dir = app.path().download_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis();
        let file = dir.join(format!(
            "{}-{}.{}",
            name.rsplit_once('.').unwrap().0,
            now,
            if kind == "json" { "json" } else { "md" }
        ));
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
            .map_err(|e| e.to_string())?;
        use std::io::Write;
        f.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
        Ok(Some(file.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn open_downloads(app: tauri::AppHandle) -> Result<(), String> {
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    std::process::Command::new("explorer.exe")
        .arg(dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
fn open_data_directory(app: tauri::AppHandle) -> Result<(), String> {
    let dir = collection_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer.exe")
        .arg(dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
async fn start_snip(window: tauri::WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())?;
    let result = tauri::async_runtime::spawn_blocking(|| {
        std::thread::sleep(Duration::from_millis(220));
        unsafe {
            use windows::{
                Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
                core::{PCWSTR, w},
            };
            let uri: Vec<u16> = snip::RECTANGLE_URI.encode_utf16().chain(Some(0)).collect();
            let result = ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(uri.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            );
            if result.0 as isize <= 32 {
                return Err(
                    "无法打开 Windows 矩形截图，请按 Win + Shift + S 后选择矩形模式。".to_string(),
                );
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?;
    if result.is_err() {
        let _ = window.show();
        let _ = window.set_focus();
    }
    result
}

fn configure_runtime() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let mut candidates = vec![dir.join("WebView2")];
            for p in dir.ancestors().take(5) {
                candidates.push(p.join("WebView2"));
            }
            if let Some(runtime) = candidates
                .into_iter()
                .find(|p| p.join("msedgewebview2.exe").exists())
            {
                unsafe {
                    std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", runtime);
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--self-test") {
        let report = std::thread::spawn(move || {
            let languages = ocr::languages();
            let sample = args.get(2).and_then(|p| std::fs::read(p).ok());
            let recognized = sample.as_ref().map(|bytes| ocr::recognize(bytes, "auto"));
            let repeated = sample.as_ref().map(|bytes| ocr::recognize(bytes, "auto"));
            let out =
                serde_json::json!({"languages":languages,"ocr":recognized,"repeatedOcr":repeated});
            if let Some(path) = args.get(3) {
                let _ = std::fs::write(path, serde_json::to_vec_pretty(&out).unwrap());
            }
        })
        .join();
        let _ = report;
        return;
    }
    // Keep one tray instance. An additional launch brings the existing window back.
    let _instance = unsafe {
        use windows::{
            Win32::{
                Foundation::{ERROR_ALREADY_EXISTS, GetLastError},
                System::Threading::CreateMutexW,
                UI::WindowsAndMessaging::{
                    FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
                },
            },
            core::w,
        };
        let mutex = CreateMutexW(None, false, w!("Local\\ScreenshotDarkroomDesktop"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(hwnd) = FindWindowW(None, w!("截图冲洗室 · Darkroom")) {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
            return;
        }
        mutex.ok()
    };
    configure_runtime();
    tauri::Builder::default()
        .manage(Runtime {
            captures: Mutex::new(VecDeque::new()),
            enabled: AtomicBool::new(true),
        })
        .manage(desktop::DesktopState::default())
        .invoke_handler(tauri::generate_handler![
            take_captures,
            set_monitor,
            get_monitor,
            hide_window,
            capture_screen_sizes,
            minimize_window,
            exit_app,
            copy_text,
            open_url,
            paste_image,
            recognize_image,
            ocr_languages,
            load_state,
            save_state,
            export_file,
            open_downloads,
            open_data_directory,
            desktop::set_desktop_options,
            start_snip,
            desktop::get_desktop_line,
            desktop::set_desktop_line,
            desktop::desktop_line_position,
            desktop::move_desktop_line,
            desktop::desktop_photos,
            desktop::desktop_line_ready,
            desktop::close_desktop_line,
            desktop::open_desktop_photo
        ])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Focused(false))
                && window.label() == "desktop-line"
            {
                desktop::lower_to_desktop(window.app_handle());
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "desktop-line" {
                    desktop::on_native_close(window.app_handle());
                } else {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            desktop::initialize(app.handle());
            #[cfg(debug_assertions)]
            desktop::exercise_qa(app.handle().clone());
            let dir = collection_dir(app.handle()).map_err(std::io::Error::other)?;
            if let Ok(value) = std::fs::read_to_string(dir.join("monitor.json")) {
                app.state::<Runtime>()
                    .enabled
                    .store(value.trim() != "false", Ordering::Relaxed);
            }
            if std::env::args().any(|a| a == "--background") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            } else {
                reveal(app.handle());
            }
            #[cfg(debug_assertions)]
            record_qa_visibility(app.handle(), "startup");
            #[cfg(debug_assertions)]
            inject_capture_fixture(app.handle().clone());
            let show = MenuItem::with_id(app, "show", "打开截图冲洗室", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause", "暂停 / 恢复自动接收", true, None::<&str>)?;
            let desktop_line =
                MenuItem::with_id(app, "desktop", "显示 / 收起桌面晾片绳", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &desktop_line, &pause, &quit])?;
            TrayIconBuilder::new()
                .icon(tauri::image::Image::from_bytes(include_bytes!(
                    "../icons/icon.png"
                ))?)
                .tooltip("截图冲洗室 · 截图后自动接收")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => reveal(app),
                    "desktop" => {
                        let enabled =
                            desktop::get_desktop_line(app.state::<desktop::DesktopState>())
                                .map(|p| !p.enabled)
                                .unwrap_or(true);
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(error) =
                                desktop::set_desktop_line(app.clone(), enabled).await
                            {
                                reveal(&app);
                                let _ = app.emit_to("main", "desktop-line-error", error);
                            }
                        });
                    }
                    "pause" => {
                        let enabled = !app.state::<Runtime>().enabled.load(Ordering::Relaxed);
                        let _ = set_monitor(app.clone(), app.state::<Runtime>(), enabled);
                        let _ = app.emit("monitor-changed", enabled);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        reveal(tray.app_handle());
                    }
                })
                .build(app)?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let mut gate = snip::CaptureGate::new(unsafe { GetClipboardSequenceNumber() }, 0);
                let mut failures = 0;
                loop {
                    std::thread::sleep(Duration::from_millis(100));
                    let sequence = unsafe { GetClipboardSequenceNumber() };
                    let now = started.elapsed().as_millis() as u64;
                    if !handle.state::<Runtime>().enabled.load(Ordering::Relaxed) {
                        gate.reset(sequence, now);
                        failures = 0;
                        continue;
                    }
                    if !gate.ready(sequence, snip::selection_active(), now) {
                        continue;
                    }
                    match clipboard::read_image() {
                        Ok(Some(image)) => {
                            // Clipboard rendering may be delayed; re-check after encoding.
                            if unsafe { GetClipboardSequenceNumber() } != sequence
                                || snip::selection_active()
                            {
                                continue;
                            }
                            gate.consume(sequence);
                            failures = 0;
                            publish_capture(&handle, image);
                        }
                        Ok(None) => {
                            gate.consume(sequence);
                            failures = 0;
                        }
                        Err(message) => {
                            failures += 1;
                            if failures >= 4 {
                                gate.consume(sequence);
                                failures = 0;
                                let _ = handle.emit("capture-error", message);
                            }
                        }
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("截图冲洗室启动失败");
}
