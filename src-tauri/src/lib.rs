mod config;
mod http;
mod lastfm_auth;
mod media_center;
mod models;

use discord_presence::DiscordError;
use parking_lot::Mutex;
use parking_lot::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use media_center::{MediaCenter, TrackUpdateEvent};

use tauri::async_runtime::JoinHandle;
use tauri::Manager;
use tauri::State;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIcon,
};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn load_config(state: tauri::State<'_, AppState>) -> config::Config {
    state.config.read().clone()
}

#[tauri::command]
async fn start_lastfm_auth(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let token = lastfm_auth::fetch_token().await?;
    let auth_url = lastfm_auth::build_auth_url(&token);
    app.opener()
        .open_url(&auth_url, None::<&str>)
        .map_err(|e| format!("failed to open browser: {e}"))?;
    *state.pending_auth.lock() = Some(token);
    Ok(())
}

#[tauri::command]
async fn complete_lastfm_auth(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let token = state
        .pending_auth
        .lock()
        .take()
        .ok_or_else(|| "no pending authorization".to_string())?;
    lastfm_auth::exchange_token(&token).await
}

#[tauri::command]
async fn start_librefm_auth(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let (token, auth_url) = lastfm_auth::fetch_librefm_token().await?;
    app.opener()
        .open_url(&auth_url, None::<&str>)
        .map_err(|e| format!("failed to open browser: {e}"))?;
    *state.pending_auth.lock() = Some(token);
    Ok(())
}

#[tauri::command]
async fn complete_librefm_auth(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let token = state
        .pending_auth
        .lock()
        .take()
        .ok_or_else(|| "no pending authorization".to_string())?;
    lastfm_auth::exchange_librefm_token(&token).await
}

#[derive(serde::Serialize)]
struct UpdateInfo {
    current: semver::Version,
    latest: semver::Version,
    url: String,
    available: bool,
}

fn parse_version(s: &str) -> Vec<u64> {
    s.trim_start_matches('v')
        .split('.')
        .filter_map(|p| p.parse().ok())
        .collect()
}

#[tauri::command]
fn get_app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let current = semver::Version::parse(&app.package_info().version.to_string())
        .map_err(|e| format!("invalid version string: {e}"))?;
    let url = "https://api.github.com/repos/thrzl/dotsong/releases/latest";
    let resp = crate::http::client()
        .get(url)
        .header("User-Agent", "thrzl/dotsong")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("github returned {}", resp.status()));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
    let tag = json["tag_name"]
        .as_str()
        .ok_or_else(|| "no tag_name in release".to_string())?;
    let html_url = json["html_url"]
        .as_str()
        .ok_or_else(|| "no html_url in release".to_string())?;
    let latest = semver::Version::parse(&tag.trim_start_matches('v').to_string())
        .map_err(|e| format!("invalid version string: {e}"))?;
    let available = &latest > &current;
    Ok(UpdateInfo {
        current,
        latest,
        url: html_url.to_string(),
        available,
    })
}

#[tauri::command]
async fn save_config(
    state: tauri::State<'_, AppState>,
    config: config::Config,
) -> Result<(), String> {
    // update internal config state
    *state.config.write() = config.clone();

    if config.discord_rpc_enabled {
        let mut task_lock = state.presence_task.lock();

        if task_lock.is_none() {
            *task_lock = Some(state.start_discord_presence());
        }
    } else {
        println!("stopping discord presence");
        state.stop_discord_presence();
    }
    let config_path = &state.config_path;
    let config_str = serde_json::to_string_pretty(&config).expect("failed to serialize config");
    state.media_center.set_scrobblers(config.scrobblers.clone());
    println!("writing config");
    tokio::fs::write(config_path, config_str)
        .await
        .map_err(|e| format!("failed to write config file: {e}"))
}

struct AppState {
    media_center: Arc<MediaCenter>,
    tray: Arc<Mutex<TrayIcon>>,
    quitting: AtomicBool,
    config: Arc<RwLock<config::Config>>,
    config_path: std::path::PathBuf,
    presence_task: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    rpc: Arc<Mutex<Option<discord_presence::Client>>>,
    pending_auth: Arc<Mutex<Option<lastfm_auth::AuthToken>>>,
}

impl AppState {
    fn stop_discord_presence(&self) {
        if let Some(handle) = self.presence_task.lock().take() {
            handle.abort();
        }
        let rpc = self.rpc.lock().take();
        if let Some(mut rpc) = rpc {
            // these both only error if the rpc isn't started
            // so we don't actually care
            let _ = rpc.clear_activity();
            let _ = rpc.shutdown();
        } else {
            println!("no rpc client to shutdown");
        }
    }
    fn start_tray_updater(&self, app: &tauri::AppHandle) {
        let mut track_rx = self.media_center.get_rx();
        let tray = self.tray.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                track_rx.changed().await.unwrap();
                let track = match track_rx.borrow_and_update().clone() {
                    TrackUpdateEvent::NewTrack(track) => track,
                    _ => continue,
                };
                let now_playing_title = track.title.clone().unwrap_or_else(|| "-".to_string());
                let now_playing_artist = track.artist.clone().unwrap_or_else(|| "-".to_string());
                let nothing_playing = now_playing_title == "-" && now_playing_artist == "-";
                let now_playing_text = if nothing_playing {
                    "nothing playing".to_string()
                } else {
                    format!(
                        "now playing: {} - {}",
                        now_playing_title, now_playing_artist
                    )
                };

                let now_playing =
                    MenuItem::with_id(&app, "now_playing", &now_playing_text, false, None::<&str>)
                        .unwrap();
                let settings =
                    MenuItem::with_id(&app, "settings", "settings", true, None::<&str>).unwrap();
                let quit = MenuItem::with_id(&app, "quit", "quit", true, None::<&str>).unwrap();
                let menu = Menu::with_items(&app, &[&now_playing, &settings, &quit]).unwrap();

                tray.lock().set_menu(Some(menu)).unwrap();
            }
        });
    }

    fn start_discord_presence(&self) -> JoinHandle<()> {
        let mut rx = self.media_center.get_rx();
        {
            let rpc = discord_presence::Client::new(1516876269248315422);
            self.rpc.lock().replace(rpc);
        }
        let rpc = self.rpc.clone();
        tauri::async_runtime::spawn(async move {
            {
                let mut guard = rpc.lock();
                let rpc = guard.as_mut().unwrap();
                let _ = rpc.on_ready(|_client| {
                    println!("discord RPC connected");
                });

                rpc.start();
            }

            loop {
                rx.changed().await.unwrap();
                let track_event = rx.borrow_and_update().clone();
                let mut guard = rpc.lock();
                let Some(client) = guard.as_mut() else {
                    continue;
                };
                let media_info = match track_event {
                    TrackUpdateEvent::NewTrack(info) => info,
                    TrackUpdateEvent::PlaybackStateChange(info) => info,
                    _ => continue,
                };
                if !media_info.is_playing {
                    match client.clear_activity() {
                        Err(DiscordError::NotStarted) => {
                            eprintln!("discord presence update failed; rpc not connected");
                        }
                        _ => {}
                    };
                    continue;
                }
                if media_info.title.is_some() && media_info.artist.is_some() {
                    let activity_result = client.set_activity(|p| {
                        p.activity_type(discord_presence::models::ActivityType::Listening)
                            .status_display(discord_presence::models::DisplayType::State)
                            .state(media_info.artist.clone().unwrap_or_default())
                            .details(media_info.title.clone().unwrap_or_default())
                            .assets(|assets| {
                                let assets = assets.large_image(
                                    &media_info
                                        .cover_artwork
                                        .clone()
                                        .unwrap_or("default".to_string()),
                                );
                                if let Some(album_name) = media_info.album.clone() {
                                    assets.large_text(album_name)
                                } else {
                                    assets
                                }
                            })
                            .timestamps(|timestamps| {
                                if let Some(elapsed_time) = media_info.elapsed_time {
                                    let start_time = chrono::Utc::now()
                                        - chrono::Duration::seconds(elapsed_time as i64);

                                    if media_info.duration.is_some_and(|duration| duration > 0) {
                                        timestamps.start(start_time.timestamp() as u64).end(
                                            media_info.duration.unwrap() as u64
                                                + start_time.timestamp() as u64,
                                        )
                                    } else {
                                        timestamps.start(start_time.timestamp() as u64)
                                    }
                                } else {
                                    timestamps
                                }
                            })
                    });
                    match activity_result {
                        Err(DiscordError::NotStarted) => {
                            eprintln!("discord presence update failed; rpc not connected");
                        }
                        _ => {}
                    }
                } else {
                    match client.clear_activity() {
                        Err(DiscordError::NotStarted) => {
                            eprintln!("discord presence update failed; rpc not connected");
                        }
                        _ => {}
                    };
                }
            }
        })
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let program = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {})) // we don't gotta do anything just don't reopen
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let icon = tauri::include_image!("./icons/tray-icon.png");
            let now_playing =
                MenuItem::with_id(app, "now_playing", "nothing playing", false, None::<&str>)
                    .unwrap();
            let quit = MenuItem::with_id(app, "quit", "quit", true, None::<&str>).unwrap();
            let settings =
                MenuItem::with_id(app, "settings", "settings", true, None::<&str>).unwrap();
            let menu = Menu::with_items(app, &[&now_playing, &settings, &quit]).unwrap();
            let tray = tauri::tray::TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(true)
                .icon(icon)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        let quitting = &app.state::<Arc<AppState>>().quitting;
                        quitting.store(true, std::sync::atomic::Ordering::SeqCst);
                        app.exit(0);
                    }
                    "settings" => {
                        #[cfg(target_os = "macos")]
                        app.set_activation_policy(tauri::ActivationPolicy::Regular)
                            .ok();
                        // create new settings window
                        if let Some(window) = app.get_webview_window("main") {
                            window.show().unwrap();
                            window.set_always_on_top(true).unwrap();
                            window.set_focus().unwrap();
                            window.set_always_on_top(false).unwrap();
                            return;
                        }
                        let window = tauri::WebviewWindowBuilder::new(
                            app,
                            "main",
                            tauri::WebviewUrl::App("index.html".into()),
                        )
                        .title("dotsong settings")
                        .inner_size(400.0, 600.0)
                        .resizable(false)
                        .decorations(true)
                        .visible(true)
                        .focused(true)
                        .always_on_top(true)
                        .build()
                        .unwrap();
                        window.set_always_on_top(false).unwrap();
                    }
                    _ => {}
                })
                .build(app)?;
            let app_config_dir = dirs::config_dir()
                .expect("failed to resolve config directory")
                .join(app.config().identifier.clone());
            std::fs::create_dir_all(&app_config_dir)
                .expect("failed to ensure config directory exists");
            let config = {
                // read app_config_dir/config.json if it exists, otherwise create it with default config
                let config_path = app_config_dir.join("dotsong_config.json");
                if config_path.exists() {
                    let config_str =
                        std::fs::read_to_string(config_path).expect("failed to read config file");
                    serde_json::from_str(&config_str).expect("failed to parse config file")
                } else {
                    let default_config = config::Config::default();
                    let config_str = serde_json::to_string_pretty(&default_config)
                        .expect("failed to serialize default config");
                    std::fs::write(config_path, config_str)
                        .expect("failed to write default config file");
                    default_config
                }
            };
            let app_state = AppState {
                media_center: Arc::new(MediaCenter::new(config.scrobblers.clone())),
                tray: Arc::new(Mutex::new(tray)),
                quitting: AtomicBool::new(false),
                config: Arc::new(RwLock::new(config)),
                config_path: app_config_dir.join("dotsong_config.json"),
                presence_task: Arc::new(Mutex::new(None)),
                rpc: Arc::new(Mutex::new(None)),
                pending_auth: Arc::new(Mutex::new(None)),
            };

            app_state.media_center.clone().start_media_poller();
            if app_state.config.read().discord_rpc_enabled {
                *app_state.presence_task.lock() = Some(app_state.start_discord_presence());
            }
            app_state.media_center.clone().start_scrobbling_task();
            app_state.start_tray_updater(app.handle());

            app.manage(app_state);
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            save_config,
            load_config,
            start_lastfm_auth,
            complete_lastfm_auth,
            start_librefm_auth,
            complete_librefm_auth,
            get_app_version,
            check_for_update
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application");
    program.run(|_app, event| match event {
        tauri::RunEvent::ExitRequested { api, .. } => {
            let s: State<AppState> = _app.state();
            if s.quitting.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            api.prevent_exit();
        }
        tauri::RunEvent::WindowEvent { label, event, .. } => {
            if label == "main" {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    #[cfg(target_os = "macos")]
                    _app.set_activation_policy(tauri::ActivationPolicy::Accessory)
                        .ok();
                }
            }
        }
        _ => {}
    })
}
