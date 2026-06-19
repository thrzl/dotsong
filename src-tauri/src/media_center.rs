use crate::models::{self, MediaInfo};
use crate::config::Scrobbler;
use parking_lot::Mutex;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use tokio::sync::broadcast;

#[cfg(any(target_os = "linux", target_os = "windows"))]
use futures::StreamExt;

#[derive(Clone, Debug)]
pub enum TrackUpdateEvent {
    NewTrack(MediaInfo),
    PlaybackStateChange(MediaInfo),
}

pub struct MediaCenter {
    last_track: Arc<Mutex<Option<MediaInfo>>>,
    track_tx: broadcast::Sender<TrackUpdateEvent>,
    scrobblers: Arc<Mutex<Vec<Scrobbler>>>,
    scrobbling_task_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
}

impl MediaCenter {
    pub fn set_scrobblers(&self, scrobblers: Vec<Scrobbler>) {
        let mut scrobblers_lock = self.scrobblers.lock();
        *scrobblers_lock = scrobblers;
    }
    pub fn get_scrobblers(&self) -> Vec<Scrobbler> {
        let scrobblers_lock = self.scrobblers.lock();
        scrobblers_lock.clone()
    }
    pub fn new(scrobblers: Vec<Scrobbler>) -> Self {
        let (tx, _) = broadcast::channel(1);
        MediaCenter {
            last_track: Arc::new(Mutex::new(None)),
            track_tx: tx,
            scrobblers: Arc::new(Mutex::new(scrobblers)),
            scrobbling_task_handle: Arc::new(Mutex::new(None)),
        }
    }

    pub fn get_rx(&self) -> broadcast::Receiver<TrackUpdateEvent> {
        self.track_tx.subscribe()
    }

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

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub fn start_media_poller(self: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            let now_playing = nowhear::MediaSourceBuilder::new().build().await.unwrap();
            let mut now_playing_stream = now_playing.event_stream().await.unwrap();
            let mut deezer_client = models::deezer_api::DeezerClient::new(100);
            while let Some(event) = now_playing_stream.next().await {
                let media_info = match event {
                    nowhear::MediaEvent::TrackChanged { player_name, track } => {
                        let player = now_playing.get_player(player_name).await.unwrap();
                        let media_info = MediaInfo {
                            title: Some(track.title),
                            album: track.album,
                            artist: Some(track.artist.join(", ")),
                            elapsed_time: None,
                            cover_artwork: track.art_url,
                            is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                            duration: track.duration.map(|t| t.as_secs() as u32),
                            isrc: None
                        };
                        let enriched_track = deezer_client.enrich_media_info(&media_info).await;
                        self.track_tx.send(TrackUpdateEvent::NewTrack(enriched_track.clone())).unwrap();
                        enriched_track
                    }
                    nowhear::MediaEvent::PositionChanged { player_name, position } => {
                        let player = now_playing.get_player(player_name).await.unwrap();
                        let track = player.current_track;
                        let media_info = if let Some(track) = track {
                            MediaInfo {
                                title: Some(track.title),
                                album: track.album,
                                artist: Some(track.artist.join(", ")),
                                elapsed_time: Some(position.as_secs() as u32),
                                cover_artwork: track.art_url,
                                is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                                duration: track.duration.map(|t| t.as_secs() as u32),
                                isrc: None
                            }
                        } else {
                            MediaInfo {
                                title: None,
                                album: None,
                                artist: None,
                                elapsed_time: None,
                                cover_artwork: None,
                                is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                                duration: None,
                                isrc: None
                            }
                        };
                        let enriched_track = deezer_client.enrich_media_info(&media_info).await;
                        self.track_tx.send(TrackUpdateEvent::PlaybackStateChange(enriched_track.clone())).unwrap();
                        enriched_track
                    }
                    nowhear::MediaEvent::StateChanged { player_name, state } => {
                        let player = now_playing.get_player(player_name).await.unwrap();
                        let track = player.current_track;
                        let media_info = if let Some(track) = track {
                            MediaInfo {
                                title: Some(track.title),
                                album: track.album,
                                artist: Some(track.artist.join(", ")),
                                elapsed_time: player.position.map(|p| p.as_secs() as u32),
                                cover_artwork: track.art_url,
                                is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                                duration: track.duration.map(|d| d.as_secs() as u32),
                                isrc: None
                            }
                        } else {
                            MediaInfo {
                                title: None,
                                album: None,
                                artist: None,
                                elapsed_time: None,
                                cover_artwork: None,
                                is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                                duration: None,
                                isrc: None
                            }
                        };
                        let enriched_track = deezer_client.enrich_media_info(&media_info).await;
                        self.track_tx.send(TrackUpdateEvent::PlaybackStateChange(enriched_track.clone())).unwrap();
                        enriched_track
                    }
                    _ => continue
                };

                // asynchronous enriching of media info with Deezer API
                // let media_info_clone = media_info.clone();

                {
                    let mut last_track = self.last_track.lock();
                    *last_track = Some(media_info.clone());
                }

                self.track_tx.send(TrackUpdateEvent::NewTrack(media_info.clone())).unwrap();
            }
        });
    }

    #[cfg(target_os = "macos")]
    pub fn start_media_poller(self: Arc<Self>) {
        println!("starting media poller");
        let tx = self.track_tx.clone();
        tauri::async_runtime::spawn(async move {
            let mut deezer_client = models::deezer_api::DeezerClient::new(100);
            let now_playing = media_remote::NowPlayingPerl::new();
            loop {
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
                    duration: media.duration.map(|t| t as u32),
                    isrc: None
                };

                // asynchronous enriching of media info with Deezer API
                let media_info_clone = media_info.clone();
                let enriched_track = deezer_client.enrich_media_info(&media_info_clone).await;

                if !Self::should_broadcast_track(self.last_track.lock().as_ref(), &enriched_track) {
                    tx.send(TrackUpdateEvent::PlaybackStateChange(enriched_track))
                    .unwrap();
                    continue;
                }
                {
                    {
                        let mut last_track = self.last_track.lock();
                        *last_track = Some(enriched_track.clone());
                    }
                }

                tx.send(TrackUpdateEvent::NewTrack(enriched_track.clone()))
                    .unwrap();
            }
        });
    }

    pub fn start_scrobbling_task(self: Arc<Self>) {
        println!("starting scrobbling task");
        let scrobblers = self.get_scrobblers();
        let mut rx = self.get_rx();
        let mut task_guard = self.scrobbling_task_handle.lock();
        println!("spawning scrobbling task with {} scrobblers", scrobblers.len());
        *task_guard = Some(tauri::async_runtime::spawn(async move {
            let last_scrobble = Arc::new(Mutex::new(None::<MediaInfo>));
            loop {
                let event = match rx.recv().await {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    _ => continue,
                };
                let track = match event {
                    TrackUpdateEvent::NewTrack(track) => track,
                    TrackUpdateEvent::PlaybackStateChange(track) => track,
                };
                if track.elapsed_time.is_none() || track.duration.is_none() {
                    continue;
                }
                let last_track = last_scrobble.lock().clone();
                if track.elapsed_time.unwrap() > track.duration.unwrap() / 2 {
                    let already_scrobbled = if let Some(last_track) = last_track {
                        // if the last scrobbled track was over 50%, we already did now playing
                        last_track.title == track.title && last_track.album == track.album && (last_track.elapsed_time.unwrap() > last_track.duration.unwrap() / 2)
                    } else {false};

                    if already_scrobbled {
                        continue;
                    }
                    for scrobbler in &scrobblers {
                        scrobbler.scrobble(&track).await;
                    }
                } else {
                    let already_scrobbled = if let Some(last_track) = last_track {
                        // if the last scrobbled track was under 50%, we already did scrobble
                        last_track.title == track.title && last_track.album == track.album && (last_track.elapsed_time.unwrap() <= last_track.duration.unwrap() / 2)
                    } else {false};

                    if already_scrobbled {
                        continue;
                    }
                    for scrobbler in scrobblers.clone() {
                        let track = track.clone();
                        tauri::async_runtime::spawn(async move {scrobbler.now_playing(&track).await;});
                    }
                }
                last_scrobble.lock().replace(track.clone());
            }
        }));
    }

    fn sanitize_apple_music_album_name(album_name: &str) -> String {
        let patterns = [" - Single", " - EP"];

        let mut sanitized_name = album_name.to_string();
        for pattern in patterns {
            sanitized_name = sanitized_name.replace(pattern, "");
        }
        sanitized_name.trim().to_string()
    }
}
