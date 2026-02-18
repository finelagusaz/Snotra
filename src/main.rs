#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
mod app;
#[cfg(target_os = "windows")]
mod binfmt;
#[cfg(target_os = "windows")]
mod config;
#[cfg(target_os = "windows")]
mod folder;
#[cfg(target_os = "windows")]
mod history;
#[cfg(target_os = "windows")]
mod hotkey;
#[cfg(target_os = "windows")]
mod icon;
#[cfg(target_os = "windows")]
mod ime;
#[cfg(target_os = "windows")]
mod indexer;
#[cfg(target_os = "windows")]
mod launcher;
#[cfg(target_os = "windows")]
mod platform_win32;
#[cfg(target_os = "windows")]
mod query;
#[cfg(target_os = "windows")]
mod search;
#[cfg(target_os = "windows")]
mod ui_types;
#[cfg(target_os = "windows")]
mod window_data;

#[cfg(target_os = "windows")]
fn main() {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;
    use std::rc::Rc;

    use eframe::egui;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    use config::{Config, RendererConfig, WgpuBackendConfig};

    struct SingletonGuard(HANDLE);
    impl Drop for SingletonGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    fn startup_log_path() -> Option<PathBuf> {
        let base = std::env::var_os("LOCALAPPDATA")?;
        Some(PathBuf::from(base).join("Snotra").join("startup.log"))
    }

    fn log_startup_line(message: &str) {
        if let Some(path) = startup_log_path() {
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(f, "{}", message);
            }
        }
    }

    std::panic::set_hook(Box::new(|panic_info| {
        log_startup_line(&format!("main:panic={panic_info}"));
        let backtrace = std::backtrace::Backtrace::force_capture();
        log_startup_line(&format!("main:panic_backtrace={backtrace}"));
    }));

    fn acquire_singleton() -> Option<SingletonGuard> {
        let mutex_name = HSTRING::from("Global\\Snotra.Singleton");
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr())) }.ok()?;
        let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if already_exists {
            unsafe {
                let _ = CloseHandle(handle);
            }
            None
        } else {
            Some(SingletonGuard(handle))
        }
    }

    fn resolve_renderer(renderer: RendererConfig) -> (eframe::Renderer, &'static str) {
        match renderer {
            RendererConfig::Auto => (eframe::Renderer::Wgpu, "auto->wgpu"),
            RendererConfig::Wgpu => (eframe::Renderer::Wgpu, "wgpu"),
            RendererConfig::Glow => (eframe::Renderer::Glow, "glow"),
        }
    }

    fn resolve_wgpu_backends(mode: WgpuBackendConfig) -> (eframe::wgpu::Backends, &'static str) {
        match mode {
            WgpuBackendConfig::Auto => (eframe::wgpu::Backends::DX12, "auto->dx12"),
            WgpuBackendConfig::Dx12 => (eframe::wgpu::Backends::DX12, "dx12"),
            WgpuBackendConfig::Vulkan => (eframe::wgpu::Backends::VULKAN, "vulkan"),
            WgpuBackendConfig::Gl => (eframe::wgpu::Backends::GL, "gl"),
        }
    }

    log_startup_line("main:start");
    let is_settings_window_mode = std::env::args().any(|arg| arg == "--settings-window");
    if is_settings_window_mode {
        log_startup_line("main:mode=settings_window");
        let mut config = Config::load();
        config.appearance.max_history_display = config
            .appearance
            .max_history_display
            .min(config.appearance.max_results);

        let mut viewport = egui::ViewportBuilder::default()
            .with_title("Snotra 設定")
            .with_decorations(true)
            .with_visible(true)
            .with_taskbar(true)
            .with_resizable(true)
            .with_min_inner_size([520.0, 360.0])
            .with_inner_size([760.0, 560.0]);
        if let Some(size) = window_data::load_settings_size() {
            viewport =
                viewport.with_inner_size([size.width.max(520) as f32, size.height.max(360) as f32]);
        }
        if let Some(placement) = window_data::load_settings_placement() {
            viewport = viewport.with_position([placement.x as f32, placement.y as f32]);
        }

        let (renderer, renderer_label) = resolve_renderer(config.general.renderer);
        let (wgpu_backends, wgpu_backends_label) =
            resolve_wgpu_backends(config.general.wgpu_backend);
        log_startup_line(&format!("main:settings_renderer={renderer_label}"));
        log_startup_line(&format!(
            "main:settings_wgpu_backends={wgpu_backends_label}"
        ));

        let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
        if matches!(renderer, eframe::Renderer::Wgpu) {
            let mut create_new = eframe::egui_wgpu::WgpuSetupCreateNew::default();
            create_new.instance_descriptor.backends = wgpu_backends;
            wgpu_options.wgpu_setup = eframe::egui_wgpu::WgpuSetup::CreateNew(create_new);
        }

        let native_options = eframe::NativeOptions {
            viewport,
            renderer,
            wgpu_options,
            ..Default::default()
        };

        let init = app::AppInit {
            config: config.clone(),
            engine: search::SearchEngine::new(Vec::new()),
            history: history::HistoryStore::load(
                config.appearance.top_n_history,
                config.appearance.max_history_display,
            ),
            icon_cache: None,
            platform: platform_win32::PlatformBridge::disabled(),
        };

        match eframe::run_native(
            "Snotra 設定",
            native_options,
            Box::new(move |cc| Ok(Box::new(app::SnotraApp::new_settings_window(cc, init)))),
        ) {
            Ok(()) => {
                log_startup_line(&format!(
                    "main:settings_run_native_ok renderer={renderer_label} wgpu_backends={wgpu_backends_label}"
                ));
            }
            Err(e) => {
                let message = format!(
                    "main:settings_run_native_err renderer={renderer_label} wgpu_backends={wgpu_backends_label} err={e}"
                );
                log_startup_line(&message);
                let title = HSTRING::from("Snotra settings startup error");
                let text = HSTRING::from(message);
                unsafe {
                    let _ = MessageBoxW(
                        None,
                        PCWSTR(text.as_ptr()),
                        PCWSTR(title.as_ptr()),
                        MB_OK | MB_ICONERROR,
                    );
                }
            }
        }
        return;
    }

    let Some(_singleton_guard) = acquire_singleton() else {
        log_startup_line("main:singleton_exists");
        return;
    };
    log_startup_line("main:singleton_acquired");

    let class_name = HSTRING::from(platform_win32::PLATFORM_WINDOW_CLASS);
    log_startup_line(&format!(
        "main:platform_window_class={}",
        class_name.to_string_lossy()
    ));

    let mut config = Config::load();
    log_startup_line("main:config_loaded");
    log_startup_line(&format!(
        "main:config_general show_on_startup={} show_tray_icon={} show_title_bar={} renderer={:?} wgpu_backend={:?}",
        config.general.show_on_startup,
        config.general.show_tray_icon,
        config.general.show_title_bar,
        config.general.renderer,
        config.general.wgpu_backend
    ));
    let startup_visibility = if config.general.show_on_startup {
        "search_visible"
    } else if config.general.show_tray_icon {
        "tray_icon_only"
    } else {
        "hidden_hotkey_only"
    };
    log_startup_line(&format!("main:startup_visibility={startup_visibility}"));
    config.appearance.max_history_display = config
        .appearance
        .max_history_display
        .min(config.appearance.max_results);

    let (entries, rescanned) = indexer::load_or_scan(
        &config.paths.additional,
        &config.paths.scan,
        config.search.show_hidden_system,
    );
    log_startup_line(&format!(
        "main:index_loaded entries={} rescanned={}",
        entries.len(),
        rescanned
    ));

    let icon_cache = if config.appearance.show_icons {
        if rescanned {
            let cache = icon::IconCache::build(&entries);
            cache.save();
            Some(Rc::new(cache))
        } else {
            Some(Rc::new(icon::IconCache::load().unwrap_or_else(|| {
                let cache = icon::IconCache::build(&entries);
                cache.save();
                cache
            })))
        }
    } else {
        None
    };

    let engine = search::SearchEngine::new(entries);
    let history = history::HistoryStore::load(
        config.appearance.top_n_history,
        config.appearance.max_history_display,
    );

    let Some(platform) =
        platform_win32::PlatformBridge::start(config.hotkey.clone(), config.general.show_tray_icon)
    else {
        log_startup_line("main:platform_start_failed");
        return;
    };
    log_startup_line("main:platform_started");

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Snotra")
        .with_decorations(config.general.show_title_bar)
        .with_visible(config.general.show_on_startup)
        .with_taskbar(config.general.show_on_startup)
        .with_inner_size([
            config.appearance.window_width as f32,
            app::search_window_height(config.appearance.max_results, config.visual.font_size),
        ]);

    if let Some(placement) = window_data::load_search_placement() {
        viewport = viewport.with_position([placement.x as f32, placement.y as f32]);
    }

    let (renderer, renderer_label) = resolve_renderer(config.general.renderer);
    let (wgpu_backends, wgpu_backends_label) = resolve_wgpu_backends(config.general.wgpu_backend);
    log_startup_line(&format!("main:renderer={renderer_label}"));
    log_startup_line(&format!("main:wgpu_backends={wgpu_backends_label}"));

    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    if matches!(renderer, eframe::Renderer::Wgpu) {
        let mut create_new = eframe::egui_wgpu::WgpuSetupCreateNew::default();
        create_new.instance_descriptor.backends = wgpu_backends;
        wgpu_options.wgpu_setup = eframe::egui_wgpu::WgpuSetup::CreateNew(create_new);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        renderer,
        wgpu_options,
        ..Default::default()
    };

    let init = app::AppInit {
        config,
        engine,
        history,
        icon_cache,
        platform,
    };

    match eframe::run_native(
        "Snotra",
        native_options,
        Box::new(move |cc| Ok(Box::new(app::SnotraApp::new(cc, init)))),
    ) {
        Ok(()) => {
            log_startup_line(&format!(
                "main:run_native_ok renderer={renderer_label} wgpu_backends={wgpu_backends_label}"
            ));
        }
        Err(e) => {
            let message = format!(
                "main:run_native_err renderer={renderer_label} wgpu_backends={wgpu_backends_label} err={e}"
            );
            log_startup_line(&message);
            let title = HSTRING::from("Snotra startup error");
            let text = HSTRING::from(message);
            unsafe {
                let _ = MessageBoxW(
                    None,
                    PCWSTR(text.as_ptr()),
                    PCWSTR(title.as_ptr()),
                    MB_OK | MB_ICONERROR,
                );
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("Snotra is Windows-only.");
}
