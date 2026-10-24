use serde::{Deserialize, Serialize};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
#[cfg(debug_assertions)]
use windows::Win32::Foundation::RECT;
use windows::Win32::{
    Foundation::{HWND, LPARAM},
    UI::WindowsAndMessaging::*,
};
use windows::core::w;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub enabled: bool,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub auto_mount: bool,
    pub photo_limit: usize,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: false,
            x: None,
            y: None,
            auto_mount: true,
            photo_limit: 6,
        }
    }
}
#[derive(Default)]
pub struct DesktopState {
    preferences: Mutex<Preferences>,
    enabled: AtomicBool,
    ready: AtomicBool,
}

fn write_preferences(app: &tauri::AppHandle, preferences: &Preferences) -> Result<(), String> {
    let dir = super::collection_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join("desktop.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec(preferences).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(tmp, dir.join("desktop.json")).map_err(|e| e.to_string())
}

fn create_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("desktop-line") {
        return Ok(window);
    }
    app.state::<DesktopState>()
        .ready
        .store(false, Ordering::Release);
    // Keep a normal top-level WebView2 window. Changing it to WS_CHILD after
    // creation breaks composition and makes Tao's window state inconsistent.
    let builder =
        WebviewWindowBuilder::new(app, "desktop-line", WebviewUrl::App("desktop.html".into()))
            .title("桌面晾片绳 · Darkroom")
            .inner_size(880.0, 252.0)
            .visible(false)
            .focused(false)
            .focusable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .skip_taskbar(true)
            .background_color(tauri::window::Color(0, 0, 0, 0))
            .disable_drag_drop_handler();
    let window = builder.build().map_err(|e| e.to_string())?;
    position_window(app, &window)?;
    Ok(window)
}

fn position_window(app: &tauri::AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    let monitor = window
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("找不到显示器")?;
    let scale = monitor.scale_factor();
    let width = 880.0_f64
        .min(monitor.size().width as f64 / scale - 40.0)
        .max(320.0);
    window
        .set_size(tauri::LogicalSize::new(width, 252.0))
        .map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let preferences = get_desktop_line(app.state::<DesktopState>())?;
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let saved = preferences.x.zip(preferences.y).filter(|&(x, y)| {
        monitors.iter().any(|m| {
            x as i64 + size.width as i64 > m.position().x as i64 + 100
                && (x as i64) < m.position().x as i64 + m.size().width as i64 - 100
                && y >= m.position().y
                && (y as i64) < m.position().y as i64 + m.size().height as i64 - 80
        })
    });
    let (x, y) = saved.unwrap_or((
        monitor.position().x + (monitor.size().width as i32 - size.width as i32) / 2,
        monitor.position().y
            + (monitor.size().height as i32 - size.height as i32 - (64.0 * scale) as i32).max(0),
    ));
    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

fn show_if_ready(app: &tauri::AppHandle) -> Result<(), String> {
    let app = app.clone();
    let ui_app = app.clone();
    app.run_on_main_thread(move || {
        #[cfg(debug_assertions)]
        if std::env::var_os("DARKROOM_QA_HIDE_DESKTOP").is_some() {
            record_qa(&ui_app);
            return;
        }
        let state = ui_app.state::<DesktopState>();
        if state.enabled.load(Ordering::Acquire) && state.ready.load(Ordering::Acquire) {
            if let Some(window) = ui_app.get_webview_window("desktop-line") {
                let _ = window.show();
                lower_to_desktop(&ui_app);
            }
        }
        #[cfg(debug_assertions)]
        record_qa(&ui_app);
    })
    .map_err(|e| e.to_string())
}

pub fn initialize(app: &tauri::AppHandle) {
    let mut preferences = super::collection_dir(app)
        .ok()
        .and_then(|dir| std::fs::read(dir.join("desktop.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<Preferences>(&bytes).ok())
        .unwrap_or_default();
    if std::env::args().any(|arg| arg == "--desktop-line") {
        preferences.enabled = true;
        let _ = write_preferences(app, &preferences);
    }
    let enabled = preferences.enabled;
    *app.state::<DesktopState>().preferences.lock().unwrap() = preferences;
    app.state::<DesktopState>()
        .enabled
        .store(enabled, Ordering::Release);
    if enabled {
        if let Err(error) = create_window(app) {
            app.state::<DesktopState>()
                .enabled
                .store(false, Ordering::Release);
            if let Ok(mut preferences) = app.state::<DesktopState>().preferences.lock() {
                preferences.enabled = false;
            }
            let _ = app.emit_to("main", "desktop-line-error", error);
        }
    }
    restore_on_desktop(app.clone());
}

// Show Desktop can hide ordinary tool windows. Restore only our enabled widget
// when the desktop receives focus, keeping it below normal application windows.
fn restore_on_desktop(app: tauri::AppHandle) {
    #[cfg(debug_assertions)]
    if std::env::var_os("DARKROOM_QA_HIDE_DESKTOP").is_some() {
        return;
    }
    std::thread::spawn(move || {
        let mut was_desktop = false;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(400));
            let is_desktop = unsafe {
                let mut class = [0u16; 128];
                let length = GetClassNameW(GetForegroundWindow(), &mut class).max(0) as usize;
                matches!(
                    String::from_utf16_lossy(&class[..length]).as_str(),
                    "Progman" | "WorkerW" | "SHELLDLL_DefView"
                )
            };
            if is_desktop && !was_desktop {
                let ui_app = app.clone();
                let _ = app.run_on_main_thread(move || {
                    let state = ui_app.state::<DesktopState>();
                    if state.enabled.load(Ordering::Acquire) && state.ready.load(Ordering::Acquire)
                    {
                        if let Some(window) = ui_app.get_webview_window("desktop-line") {
                            if let Ok(hwnd) = window.hwnd() {
                                unsafe {
                                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                                }
                            }
                            lower_to_desktop(&ui_app);
                        }
                    }
                });
            }
            was_desktop = is_desktop;
        }
    });
}

unsafe extern "system" fn find_desktop_host(window: HWND, output: LPARAM) -> windows::core::BOOL {
    unsafe {
        if FindWindowExW(Some(window), None, w!("SHELLDLL_DefView"), None).is_ok() {
            *(output.0 as *mut HWND) = window;
            return false.into();
        }
    }
    true.into()
}

// HWND_BOTTOM can put a top-level widget behind Explorer's wallpaper itself.
// Insert immediately above the desktop instead, leaving all normal apps above it.
pub fn lower_to_desktop(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("desktop-line") {
        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                let mut desktop = HWND::default();
                let _ = EnumWindows(
                    Some(find_desktop_host),
                    LPARAM(&mut desktop as *mut HWND as isize),
                );
                if desktop.0.is_null() {
                    return;
                }
                let mut anchor = GetWindow(desktop, GW_HWNDPREV).unwrap_or(HWND_NOTOPMOST);
                if anchor == hwnd {
                    return;
                }
                if GetWindowLongPtrW(anchor, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize != 0 {
                    anchor = HWND_NOTOPMOST;
                }
                let _ = SetWindowPos(
                    hwnd,
                    Some(anchor),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }
}

#[tauri::command]
pub fn get_desktop_line(state: State<DesktopState>) -> Result<Preferences, String> {
    state
        .preferences
        .lock()
        .map(|p| p.clone())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_desktop_options(
    app: tauri::AppHandle,
    auto_mount: bool,
    photo_limit: usize,
) -> Result<Preferences, String> {
    if !(3..=8).contains(&photo_limit) {
        return Err("展示数量应在 3 到 8 张之间。".into());
    }
    let next = {
        let state = app.state::<DesktopState>();
        let mut preferences = state.preferences.lock().map_err(|e| e.to_string())?;
        let mut next = preferences.clone();
        next.auto_mount = auto_mount;
        next.photo_limit = photo_limit;
        write_preferences(&app, &next)?;
        *preferences = next.clone();
        next
    };
    let _ = app.emit("desktop-line-changed", &next);
    let _ = app.emit_to("desktop-line", "desktop-photos-changed", ());
    Ok(next)
}

// Async commands run off the event loop. No settings mutex is held while
// creating, showing, moving or closing a native window.
#[tauri::command]
pub async fn set_desktop_line(app: tauri::AppHandle, enabled: bool) -> Result<Preferences, String> {
    let save_error = {
        let state = app.state::<DesktopState>();
        let mut preferences = state.preferences.lock().map_err(|e| e.to_string())?;
        preferences.enabled = enabled;
        state.enabled.store(enabled, Ordering::Release);
        // Closing remains effective even if settings cannot be written.
        let error = write_preferences(&app, &preferences).err();
        if error.is_some() {
            preferences.enabled = false;
            state.enabled.store(false, Ordering::Release);
        }
        error
    };
    if let Some(error) = save_error {
        if let Some(window) = app.get_webview_window("desktop-line") {
            let _ = window.hide();
        }
        return Err(error);
    }
    if enabled {
        if let Err(error) = create_window(&app) {
            let state = app.state::<DesktopState>();
            state.enabled.store(false, Ordering::Release);
            let mut preferences = state.preferences.lock().map_err(|e| e.to_string())?;
            preferences.enabled = false;
            let _ = write_preferences(&app, &preferences);
            let _ = app.emit("desktop-line-changed", &*preferences);
            return Err(error);
        }
        show_if_ready(&app)?;
    } else if let Some(window) = app.get_webview_window("desktop-line") {
        window.hide().map_err(|e| e.to_string())?;
    }
    let preferences = get_desktop_line(app.state::<DesktopState>())?;
    let _ = app.emit("desktop-line-changed", &preferences);
    #[cfg(debug_assertions)]
    record_qa(&app);
    Ok(preferences)
}

pub fn on_native_close(app: &tauri::AppHandle) {
    let state = app.state::<DesktopState>();
    state.enabled.store(false, Ordering::Release);
    state.ready.store(false, Ordering::Release);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = set_desktop_line(app.clone(), false).await {
            let _ = app.emit_to("main", "desktop-line-error", error);
        }
    });
}

#[tauri::command]
pub async fn close_desktop_line(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("desktop-line") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn desktop_line_ready(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    layout: serde_json::Value,
) -> Result<(), String> {
    if window.label() != "desktop-line" {
        return Err("不是桌面挂件窗口".into());
    }
    #[cfg(debug_assertions)]
    if let Some(dir) = std::env::var_os("DARKROOM_QA_DIR") {
        let _ = std::fs::write(
            std::path::PathBuf::from(dir).join("desktop-layout.json"),
            layout.to_string(),
        );
    }
    let _ = layout;
    app.state::<DesktopState>()
        .ready
        .store(true, Ordering::Release);
    show_if_ready(&app)
}

#[tauri::command]
pub fn desktop_line_position(app: tauri::AppHandle) -> Result<(i32, i32, f64), String> {
    let window = app
        .get_webview_window("desktop-line")
        .ok_or("找不到桌面晾片绳")?;
    let position = window.outer_position().map_err(|e| e.to_string())?;
    Ok((
        position.x,
        position.y,
        window.scale_factor().map_err(|e| e.to_string())?,
    ))
}

#[tauri::command]
pub async fn move_desktop_line(
    app: tauri::AppHandle,
    x: i32,
    y: i32,
    save: bool,
) -> Result<(), String> {
    let window = app
        .get_webview_window("desktop-line")
        .ok_or("找不到桌面晾片绳")?;
    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    if save {
        let state = app.state::<DesktopState>();
        let mut preferences = state.preferences.lock().map_err(|e| e.to_string())?;
        preferences.x = Some(x);
        preferences.y = Some(y);
        write_preferences(&app, &preferences)?;
    }
    Ok(())
}

#[tauri::command]
pub fn desktop_photos(app: tauri::AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let limit = get_desktop_line(app.state::<DesktopState>())?
        .photo_limit
        .clamp(3, 8);
    Ok(super::load_state(app)?
        .and_then(|data| {
            data.get("photos")
                .and_then(|p| p.as_array())
                .map(|photos| ordered_photos(photos, limit))
        })
        .unwrap_or_default())
}
fn ordered_photos(photos: &[serde_json::Value], limit: usize) -> Vec<serde_json::Value> {
    let mut selected: Vec<_> = photos.iter().collect();
    selected.sort_by_key(|p| {
        (
            !p.get("pinned").and_then(|v| v.as_bool()).unwrap_or(false),
            std::cmp::Reverse(p.get("createdAt").and_then(|v| v.as_i64()).unwrap_or(0)),
        )
    });
    selected.into_iter().take(limit).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_preferences_keep_automatic_mount_and_six_photos() {
        let value: Preferences =
            serde_json::from_str(r#"{"enabled":false,"x":100,"y":200}"#).unwrap();
        assert!(value.auto_mount);
        assert_eq!(value.photo_limit, 6);
        assert_eq!(value.x, Some(100));
    }
    #[test]
    fn pinned_references_survive_newer_screenshots() {
        let photos = serde_json::json!([{"id":"new","createdAt":30},{"id":"old-pin","createdAt":1,"pinned":true},{"id":"mid","createdAt":20},{"id":"new-pin","createdAt":10,"pinned":true}]);
        let chosen = ordered_photos(photos.as_array().unwrap(), 3);
        assert_eq!(
            chosen
                .iter()
                .map(|p| p["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["new-pin", "old-pin", "new"]
        );
    }
}

#[tauri::command]
pub fn open_desktop_photo(app: tauri::AppHandle, id: Option<String>) {
    super::reveal(&app);
    if let Some(id) = id {
        let _ = app.emit_to("main", "open-photo", id);
    }
}

#[cfg(debug_assertions)]
fn record_qa(app: &tauri::AppHandle) {
    if let Some(dir) = std::env::var_os("DARKROOM_QA_DIR") {
        if let Some(window) = app.get_webview_window("desktop-line") {
            unsafe {
                if let Ok(hwnd) = window.hwnd() {
                    let mut class = [0u16; 128];
                    let parent = GetWindow(hwnd, GW_OWNER).ok();
                    let length = parent
                        .map(|p| GetClassNameW(p, &mut class))
                        .unwrap_or(0)
                        .max(0) as usize;
                    let mut bounds = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut bounds);
                    let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                    let main = app.get_webview_window("main");
                    let data = serde_json::json!({"noActivate":GetWindowLongPtrW(hwnd,GWL_EXSTYLE)&WS_EX_NOACTIVATE.0 as isize!=0,"mainVisible":main.as_ref().is_some_and(|w|w.is_visible().unwrap_or(true)),"mainFullscreen":main.as_ref().is_some_and(|w|w.is_fullscreen().unwrap_or(true)),"visible":window.is_visible().unwrap_or(false),"rendererReady":app.state::<DesktopState>().ready.load(Ordering::Acquire),"ownerClass":String::from_utf16_lossy(&class[..length]),"childStyle":style&WS_CHILD.0 as isize!=0,"captionStyle":style&WS_CAPTION.0 as isize!=0,"bounds":[bounds.left,bounds.top,bounds.right,bounds.bottom]});
                    let _ = std::fs::write(
                        std::path::PathBuf::from(dir).join("desktop-report.json"),
                        data.to_string(),
                    );
                }
            }
        }
    }
}

#[cfg(debug_assertions)]
pub fn exercise_qa(app: tauri::AppHandle) {
    if !std::env::args().any(|arg| arg == "--desktop-self-test") {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let result=async {
            for _ in 0..60 {if app.state::<DesktopState>().ready.load(Ordering::Acquire){break;}std::thread::sleep(std::time::Duration::from_millis(100));}
            let window=app.get_webview_window("desktop-line").ok_or("missing widget")?;
            let before=desktop_line_position(app.clone())?;
            set_desktop_line(app.clone(),false).await?;
            let hidden=!window.is_visible().map_err(|e|e.to_string())?;
            set_desktop_line(app.clone(),true).await?;
            std::thread::sleep(std::time::Duration::from_millis(200));
            let shown=window.is_visible().map_err(|e|e.to_string())?;
            move_desktop_line(app.clone(),before.0+40,before.1-30,true).await?;
            let after=desktop_line_position(app.clone())?;
            window.close().map_err(|e|e.to_string())?;
            for _ in 0..30 {if app.get_webview_window("desktop-line").is_none(){break;}std::thread::sleep(std::time::Duration::from_millis(100));}
            let native_close=app.get_webview_window("desktop-line").is_none();
            for _ in 0..20 {if !get_desktop_line(app.state::<DesktopState>())?.enabled{break;}std::thread::sleep(std::time::Duration::from_millis(50));}
            let disabled=!get_desktop_line(app.state::<DesktopState>())?.enabled;
            set_desktop_line(app.clone(),true).await?;
            for _ in 0..60 {if app.state::<DesktopState>().ready.load(Ordering::Acquire){break;}std::thread::sleep(std::time::Duration::from_millis(100));}
            let restored=app.get_webview_window("desktop-line").is_some_and(|w|w.is_visible().unwrap_or(false));
            let saved=get_desktop_line(app.state::<DesktopState>())?;
            record_qa(&app);
            Ok::<_,String>(serde_json::json!({"hiddenWhenDisabled":hidden,"visibleWhenEnabled":shown,"movedByExpectedPixels":after.0==before.0+40&&after.1==before.1-30,"positionSaved":saved.x==Some(after.0)&&saved.y==Some(after.1),"nativeCloseDestroysWindow":native_close,"nativeCloseDisablesSetting":disabled,"canRecreateAfterClose":restored,"rendererReady":app.state::<DesktopState>().ready.load(Ordering::Acquire)}))
        }.await;
        if let Some(dir) = std::env::var_os("DARKROOM_QA_DIR") {
            let _ = std::fs::write(
                std::path::PathBuf::from(dir).join("desktop-exercise.json"),
                serde_json::to_vec_pretty(&result).unwrap(),
            );
        }
    });
}
