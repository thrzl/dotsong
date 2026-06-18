mod config;
mod media_center;
mod models;

use std::sync::Arc;

use models::MediaInfo;
use parking_lot::Mutex;

use media_center::{MediaCenter, TrackUpdateEvent};

use tauri::State;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIcon,
    Manager,
};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

struct AppState {
    media_center: Arc<MediaCenter>,
    tray: Arc<Mutex<TrayIcon>>,
    quitting: Arc<Mutex<bool>>,
}

// for windows and linux
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn get_now_playing_info() -> Option<MediaInfo> {
    let now_playing = nowhear::MediaSourceBuilder::new().build();
    nowhear::MediaEvent::get_current_media_info().map(|info| nowhear::MediaEvent {
        title: info.title,
        album: info.album,
        artist: info.artist,
        elapsed_time: info.elapsed_time.map(|t| t as u32),
        cover_artwork: None, // No cover artwork support for nowhear
    })
}

impl AppState {
    fn start_tray_updater(&self, app: &tauri::AppHandle) {
        let mut track_rx = self.media_center.get_rx();
        let tray = self.tray.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            // let mut track_rx = track_rx.clone();
            loop {
                if let Ok(track_event) = track_rx.recv().await {
                    let track = match track_event {
                        TrackUpdateEvent::NewTrack(info) => info,
                        TrackUpdateEvent::PlaybackStateChange(_) => continue,
                    };
                    let now_playing_title = track.title.clone().unwrap_or_else(|| "-".to_string());
                    let now_playing_artist =
                        track.artist.clone().unwrap_or_else(|| "-".to_string());
                    let now_playing_text = format!(
                        "now playing: {} - {}",
                        now_playing_title, now_playing_artist
                    );

                    let now_playing = MenuItem::with_id(
                        &app,
                        "now_playing",
                        &now_playing_text,
                        false,
                        None::<&str>,
                    )
                    .unwrap();
                    let settings =
                        MenuItem::with_id(&app, "settings", "settings", true, None::<&str>).unwrap();
                    let quit = MenuItem::with_id(&app, "quit", "quit", true, None::<&str>).unwrap();
                    let menu = Menu::with_items(&app, &[&now_playing, &settings, &quit]).unwrap();

                    tray.lock().set_menu(Some(menu)).unwrap();
                }
            }
        });
    }

    fn start_discord_presence(&self) {
        let mut rx = self.media_center.get_rx();
        tauri::async_runtime::spawn(async move {
            let mut rpc = discord_presence::Client::new(1516876269248315422);

            let _ = rpc.on_ready(|_client| {
                println!("discord RPC connected");
            });

            rpc.start();

            loop {
                if let Ok(track_event) = rx.recv().await {
                    let media_info = match track_event {
                        TrackUpdateEvent::NewTrack(info) => info,
                        TrackUpdateEvent::PlaybackStateChange(info) => info,
                    };
                    if !media_info.is_playing {
                        rpc.clear_activity().unwrap();
                        continue;
                    }
                    if media_info.title.is_some() && media_info.artist.is_some() {
                        rpc.set_activity(|p| {
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
                                        timestamps.start(start_time.timestamp() as u64).end(
                                            media_info.duration.unwrap()
                                                + start_time.timestamp() as u64,
                                        )
                                    } else {
                                        timestamps
                                    }
                                })
                        })
                        .unwrap();
                    } else {
                        rpc.clear_activity().unwrap();
                    }
                }
            }
        });
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let program = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {})) // we don't gotta do anything just don't reopen
        .setup(|app| {
            // let icon_img = image::open("./icons/icon.png").unwrap();
            // let icon_width = icon_img.width();
            // let icon_height = icon_img.height();
            let icon = tauri::include_image!("./icons/tray-icon.png");
            // let icon =
            //     Icon::from_rgba(icon_img.to_rgba8().into_raw(), icon_width, icon_height).unwrap();
            let now_playing =
                MenuItem::with_id(app, "now_playing", "now playing: -", false, None::<&str>)
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
                        let quitting = app.state::<Arc<AppState>>().quitting.clone();
                        let mut quitting_inner = quitting.lock();
                        *quitting_inner = true;
                        app.exit(0);
                    }
                    "settings" => {
                        // create new settings window
                        if let Some(window) = app.get_webview_window("main") {
                            window.show().unwrap();
                            window.set_focus().unwrap();
                            return;
                        }
                        tauri::WebviewWindowBuilder::new(
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
                        .build()
                        .unwrap();
                    }
                    _ => {}
                })
                .build(app)?;
            let app_state = Arc::new(AppState {
                media_center: Arc::new(MediaCenter::new()),
                tray: Arc::new(Mutex::new(tray)),
                quitting: Arc::new(Mutex::new(false)),
            });

            app_state.media_center.clone().start_media_poller();
            app_state.start_discord_presence();
            app_state.start_tray_updater(app.handle());

            app.manage(app_state);
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .build(tauri::generate_context!())
        .expect("error while running tauri application");
    program.run(|_app, event| match event {
        tauri::RunEvent::ExitRequested { api, .. } => {
            let s: State<Arc<AppState>> = _app.state();
            if s.quitting.lock().clone() {
                return;
            }
            api.prevent_exit();
        }
        _ => {}
    })
}
