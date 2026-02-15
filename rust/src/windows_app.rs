use anyhow::{anyhow, Context, Result};
use image_prompt_generator::config_store::ConfigStore;
use image_prompt_generator::history_store::HistoryStore;
use image_prompt_generator::path_utils::{get_base_dir, resolve_config_path};
use image_prompt_generator::server::{AppServer, AppState};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::env;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::{Window, WindowId};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTSIZE, LR_LOADFROMFILE, LR_SHARED, LoadImageW,
    SendMessageW, WM_SETICON,
};
use wry::{WebView, WebViewBuilder};

struct Args {
    config: Option<String>,
}

pub fn run() -> Result<()> {
    let args = parse_args();
    let base_dir = get_base_dir();
    let config_path = resolve_config_path(args.config, &base_dir);

    let config = ConfigStore::new(config_path.clone())
        .with_context(|| format!("設定ファイルエラー: {}", config_path.display()))?;
    let preferred_port = config.history_server_port();
    let history_max_entries = config.history_max_entries();

    let history_store = HistoryStore::new(base_dir.clone(), history_max_entries)
        .context("履歴機能エラー: history store初期化に失敗しました")?;

    let state = Arc::new(AppState::new(config, history_store));
    let server = AppServer::start(state.clone(), preferred_port)
        .context("履歴機能エラー: history server起動に失敗しました")?;

    {
        let history_regen = state
            .history
            .lock()
            .map_err(|_| anyhow!("history lock error"))?;
        history_regen
            .regenerate_html(server.port())
            .context("履歴機能エラー: initial History.html生成に失敗しました")?;
    }

    let url = format!("http://127.0.0.1:{}/", server.port());
    let trace_enabled = is_win_dpi_trace_enabled();
    let event_loop = build_event_loop().context("failed to create event loop")?;

    let mut app = DesktopApp::new(url, server, trace_enabled);
    event_loop
        .run_app(&mut app)
        .context("event loop terminated unexpectedly")?;

    Ok(())
}

struct DesktopApp {
    url: String,
    window: Option<Window>,
    webview: Option<WebView>,
    server: Option<AppServer>,
    last_logical_size: LogicalSize<f64>,
    trace_enabled: bool,
}

impl DesktopApp {
    fn new(url: String, server: AppServer, trace_enabled: bool) -> Self {
        Self {
            url,
            window: None,
            webview: None,
            server: Some(server),
            last_logical_size: LogicalSize::new(1120.0, 760.0),
            trace_enabled,
        }
    }

    fn init_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if self.window.is_some() {
            return Ok(());
        }

        let attrs = Window::default_attributes()
            .with_title("Image Prompt Generator")
            .with_inner_size(self.last_logical_size);

        let window = event_loop
            .create_window(attrs)
            .context("failed to create main window")?;
        apply_window_icon(&window, self.trace_enabled);

        let webview = WebViewBuilder::new()
            .with_url(&self.url)
            .build(&window)
            .context("failed to build webview")?;

        self.last_logical_size = window.inner_size().to_logical(window.scale_factor());
        self.webview = Some(webview);
        self.window = Some(window);
        Ok(())
    }

    fn shutdown_server(&mut self) {
        if let Some(mut server) = self.server.take() {
            server.stop();
        }
    }
}

impl ApplicationHandler for DesktopApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(err) = self.init_window(event_loop) {
            eprintln!("{err}");
            self.shutdown_server();
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.shutdown_server();
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let Some(scale_factor) = self.window.as_ref().map(Window::scale_factor) {
                    self.last_logical_size = new_size.to_logical(scale_factor);
                    if self.trace_enabled {
                        eprintln!(
                            "[dpi-trace] event=Resized physical={}x{} logical={:.2}x{:.2} scale_factor={scale_factor:.4}",
                            new_size.width,
                            new_size.height,
                            self.last_logical_size.width,
                            self.last_logical_size.height
                        );
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if self.trace_enabled {
                    if let Some(window) = self.window.as_ref() {
                        let physical = window.inner_size();
                        let logical = physical.to_logical::<f64>(scale_factor);
                        eprintln!(
                            "[dpi-trace] event=ScaleFactorChanged physical={}x{} logical={:.2}x{:.2} scale_factor={scale_factor:.4}",
                            physical.width,
                            physical.height,
                            logical.width,
                            logical.height
                        );
                    } else {
                        eprintln!(
                            "[dpi-trace] event=ScaleFactorChanged scale_factor={scale_factor:.4} (window unavailable)"
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.shutdown_server();
    }
}

fn parse_args() -> Args {
    let mut config = None;
    let mut args = env::args().skip(1).peekable();

    while let Some(arg) = args.next() {
        if arg == "--config" {
            if let Some(value) = args.next() {
                config = Some(value);
            }
        }
    }

    Args { config }
}

fn build_event_loop() -> Result<EventLoop<()>> {
    let mut builder = EventLoop::builder();
    // Use app manifest for DPI mode and avoid duplicating process-wide DPI setup here.
    builder.with_dpi_aware(false);
    builder.build().map_err(Into::into)
}

fn is_win_dpi_trace_enabled() -> bool {
    match env::var("IPG_WIN_DPI_TRACE") {
        Ok(raw) => {
            let value = raw.trim();
            value == "1"
                || value.eq_ignore_ascii_case("true")
                || value.eq_ignore_ascii_case("yes")
                || value.eq_ignore_ascii_case("on")
        }
        Err(_) => false,
    }
}

fn apply_window_icon(window: &Window, trace_enabled: bool) {
    let Some(hwnd) = hwnd_from_window(window) else {
        if trace_enabled {
            eprintln!("[dpi-trace] event=WindowIcon hwnd_unavailable");
        }
        return;
    };

    if let Some(icon_handle) = load_icon_handle_from_resource() {
        unsafe {
            SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon_handle);
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, icon_handle);
        }
        if trace_enabled {
            eprintln!("[dpi-trace] event=WindowIcon applied source=embedded_resource");
        }
        return;
    }

    let Some(icon_path) = resolve_icon_path() else {
        if trace_enabled {
            eprintln!("[dpi-trace] event=WindowIcon embedded_resource_missing_and_file_not_found");
        }
        return;
    };

    let Some(icon_handle) = load_icon_handle_from_file(&icon_path) else {
        if trace_enabled {
            eprintln!(
                "[dpi-trace] event=WindowIcon load_failed path={}",
                icon_path.display()
            );
        }
        return;
    };

    unsafe {
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon_handle);
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, icon_handle);
    }

    if trace_enabled {
        eprintln!(
            "[dpi-trace] event=WindowIcon applied source=file path={}",
            icon_path.display()
        );
    }
}

fn hwnd_from_window(window: &Window) -> Option<*mut core::ffi::c_void> {
    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(win32) => Some(win32.hwnd.get() as *mut core::ffi::c_void),
        _ => None,
    }
}

fn resolve_icon_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            candidates.push(exe_dir.join("app.ico"));
            candidates.push(exe_dir.join("assets").join("app.ico"));
        }
    }

    candidates.push(PathBuf::from("assets").join("app.ico"));
    candidates.push(PathBuf::from("app.ico"));

    candidates.into_iter().find(|path| path.is_file())
}

fn load_icon_handle_from_resource() -> Option<isize> {
    let module = unsafe { GetModuleHandleW(core::ptr::null()) };
    if module.is_null() {
        return None;
    }

    // winres embeds the primary icon as the first icon resource.
    let icon_resource_id = 1usize as *const u16;
    let handle = unsafe {
        LoadImageW(
            module,
            icon_resource_id,
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE | LR_SHARED,
        )
    };

    if handle.is_null() {
        None
    } else {
        Some(handle as isize)
    }
}

fn load_icon_handle_from_file(path: &Path) -> Option<isize> {
    let mut wide = path.as_os_str().encode_wide().collect::<Vec<u16>>();
    wide.push(0);

    let handle = unsafe {
        LoadImageW(
            core::ptr::null_mut(),
            wide.as_ptr(),
            IMAGE_ICON,
            0,
            0,
            LR_LOADFROMFILE | LR_DEFAULTSIZE | LR_SHARED,
        )
    };

    if handle.is_null() {
        None
    } else {
        Some(handle as isize)
    }
}
