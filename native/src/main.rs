//! LSP Converter's native shell (Tauri + WebView2), forked from ACECM's: it
//! owns the window and the UI's HTTP port on this PC (server.rs).
// ⚠ A windowed program in EVERY build, debug included. As a console
// program, each launch from Explorer opened a terminal window next to the
// app. --headless, which prints, attaches to the console it was started from.
#![windows_subsystem = "windows"]


use acecm::{paths, procs::open_url, server};
use serde_json::Value;
use std::time::{Duration, Instant};

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    #[cfg(windows)]
    message_box(msg);
    std::process::exit(1);
}

#[cfg(windows)]
fn message_box(msg: &str) {
    // a GUI-subsystem exe has no console to print to
    let wide: Vec<u16> = msg.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = acecm::edition::NAME.encode_utf16().chain(Some(0)).collect();
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            std::ptr::null_mut(), wide.as_ptr(), title.as_ptr(), 0x10);
    }
}

/// The window's icons, both sizes, straight from this exe. ⚠ Tauri sets
/// only the small one (the title bar): the taskbar and Alt+Tab use the big
/// one, and without it Windows falls back to its cached copy of the exe's
/// icon, which stays stale after the icon changes. Loaded at the sizes the
/// system asks for, so they're sharp at any display scale.
#[cfg(windows)]
fn set_window_icons(win: &tauri::WebviewWindow) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ExtractIconExW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, ICON_BIG, ICON_SMALL, WM_SETICON};
    let (Ok(hwnd), Ok(exe)) = (win.hwnd(), std::env::current_exe()) else { return };
    let path: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let (mut big, mut small) = (std::ptr::null_mut(), std::ptr::null_mut());
    // negative = by resource id: tauri-build embeds the app icon as 32512
    // (tauri_utils::platform::WINDOWS_APP_ICON_RESOURCE_ID)
    if unsafe { ExtractIconExW(path.as_ptr(), -32512, &mut big, &mut small, 1) } == 0 {
        return;
    }
    let hwnd = hwnd.0 as windows_sys::Win32::Foundation::HWND;
    unsafe {
        if !big.is_null() {
            SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, big as isize);
        }
        if !small.is_null() {
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small as isize);
        }
    }
}

/// --headless prints: a GUI-subsystem exe borrows the console it was
/// started from.
fn attach_console() {
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(u32::MAX);
    }
}

fn http(timeout: f64) -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder().no_proxy().timeout(Duration::from_secs_f64(timeout))
        .build().unwrap_or_else(|_| reqwest::blocking::Client::new())
}

/// Is the thing on this port one of ours? (ACECM answers with its own
/// name, so it is not mistaken for us.)
fn ours_answering(port: u16) -> bool {
    http(2.5).get(format!("http://127.0.0.1:{port}/api/version")).send().ok()
        .and_then(|r| r.json::<Value>().ok())
        .map(|v| v["name"] == acecm::edition::NAME).unwrap_or(false)
}

/// Ask the instance on the port to show its window. False if it has none.
fn raise_window(port: u16) -> bool {
    http(5.0).post(format!("http://127.0.0.1:{port}/api/app/show"))
        .header("Content-Type", "application/json").body("{}").send().ok()
        .and_then(|r| r.json::<Value>().ok())
        .map(|v| v["ok"] == true).unwrap_or(false)
}

/// Bind the UI port. ⚠ A copy that is just closing may hold it for a moment,
/// so something that isn't one of ours gets a few seconds to let go.
fn bind(host: &str, port: u16) -> Result<std::net::TcpListener, String> {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        match std::net::TcpListener::bind((host, port)) {
            Ok(l) => return Ok(l),
            Err(e) => {
                if ours_answering(port) || Instant::now() >= deadline {
                    return Err(e.to_string());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Mode { Window, Browser, Headless }

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let mode = if has("--browser") { Mode::Browser }
        else if has("--headless") || has("--no-ui") { Mode::Headless } else { Mode::Window };
    if mode == Mode::Headless {
        attach_console();
    }

    let listen = paths::listen();
    let std_listener = match bind(&listen.host, listen.port) {
        Ok(l) => l,
        Err(e) => {
            if !ours_answering(listen.port) {
                fail(&format!("Port {0} is in use by something that is not LSP Converter, so the app cannot start.\n\n{e}\n\nClose whatever is using port {0}, or change ui_port in LSP Converter's settings file:\n{1}",
                    listen.port, paths::root().join("config.json").display()));
            }
            if raise_window(listen.port) {
                acecm::logs::info("LSP Converter already running; raised its window");
                return;
            }
            if mode == Mode::Browser {
                open_url(&format!("http://127.0.0.1:{}", listen.port));
            }
            if mode != Mode::Window {
                return;
            }
            fail("LSP Converter is already running without a window (started with --headless or --browser).\n\nClose that copy, then start LSP Converter again.");
        }
    };
    std_listener.set_nonblocking(true).ok();
    // the UI and converter this exe carries (release builds)
    acecm::bundle::ensure();
    // fill the Convert page's car list cache once the window is up,
    // so the first visit is as quick as the second
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let _ = acecm::convert::cars();
    });

    let url = format!("http://127.0.0.1:{}/", listen.port);
    if mode != Mode::Window {
        // no window: serve until the process is stopped
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        acecm::api::set_show(|| false);
        rt.block_on(async move {
            let l = tokio::net::TcpListener::from_std(std_listener).expect("listener");
            if mode == Mode::Browser {
                open_url(&url);
            } else {
                println!("LSP Converter (headless)\n  {url}");
            }
            server::serve(l).await;
        });
        return;
    }
    tauri::Builder::default()
        .setup(move |app| {
            tauri::async_runtime::spawn(async move {
                let l = tokio::net::TcpListener::from_std(std_listener)
                    .expect("listener");
                server::serve(l).await;
            });
            let h = app.handle().clone();
            acecm::api::set_show(move || match h.get_webview_window("main") {
                Some(w) => {
                    let _ = w.unminimize();
                    let _ = w.show();
                    let _ = w.set_focus();
                    true
                }
                None => false,
            });
            WebviewWindowBuilder::new(app, "main",
                                      WebviewUrl::External(url.parse().unwrap()))
                .title(acecm::edition::window_title())
                // the window's browser data lives in our own folder, not in
                // one named after the app identifier
                .data_directory(paths::root().join("webview"))
                .inner_size(1320.0, 860.0)
                .min_inner_size(900.0, 600.0)
                // ⚠ Tauri's own file-drop handler swallows every dropped
                // file, so the page's drag-and-drop (convert an AC1 zip)
                // never fired in the window - only in a browser
                .disable_drag_drop_handler()
                .build()?;
            #[cfg(windows)]
            if let Some(w) = app.get_webview_window("main") {
                set_window_icons(&w);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("tauri");
}
