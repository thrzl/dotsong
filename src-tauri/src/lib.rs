mod models;

use std::sync::{mpsc, Arc};
use std::thread::sleep;
use std::time::Duration;

use models::MediaInfo;
use parking_lot::Mutex;
use async_trait::async_trait;

use tauri::State;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon},
    Manager
};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

enum ScrobblerFormat {
    ListenBrainz,
    LastFM
}

#[async_trait]
trait Scrobbler: Send + Sync {
    async fn scrobble(&self, track: &MediaInfo);
}

struct ListenBrainzScrobbler {
    endpoint_url: String,
    api_key: String,
}

#[async_trait]
impl Scrobbler for ListenBrainzScrobbler {
    async fn scrobble(&self, track: &MediaInfo) {
        // implement ListenBrainz scrobbling logic here
    }
}

struct LastFMScrobbler {
    endpoint_url: String,
    api_key: String,
}

#[async_trait]
impl Scrobbler for LastFMScrobbler {
    async fn scrobble(&self, track: &MediaInfo) {
        // implement LastFM scrobbling logic here
    }
}

struct ScrobblerConfig {
    endpoint_url: String,
    api_key: String,
    format: ScrobblerFormat,
}

impl ScrobblerConfig {
    fn scrobbler(&self) -> Box<dyn Scrobbler + Send + Sync> {
        match self.format {
            ScrobblerFormat::ListenBrainz => Box::new(ListenBrainzScrobbler {
                endpoint_url: self.endpoint_url.clone(),
                api_key: self.api_key.clone(),
            }),
            ScrobblerFormat::LastFM => Box::new(LastFMScrobbler {
                endpoint_url: self.endpoint_url.clone(),
                api_key: self.api_key.clone(),
            }),
        }
    }
}

struct Config {
    scrobblers: Vec<ScrobblerConfig>,
    discord_rpc_enabled: bool,
}

struct AppState {
    current_track: Arc<Mutex<Option<MediaInfo>>>,
    last_track: Arc<Mutex<Option<MediaInfo>>>,
    track_tx: Arc<Mutex<mpsc::Sender<MediaInfo>>>,
    track_rx: Arc<Mutex<mpsc::Receiver<MediaInfo>>>,
    tray: Arc<Mutex<TrayIcon>>,
    quitting: Arc<Mutex<bool>>
}

// for windows and linux
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn get_now_playing_info() -> Option<MediaInfo> {
    let now_playing = nowhear::MediaSourceBuilder::new().build();
    nowhear::MediaEvent::get_current_media_info().map(|info| MediaInfo {
        title: info.title,
        album: info.album,
        artist: info.artist,
        elapsed_time: info.elapsed_time.map(|t| t as u32),
        cover_artwork: None, // No cover artwork support for nowhear
    })
}

impl AppState {
    fn should_broadcast_track(previous: Option<&MediaInfo>, current: &MediaInfo) -> bool {
        let Some(previous) = previous else {
            return true;
        };

        if previous.title != current.title
            || previous.artist != current.artist
            || previous.is_playing != current.is_playing
        {
            return true;
        }

        match (previous.elapsed_time, current.elapsed_time) {
            (Some(previous_elapsed), Some(current_elapsed)) => {
                previous_elapsed.abs_diff(current_elapsed) >= 2
            }
            _ => false,
        }
    }

    fn should_refresh_tray_menu(previous: Option<&MediaInfo>, current: &MediaInfo) -> bool {
        let Some(previous) = previous else {
            return true;
        };

        previous.title != current.title || previous.artist != current.artist
    }

    fn update_tray_menu(&self, app: &tauri::AppHandle) {
        let current_track = self.current_track.lock().clone();
        let now_playing_title = current_track
            .as_ref()
            .and_then(|track| track.title.clone())
            .unwrap_or_else(|| "-".to_string());
        let now_playing_artist = current_track
            .as_ref()
            .and_then(|track| track.artist.clone())
            .unwrap_or_else(|| "-".to_string());
        let now_playing_text = format!(
            "now playing: {} - {}",
            now_playing_title, now_playing_artist
        );

        let now_playing =
            MenuItem::with_id(app, "now_playing", &now_playing_text, false, None::<&str>).unwrap();
        let settings = MenuItem::with_id(app, "settings", "settings", true, None::<&str>).unwrap();
        let quit = MenuItem::with_id(app, "quit", "quit", true, None::<&str>).unwrap();
        let menu = Menu::with_items(app, &[&now_playing, &settings, &quit]).unwrap();

        self.tray.lock().set_menu(Some(menu)).unwrap();
    }

    /// remove suffixes like " - Single" and " - EP" from apple music album names to improve matching with Deezer API
    fn sanitize_apple_music_album_name(album_name: &str) -> String {
        let patterns = [" - Single", " - EP"];

        let mut sanitized_name = album_name.to_string();
        for pattern in patterns {
            sanitized_name = sanitized_name.replace(pattern, "");
        }
        sanitized_name.trim().to_string()
    }

    #[cfg(target_os = "macos")]
    fn start_media_poller(self: Arc<Self>, app: tauri::AppHandle) {
        println!("starting media poller");
        let tx = self.track_tx.clone();
        let current_track_mut = self.current_track.clone();
        std::thread::spawn(move || {
            let mut deezer_client = models::deezer_api::DeezerClient::new(100);
            let now_playing = media_remote::NowPlayingPerl::new();
            loop {
                use tauri::async_runtime;

                sleep(Duration::from_millis(500));
                let Some(media) = now_playing.get_info().clone() else {
                    continue;
                };
                if media.title.is_none() && media.album.is_none() {
                    continue;
                }

                let media_info = MediaInfo {
                    title: media.title,
                    album: media
                        .album
                        .map(|album| Self::sanitize_apple_music_album_name(&album)),
                    artist: media.artist,
                    elapsed_time: media.elapsed_time.map(|t| t as u32),
                    cover_artwork: None,
                    is_playing: media.is_playing.unwrap_or(false),
                    duration: media.duration.map(|t| t as u64),
                };

                // asynchronous enriching of media info with Deezer API
                let media_info_clone = media_info.clone();
                let enriched_track = async_runtime::block_on(async {
                    if let Some(enriched_info) = deezer_client.track_search(&media_info_clone).await
                    {
                        let media_info = MediaInfo {
                            title: Some(
                                media_info_clone
                                    .title
                                    .clone()
                                    .unwrap_or(enriched_info.title),
                            ),
                            album: Some(
                                media_info_clone
                                    .album
                                    .clone()
                                    .unwrap_or(enriched_info.album.title),
                            ),
                            artist: Some(
                                media_info_clone
                                    .artist
                                    .clone()
                                    .unwrap_or(enriched_info.artist),
                            ),
                            elapsed_time: media_info_clone
                                .elapsed_time
                                .or(enriched_info.elapsed_time),
                            cover_artwork: enriched_info.cover_artwork,
                            is_playing: media_info_clone.is_playing,
                            duration: media_info_clone.duration.or(Some(enriched_info.duration)),
                        };
                        return media_info;
                    } else {
                        return media_info_clone;
                    }
                });
                let previous_track = self.last_track.lock().clone();
                if !Self::should_broadcast_track(previous_track.as_ref(), &enriched_track) {
                    continue;
                }
                let should_refresh_tray_menu =
                    Self::should_refresh_tray_menu(previous_track.as_ref(), &enriched_track);
                {
                    let mut last_track = self.last_track.lock();
                    *last_track = Some(enriched_track.clone());
                }
                {
                    let mut current_track = current_track_mut.lock();
                    *current_track = Some(enriched_track.clone());
                }

                tx.lock().send(enriched_track).unwrap();
                if should_refresh_tray_menu {
                    self.update_tray_menu(&app);
                }
            }
        });
    }

    fn start_discord_presence(&self) {
        let rx = self.track_rx.clone();
        std::thread::spawn(move || {
            let mut rpc = discord_presence::Client::new(1516876269248315422);

            let _ = rpc.on_ready(|_client| {
                println!("discord RPC connected");
            });

            rpc.start();

            loop {
                if let Ok(media_info) = rx.lock().recv() {
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
            let (tx, rx) = mpsc::channel();
            let track_tx = Arc::new(Mutex::new(tx));
            let track_rx = Arc::new(Mutex::new(rx));

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
                        app.get_webview_window("main").unwrap().show().unwrap();
                    }
                    _ => {}
                })
                .build(app)?;
            let app_state = Arc::new(AppState {
                current_track: Arc::new(Mutex::new(None)),
                last_track: Arc::new(Mutex::new(None)),
                track_tx,
                track_rx,
                tray: Arc::new(Mutex::new(tray)),
                quitting: Arc::new(Mutex::new(false)),
            });

            app_state.clone().start_media_poller(app.handle().clone());
            app_state.start_discord_presence();

            app.manage(app_state);
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .build(tauri::generate_context!())
        .expect("error while running tauri application");
    program.run(|_app, event| {
        match event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                let s: State<Arc<AppState>> = _app.state();
                if s.quitting.lock().clone() {
                    return;
                }
                api.prevent_exit();
            }
            _ => {}
        }
    })
}
