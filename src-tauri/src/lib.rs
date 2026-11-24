mod config;
mod input_hook;
mod keymap;
#[cfg(target_os = "macos")]
mod keymap_macos;
mod tray;

use config::{load_settings, save_settings, set_autostart, AppSettings, MAX_SCALE, MIN_SCALE};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Size, State, WebviewWindow,
};

pub const BASE_WIDTH: f64 = 660.0;
pub const BASE_HEIGHT: f64 = 480.0;
pub const BASE_WIDTH_104: f64 = 740.0;
pub const BASE_HEIGHT_104: f64 = 520.0;
pub const SETTINGS_WIDTH: f64 = 244.0;
pub const SETTINGS_HEIGHT: f64 = 572.0;

pub struct AppState {
    settings: Arc<Mutex<AppSettings>>,
    dragging: Arc<AtomicBool>,
    hit_regions: Arc<Mutex<Vec<[f64; 4]>>>,
}

fn base_dimensions(layout: &str) -> (f64, f64) {
    if layout == "104" {
        (BASE_WIDTH_104, BASE_HEIGHT_104)
    } else {
        (BASE_WIDTH, BASE_HEIGHT)
    }
}

fn window_dimensions(layout: &str, scale: f64) -> (f64, f64) {
    let s = scale.clamp(MIN_SCALE, MAX_SCALE);
    let (w, h) = base_dimensions(layout);
    ((w * s).round(), (h * s).round())
}

// 65 layout, right-handed; the frontend reports the active regions right after boot.
fn default_regions() -> Vec<[f64; 4]> {
    vec![
        [0.392, 0.160, 0.216, 0.309],
        [0.369, 0.231, 0.262, 0.499],
        [0.508, 0.474, 0.227, 0.253],
        [0.325, 0.680, 0.428, 0.169],
        [0.248, 0.667, 0.113, 0.145],
    ]
}

fn resize_anchored_bottom_center(
    win: &WebviewWindow,
    layout: &str,
    scale: f64,
) -> Option<(i32, i32)> {
    let (new_w, new_h) = window_dimensions(layout, scale);
    let old_pos = win.outer_position().ok();
    let old_size = win.outer_size().ok();
    let sf = win.scale_factor().unwrap_or(1.0);

    let _ = win.set_size(Size::Logical(LogicalSize::new(new_w, new_h)));

    if let (Some(pos), Some(sz)) = (old_pos, old_size) {
        let new_pw = (new_w * sf).round() as i32;
        let new_ph = (new_h * sf).round() as i32;
        let mut new_x = pos.x + (sz.width as i32 - new_pw) / 2;
        let mut new_y = pos.y + sz.height as i32 - new_ph;
        if let Ok(Some(monitor)) = win.current_monitor() {
            let mx = monitor.position().x;
            let my = monitor.position().y;
            let mw = monitor.size().width as i32;
            let mh = monitor.size().height as i32;
            new_x = new_x.clamp(mx, (mx + mw - new_pw).max(mx));
            new_y = new_y.clamp(my, (my + mh - new_ph).max(my));
        }
        let _ = win.set_position(PhysicalPosition::new(new_x, new_y));
        Some((new_x, new_y))
    } else {
        None
    }
}

fn bottom_right_position(app: &AppHandle, layout: &str, scale: f64, sf: f64) -> PhysicalPosition<i32> {
    let (lw, lh) = window_dimensions(layout, scale);
    let w = (lw * sf).round() as i32;
    let h = (lh * sf).round() as i32;
    let pad_x = (28.0 * sf).round() as i32;
    let pad_y = (64.0 * sf).round() as i32;

    if let Ok(Some(monitor)) = app.primary_monitor() {
        let size = monitor.size();
        let pos = monitor.position();
        return PhysicalPosition::new(
            pos.x + size.width as i32 - w - pad_x,
            pos.y + size.height as i32 - h - pad_y,
        );
    }
    PhysicalPosition::new(pad_x, pad_y)
}

fn clamp_to_primary(app: &AppHandle, x: i32, y: i32, w: i32, h: i32) -> PhysicalPosition<i32> {
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let mx = monitor.position().x;
        let my = monitor.position().y;
        let mw = monitor.size().width as i32;
        let mh = monitor.size().height as i32;
        return PhysicalPosition::new(
            x.clamp(mx, (mx + mw - w).max(mx)),
            y.clamp(my, (my + mh - h).max(my)),
        );
    }
    PhysicalPosition::new(x, y)
}

#[cfg(windows)]
fn cursor_pos() -> Option<(i32, i32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut pt) != 0 {
            Some((pt.x, pt.y))
        } else {
            None
        }
    }
}

#[cfg(windows)]
fn cursor_pos_for(_win: &WebviewWindow) -> Option<(f64, f64)> {
    cursor_pos().map(|(x, y)| (x as f64, y as f64))
}

// NSEvent reports points; Tauri converts to the physical pixels used by the
// window position and size, so hit-test ratios match the Windows path.
#[cfg(target_os = "macos")]
fn cursor_pos_for(win: &WebviewWindow) -> Option<(f64, f64)> {
    win.cursor_position().ok().map(|p| (p.x, p.y))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn cursor_pos_for(_win: &WebviewWindow) -> Option<(f64, f64)> {
    None
}

fn start_hit_test_loop(
    app: AppHandle,
    settings: Arc<Mutex<AppSettings>>,
    dragging: Arc<AtomicBool>,
    regions: Arc<Mutex<Vec<[f64; 4]>>>,
) {
    thread::spawn(move || {
        let mut last_ignore: Option<bool> = None;

        loop {
            thread::sleep(Duration::from_millis(25));

            let Some(win) = app.get_webview_window("main") else {
                continue;
            };
            if !win.is_visible().unwrap_or(false) {
                continue;
            }
            let Some((cx, cy)) = cursor_pos_for(&win) else {
                continue;
            };
            let Ok(pos) = win.outer_position() else {
                continue;
            };
            let Ok(size) = win.outer_size() else {
                continue;
            };

            let w = size.width.max(1) as f64;
            let h = size.height.max(1) as f64;
            let rx = (cx - pos.x as f64) / w;
            let ry = (cy - pos.y as f64) / h;
            let inside = rx >= 0.0 && rx <= 1.0 && ry >= 0.0 && ry <= 1.0;
            let forced = dragging.load(Ordering::Relaxed);

            let locked = settings
                .lock()
                .map(|s| s.click_through_locked)
                .unwrap_or(false);
            let effective_locked = locked && !input_hook::is_alt_pressed();

            let over = if forced {
                true
            } else if effective_locked || !inside {
                false
            } else {
                regions
                    .lock()
                    .map(|list| {
                        list.iter().any(|[x, y, rw, rh]| {
                            rx >= *x && rx <= x + rw && ry >= *y && ry <= y + rh
                        })
                    })
                    .unwrap_or(true)
            };

            let should_ignore = !over;
            if last_ignore != Some(should_ignore) {
                let _ = win.set_ignore_cursor_events(should_ignore);
                last_ignore = Some(should_ignore);
            }
        }
    });
}

fn start_position_watcher(app: AppHandle, settings: Arc<Mutex<AppSettings>>) {
    thread::spawn(move || {
        let mut last: Option<(i32, i32)> = None;
        loop {
            thread::sleep(Duration::from_millis(700));
            let Some(win) = app.get_webview_window("main") else {
                continue;
            };
            if !win.is_visible().unwrap_or(true) {
                continue;
            }
            let Ok(pos) = win.outer_position() else {
                continue;
            };
            if last == Some((pos.x, pos.y)) {
                continue;
            }
            last = Some((pos.x, pos.y));
            let mut s = settings.lock().unwrap();
            if s.x != Some(pos.x) || s.y != Some(pos.y) {
                s.x = Some(pos.x);
                s.y = Some(pos.y);
                save_settings(&s);
            }
        }
    });
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    mut patch: AppSettings,
) -> AppSettings {
    let mut s = state.settings.lock().unwrap();
    let prev_scale = s.scale;
    let prev_layout = s.keyboard_layout.clone();
    let prev_autostart = s.autostart;

    patch.normalize();
    s.scale = patch.scale;
    s.always_on_top = patch.always_on_top;
    s.click_through_locked = patch.click_through_locked;
    s.purr_pet = patch.purr_pet;
    s.purr_sleep = patch.purr_sleep;
    s.autostart = patch.autostart;
    s.privacy_mode = patch.privacy_mode;
    s.left_mouse = patch.left_mouse;
    s.coat = patch.coat;
    s.sleep_minutes = patch.sleep_minutes;
    s.show_key_labels = patch.show_key_labels;
    s.keyboard_layout = patch.keyboard_layout;

    if prev_autostart != s.autostart {
        let _ = set_autostart(s.autostart);
    }

    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_always_on_top(s.always_on_top);
        if (s.scale - prev_scale).abs() > 0.0005 || s.keyboard_layout != prev_layout {
            if let Some((x, y)) = resize_anchored_bottom_center(&win, &s.keyboard_layout, s.scale) {
                s.x = Some(x);
                s.y = Some(y);
            }
        }
    }

    save_settings(&s);
    let out = s.clone();
    drop(s);

    let _ = app.emit("cat-settings", out.clone());
    out
}

#[tauri::command]
fn set_dragging(state: State<'_, AppState>, dragging: bool) {
    state.dragging.store(dragging, Ordering::Relaxed);
}

fn place_settings_window(app: &AppHandle, win: &WebviewWindow) {
    let Some(pet) = app.get_webview_window("main") else {
        return;
    };
    let (Ok(pet_pos), Ok(pet_size)) = (pet.outer_position(), pet.outer_size()) else {
        return;
    };
    let sf = pet.scale_factor().unwrap_or(1.0);
    let sw = (SETTINGS_WIDTH * sf).round() as i32;
    let sh = (SETTINGS_HEIGHT * sf).round() as i32;
    let gap = (4.0 * sf).round() as i32;
    let monitor = pet
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let (ax, ay, aw, ah) = if let Some(m) = monitor {
        let wa = m.work_area();
        (
            wa.position.x,
            wa.position.y,
            wa.size.width as i32,
            wa.size.height as i32,
        )
    } else {
        (0, 0, 1920, 1080)
    };
    // Anchor to the visible cat/keyboard/mouse bounds instead of the window edge:
    // both layouts leave a wide transparent margin around the drawing.
    let regions = app
        .state::<AppState>()
        .hit_regions
        .lock()
        .map(|list| list.clone())
        .unwrap_or_default();
    let (nx0, nx1, ny1) = if regions.is_empty() {
        (0.0, 1.0, 1.0)
    } else {
        let mut nx0 = 1.0f64;
        let mut nx1 = 0.0f64;
        let mut ny1 = 0.0f64;
        for [x, y, w, h] in &regions {
            nx0 = nx0.min(*x);
            nx1 = nx1.max(x + w);
            ny1 = ny1.max(y + h);
        }
        (nx0, nx1, ny1)
    };
    let pet_w = pet_size.width as f64;
    let pet_h = pet_size.height as f64;
    let content_left = pet_pos.x as f64 + nx0 * pet_w;
    let content_right = pet_pos.x as f64 + nx1 * pet_w;
    let content_bottom = pet_pos.y as f64 + ny1 * pet_h;

    let mut x = content_right + gap as f64;
    if x + sw as f64 > (ax + aw) as f64 {
        x = content_left - gap as f64 - sw as f64;
    }
    let x = (x.round() as i32).clamp(ax, (ax + aw - sw).max(ax));
    let y = ((content_bottom - sh as f64).round() as i32).clamp(ay, (ay + ah - sh).max(ay));
    let _ = win.set_position(PhysicalPosition::new(x, y));
}

// Webview windows must not be created inside an IPC/tray callback on the main
// thread: the inline creation deadlocks WebView2 initialization.
static SETTINGS_CREATE_LOCK: Mutex<()> = Mutex::new(());

fn point_in_rect(pos: PhysicalPosition<i32>, size: tauri::PhysicalSize<u32>, x: f64, y: f64) -> bool {
    x >= pos.x as f64
        && x <= (pos.x + size.width as i32) as f64
        && y >= pos.y as f64
        && y <= (pos.y + size.height as i32) as f64
}

pub(crate) fn close_settings_if_clicked_outside(app: &AppHandle) {
    let Some(settings) = app.get_webview_window("settings") else {
        return;
    };
    if !settings.is_visible().unwrap_or(false) {
        return;
    }
    let Some((cx, cy)) = cursor_pos_for(&settings) else {
        return;
    };
    let (Ok(pos), Ok(size)) = (settings.outer_position(), settings.outer_size()) else {
        return;
    };
    if point_in_rect(pos, size, cx, cy) {
        return;
    }
    if let Some(pet) = app.get_webview_window("main") {
        if pet.is_visible().unwrap_or(false) {
            if let (Ok(ppos), Ok(psize)) = (pet.outer_position(), pet.outer_size()) {
                if point_in_rect(ppos, psize, cx, cy) {
                    let _ = settings.hide();
                    return;
                }
            }
        }
    }
    // Native <select> popups live outside the window but keep it focused, so
    // wait for the click to settle and only close if focus actually moved.
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(160));
        if let Some(settings) = app.get_webview_window("settings") {
            if settings.is_visible().unwrap_or(false)
                && !settings.is_focused().unwrap_or(true)
            {
                let _ = settings.hide();
            }
        }
    });
}

pub(crate) fn show_settings_window(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        let Ok(_guard) = SETTINGS_CREATE_LOCK.try_lock() else {
            return;
        };
        show_settings_window_blocking(&app);
    });
}

fn show_settings_window_blocking(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        // Re-assert the logical size while visible: a webview created hidden can
        // keep a stale viewport on Windows until the next resize event, which used
        // to leave the bottom of the menu (quit button) outside the window.
        let _ = win.set_size(Size::Logical(LogicalSize::new(SETTINGS_WIDTH, SETTINGS_HEIGHT)));
        place_settings_window(app, &win);
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let built = tauri::WebviewWindowBuilder::new(
        app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("7paws 设置")
    .inner_size(SETTINGS_WIDTH, SETTINGS_HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    // Must match the main window: WebView2 rejects a second webview that
    // requests different browser arguments against the shared environment.
    .additional_browser_args("--autoplay-policy=no-user-gesture-required")
    .visible(false)
    .build();
    match built {
        Ok(win) => {
            let _ = win.show();
            let _ = win.set_size(Size::Logical(LogicalSize::new(SETTINGS_WIDTH, SETTINGS_HEIGHT)));
            place_settings_window(app, &win);
            let _ = win.set_focus();
        }
        Err(e) => eprintln!("failed to open settings window: {e}"),
    }
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    show_settings_window(&app);
}

#[tauri::command]
fn set_hit_regions(state: State<'_, AppState>, regions: Vec<[f64; 4]>) {
    let mut sanitized: Vec<[f64; 4]> = Vec::new();
    for rect in regions.into_iter().take(8) {
        let x = rect[0].clamp(0.0, 1.0);
        let y = rect[1].clamp(0.0, 1.0);
        let w = rect[2].clamp(0.0, 1.0 - x);
        let h = rect[3].clamp(0.0, 1.0 - y);
        if w > 0.001 && h > 0.001 {
            sanitized.push([x, y, w, h]);
        }
    }
    if sanitized.is_empty() {
        sanitized = default_regions();
    }
    *state.hit_regions.lock().unwrap() = sanitized;
}

#[tauri::command]
fn reset_position(app: AppHandle, state: State<'_, AppState>) {
    let mut s = state.settings.lock().unwrap();
    if let Some(win) = app.get_webview_window("main") {
        let sf = win.scale_factor().unwrap_or(1.0);
        let pos = bottom_right_position(&app, &s.keyboard_layout, s.scale, sf);
        let _ = win.set_position(pos);
        s.x = Some(pos.x);
        s.y = Some(pos.y);
        save_settings(&s);
        let out = s.clone();
        drop(s);
        let _ = app.emit("cat-settings", out);
    }
}

#[tauri::command]
fn get_app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let initial = load_settings();
    let settings = Arc::new(Mutex::new(initial.clone()));
    let dragging = Arc::new(AtomicBool::new(false));
    let hit_regions = Arc::new(Mutex::new(default_regions()));

    let settings_setup = settings.clone();
    let drag_setup = dragging.clone();
    let regions_setup = hit_regions.clone();

    tauri::Builder::default()
        .manage(AppState {
            settings: settings.clone(),
            dragging,
            hit_regions,
        })
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            {
                // Tray-only pet: no Dock icon, no app menu bar.
                let _ = app
                    .handle()
                    .set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            let handle = app.handle().clone();
            let s = settings_setup.lock().unwrap().clone();

            if s.autostart {
                // Re-register under the new name after the 7paws rename.
                let _ = set_autostart(true);
            }

            if let Some(win) = app.get_webview_window("main") {
                let (w, h) = window_dimensions(&s.keyboard_layout, s.scale);
                let _ = win.set_size(Size::Logical(LogicalSize::new(w, h)));
                let _ = win.set_always_on_top(s.always_on_top);

                let sf = win.scale_factor().unwrap_or(1.0);
                let pw = (w * sf).round() as i32;
                let ph = (h * sf).round() as i32;
                let pos = if let (Some(x), Some(y)) = (s.x, s.y) {
                    clamp_to_primary(&handle, x, y, pw, ph)
                } else {
                    bottom_right_position(&handle, &s.keyboard_layout, s.scale, sf)
                };
                let _ = win.set_position(pos);
            }

            tray::setup_tray(&handle, settings_setup.clone())?;
            input_hook::start_global_hooks(handle.clone(), settings_setup.clone());
            start_hit_test_loop(
                handle.clone(),
                settings_setup.clone(),
                drag_setup.clone(),
                regions_setup.clone(),
            );
            start_position_watcher(handle.clone(), settings_setup.clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            update_settings,
            open_settings,
            set_dragging,
            set_hit_regions,
            reset_position,
            get_app_version,
            quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running 7paws");
}
