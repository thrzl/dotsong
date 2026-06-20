use crate::config::Scrobbler;
use crate::models::{self, MediaInfo};
#[cfg(target_os = "macos")]
use media_remote::Subscription;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::{sync::Notify, time::Duration};

#[cfg(any(target_os = "linux", target_os = "windows"))]
use futures::StreamExt;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use nowhear::MediaSource;

#[derive(Clone, Debug)]
pub enum TrackUpdateEvent {
    NewTrack(MediaInfo),
    PlaybackStateChange(MediaInfo),
    PositionChanged(MediaInfo),
}

pub struct MediaCenter {
    last_track: Arc<Mutex<Option<MediaInfo>>>,
    track_tx: watch::Sender<TrackUpdateEvent>,
    scrobblers: Arc<Mutex<Vec<Scrobbler>>>,
    scrobbling_task_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    #[cfg(target_os = "macos")]
    macos_listener: Arc<Mutex<Option<media_remote::NowPlayingPerl>>>,
    deezer_client: Arc<models::deezer_api::DeezerClient>,

    play_state_notify: Arc<Notify>,
}

impl MediaCenter {
    pub fn set_scrobblers(&self, scrobblers: Vec<Scrobbler>) {
        let mut scrobblers_lock = self.scrobblers.lock();
        *scrobblers_lock = scrobblers;
    }
    pub fn new(scrobblers: Vec<Scrobbler>) -> Self {
        let (tx, _) = watch::channel(TrackUpdateEvent::PlaybackStateChange(MediaInfo::default()));
        MediaCenter {
            last_track: Arc::new(Mutex::new(None)),
            track_tx: tx,
            scrobblers: Arc::new(Mutex::new(scrobblers)),
            scrobbling_task_handle: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "macos")]
            macos_listener: Arc::new(Mutex::new(None)),
            deezer_client: Arc::new(models::deezer_api::DeezerClient::new(100)),
            play_state_notify: Arc::new(Notify::new()),
        }
    }

    pub fn get_rx(&self) -> watch::Receiver<TrackUpdateEvent> {
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

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub fn start_media_poller(self: Arc<Self>) {
        let source_fut = nowhear::MediaSourceBuilder::new().build();
        tauri::async_runtime::spawn(async move {
            let now_playing = match source_fut.await {
                Ok(np) => np,
                Err(e) => {
                    eprintln!("failed to build nowhear media source: {e}");
                    return;
                }
            };
            let mut stream = match now_playing.event_stream().await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("failed to open nowhear event stream: {e}");
                    return;
                }
            };
            while let Some(event) = stream.next().await {
                let Some(media_info) = Self::build_media_info(&now_playing, event).await else {
                    continue;
                };
                let enriched = self.deezer_client.enrich_media_info(&media_info).await;
                self.last_track.lock().replace(enriched.clone());
                self.play_state_notify.notify_one();
                if !Self::should_broadcast_track(self.last_track.lock().as_ref(), &enriched) {
                    let _ = self
                        .track_tx
                        .send(TrackUpdateEvent::PlaybackStateChange(enriched));
                    continue;
                }
                *self.last_track.lock() = Some(enriched.clone());
                let _ = self.track_tx.send(TrackUpdateEvent::NewTrack(enriched));
            }
        });
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    async fn build_media_info(
        now_playing: &impl nowhear::MediaSource,
        event: nowhear::MediaEvent,
    ) -> Option<MediaInfo> {
        match event {
            nowhear::MediaEvent::TrackChanged { player_name, track } => {
                let player = now_playing.get_player(player_name).await.ok()?;
                Some(MediaInfo {
                    title: Some(track.title),
                    album: track.album,
                    artist: Some(track.artist.join(", ")),
                    elapsed_time: None,
                    cover_artwork: track.art_url,
                    is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                    duration: track.duration.map(|t| t.as_secs() as u32),
                    isrc: None,
                })
            }
            nowhear::MediaEvent::PositionChanged {
                player_name,
                position,
            } => {
                let player = now_playing.get_player(player_name).await.ok()?;
                Some(match player.current_track {
                    Some(track) => MediaInfo {
                        title: Some(track.title),
                        album: track.album,
                        artist: Some(track.artist.join(", ")),
                        elapsed_time: Some(position.as_secs() as u32),
                        cover_artwork: track.art_url,
                        is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                        duration: track.duration.map(|t| t.as_secs() as u32),
                        isrc: None,
                    },
                    None => MediaInfo {
                        title: None,
                        album: None,
                        artist: None,
                        elapsed_time: None,
                        cover_artwork: None,
                        is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                        duration: None,
                        isrc: None,
                    },
                })
            }
            nowhear::MediaEvent::StateChanged {
                player_name,
                state: _,
            } => {
                let player = now_playing.get_player(player_name).await.ok()?;
                Some(match player.current_track {
                    Some(track) => MediaInfo {
                        title: Some(track.title),
                        album: track.album,
                        artist: Some(track.artist.join(", ")),
                        elapsed_time: player.position.map(|p| p.as_secs() as u32),
                        cover_artwork: track.art_url,
                        is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                        duration: track.duration.map(|d| d.as_secs() as u32),
                        isrc: None,
                    },
                    None => MediaInfo {
                        title: None,
                        album: None,
                        artist: None,
                        elapsed_time: None,
                        cover_artwork: None,
                        is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                        duration: None,
                        isrc: None,
                    },
                })
            }
            _ => None,
        }
    }

    fn start_position_ticker(self: &Arc<Self>) {
        let tx = self.track_tx.clone();
        let last_track = self.last_track.clone();
        let play_state = self.play_state_notify.clone();
        let tick = Duration::from_secs(5);

        tauri::async_runtime::spawn(async move {
            let mut is_playing = false;
            loop {
                if !is_playing {
                    play_state.notified().await;
                    is_playing = last_track.lock().as_ref().is_some_and(|t| t.is_playing);
                    continue;
                }
                tokio::select! {
                    _ = tokio::time::sleep(tick) => {
                        let mut last_track_guard = last_track.lock();
                        if let Some(track) = last_track_guard.as_mut() {
                            if !track.is_playing { is_playing = false; continue; }

                            // if its playing then update the time
                            if let Some(elapsed_time) = track.elapsed_time.as_mut() {
                                *elapsed_time += tick.as_secs() as u32;
                            }
                            let snapshot = track.clone();
                            drop(last_track_guard);
                            let _ = tx.send(TrackUpdateEvent::PositionChanged(snapshot));
                        }
                    }
                    _ = play_state.notified() => {
                        is_playing = last_track.lock().as_ref().is_some_and(|track| track.is_playing);
                    }
                }
            }
        });
    }

    #[cfg(target_os = "macos")]
    pub fn start_media_poller(self: Arc<Self>) {
        println!("starting media poller");
        let tx = self.track_tx.clone();
        let now_playing = media_remote::NowPlayingPerl::new();
        let last_track_ptr = self.last_track.clone();
        let deezer_client = self.deezer_client.clone();
        let play_state_notify = self.play_state_notify.clone();

        now_playing.subscribe(move |event| {
            let event = event.clone();
            let last_track_ptr = last_track_ptr.clone();
            let tx = tx.clone();
            let deezer_client = deezer_client.clone();
            let play_state_notify = play_state_notify.clone();
            tauri::async_runtime::spawn(async move {
                let event = event.clone();
                let Some(media) = event.clone() else { return };
                if media.title.is_none() && media.album.is_none() {
                    return;
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
                    isrc: None,
                };

                // asynchronous enriching of media info with Deezer API
                let media_info_clone = media_info.clone();
                let enriched_track = deezer_client.enrich_media_info(&media_info_clone).await;

                if !Self::should_broadcast_track(last_track_ptr.lock().as_ref(), &enriched_track) {
                    tx.send(TrackUpdateEvent::PlaybackStateChange(
                        enriched_track.clone(),
                    ))
                    .unwrap();
                } else {
                    tx.send(TrackUpdateEvent::NewTrack(enriched_track.clone()))
                        .unwrap();
                };
                {
                    let mut last_track = last_track_ptr.lock();
                    *last_track = Some(enriched_track.clone());
                }
                play_state_notify.notify_one();
            });
        });
        self.clone().start_position_ticker();
        println!("started position ticker");

        *self.macos_listener.lock() = Some(now_playing);
    }

    pub fn start_scrobbling_task(self: Arc<Self>) {
        println!("starting scrobbling task");
        let scrobblers = self.scrobblers.clone();
        let mut rx = self.get_rx();
        let mut task_guard = self.scrobbling_task_handle.lock();
        println!(
            "spawning scrobbling task with {} scrobblers",
            scrobblers.lock().len()
        );
        *task_guard = Some(tauri::async_runtime::spawn(async move {
            let scrobblers = scrobblers.clone();
            let last_scrobble = Arc::new(Mutex::new(None::<MediaInfo>));
            loop {
                let scrobblers = scrobblers.lock().clone();
                let event = match rx.changed().await {
                    Ok(()) => rx.borrow_and_update().clone(),
                    _ => continue,
                };
                let last_track = last_scrobble.lock().clone();
                match event {
                    TrackUpdateEvent::NewTrack(track) => {
                        // when it's a new track, we do now playing
                        for scrobbler in &scrobblers.clone() {
                            scrobbler.now_playing(&track).await;
                        }
                    }
                    TrackUpdateEvent::PositionChanged(track) => {
                        if track.elapsed_time.is_none() || track.duration.is_none() {
                            continue;
                        }
                        if track.elapsed_time.unwrap() > (track.duration.unwrap() / 2) {
                            let already_scrobbled = if let Some(last_track) = last_track {
                                // if the last scrobbled track was over 50%, we already did now playing
                                last_track.title == track.title
                                    && last_track.album == track.album
                                    && (last_track.elapsed_time.unwrap()
                                        > (last_track.duration.unwrap() / 2))
                            } else {
                                false
                            };

                            if already_scrobbled {
                                continue;
                            }
                            for scrobbler in &scrobblers.clone() {
                                scrobbler.scrobble(&track).await;
                            }
                        } else if last_track.is_none() {
                            for scrobbler in &scrobblers.clone() {
                                scrobbler.now_playing(&track).await;
                            }
                        }
                        last_scrobble.lock().replace(track.clone());
                    }
                    _ => {}
                };
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
