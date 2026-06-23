use crate::config::Scrobbler;
use crate::models::{self, MediaInfo};
use arc_swap::{ArcSwap, ArcSwapOption};
#[cfg(target_os = "macos")]
use media_remote::Subscription;
use parking_lot::Mutex;
use std::ops::Deref;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::{sync::Notify, time::Duration};

#[cfg(any(target_os = "linux", target_os = "windows"))]
use futures::StreamExt;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use nowhear::MediaSource;

#[derive(Clone, Debug)]
pub enum TrackUpdateEvent {
    NewTrack(Arc<MediaInfo>),
    PlaybackStateChange(Arc<MediaInfo>),
    PositionChanged(Arc<MediaInfo>),
}

pub struct MediaCenter {
    last_track: ArcSwapOption<MediaInfo>,
    elapsed_offset: Arc<AtomicU32>,
    track_tx: watch::Sender<TrackUpdateEvent>,
    scrobblers: ArcSwap<Vec<Scrobbler>>,
    scrobbling_task_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    #[cfg(target_os = "macos")]
    macos_listener: Arc<Mutex<Option<media_remote::NowPlayingPerl>>>,
    deezer_client: Arc<models::deezer_api::DeezerClient>,

    play_state_notify: Arc<Notify>,
}

impl MediaCenter {
    pub fn set_scrobblers(&self, scrobblers: Vec<Scrobbler>) {
        self.scrobblers.store(Arc::new(scrobblers));
    }
    pub fn new(scrobblers: Vec<Scrobbler>) -> Self {
        let (tx, _) = watch::channel(TrackUpdateEvent::PlaybackStateChange(Arc::new(
            MediaInfo::default(),
        )));
        MediaCenter {
            last_track: ArcSwapOption::from(None),
            elapsed_offset: Arc::new(AtomicU32::new(0)),
            track_tx: tx,
            scrobblers: ArcSwap::new(Arc::new(scrobblers)),
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

    fn media_info_equal(previous: Option<&MediaInfo>, current: &MediaInfo) -> bool {
        // make sure it's even real
        let Some(previous) = previous else {
            return true;
        };

        // check metadata
        if previous.title != current.title
            || previous.artist != current.artist
            || previous.is_playing != current.is_playing
        {
            return true;
        }

        false
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub fn start_media_poller(self: Arc<Self>) {
        println!("starting media poller");
        let source_fut = nowhear::MediaSourceBuilder::new().build();
        let s = self.clone();
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
                let Some(media_info) = Self::build_media_info(&now_playing, &event).await else {
                    continue;
                };
                let player_name = match &event {
                    nowhear::MediaEvent::TrackChanged { player_name, .. }
                    | nowhear::MediaEvent::PositionChanged { player_name, .. }
                    | nowhear::MediaEvent::StateChanged { player_name, .. } => player_name,
                    _ => continue,
                };
                let enriched = self
                    .deezer_client
                    .enrich_media_info(
                        &media_info,
                        player_name.to_lowercase().contains("applemusic"),
                    )
                    .await;
                if !Self::should_broadcast_track(self.last_track.load_full().as_deref(), &enriched)
                {
                    self.track_tx
                        .send(TrackUpdateEvent::PlaybackStateChange(enriched))
                        .unwrap();
                    continue;
                }
                self.elapsed_offset.store(0, Ordering::Relaxed);
                self.last_track.store(Some(Arc::new(enriched.clone())));
                self.play_state_notify.notify_one();
                self.track_tx
                    .send(TrackUpdateEvent::NewTrack(enriched))
                    .unwrap();
            }
        });
        s.clone().start_position_ticker();
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    async fn build_media_info(
        now_playing: &impl nowhear::MediaSource,
        event: &nowhear::MediaEvent,
    ) -> Option<MediaInfo> {
        match event {
            nowhear::MediaEvent::TrackChanged { player_name, track } => {
                let track = track.clone();
                let player = now_playing.get_player(&player_name).await.ok()?;
                let artist = if player_name.to_lowercase().contains("applemusic") {
                    // apple music on windows puts the album in the artist field separated by an em dash, so we need to split it
                    println!(
                        "{} becomes {}",
                        track.artist[0],
                        track.artist[0].replace(" — ", " ")
                    );
                    vec![track.artist[0].replace(" — ", " ")]
                } else {
                    println!("player {}", player_name);
                    track.artist
                };
                Some(MediaInfo {
                    title: Some(track.title),
                    album: track
                        .album
                        .map(|album| Self::sanitize_apple_music_album_name(&album)),
                    artist: Some(artist.join(", ")),
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
                let player = now_playing.get_player(&player_name).await.ok()?;
                Some(match player.current_track {
                    Some(track) => {
                        let artist = if player_name.to_lowercase().contains("applemusic") {
                            // apple music on windows puts the album in the artist field separated by an em dash, so we need to split it
                            vec![track.artist[0].replace(" — ", " ")]
                        } else {
                            track.artist
                        };
                        MediaInfo {
                            title: Some(track.title),
                            album: track
                                .album
                                .map(|album| Self::sanitize_apple_music_album_name(&album)),
                            artist: Some(artist.join(", ")),
                            elapsed_time: Some(position.as_secs() as u32),
                            cover_artwork: track.art_url,
                            is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                            duration: track.duration.map(|t| t.as_secs() as u32),
                            isrc: None,
                        }
                    }
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
            nowhear::MediaEvent::StateChanged { player_name, .. } => {
                let player = now_playing.get_player(&player_name).await.ok()?;
                Some(match player.current_track {
                    Some(track) => {
                        let artist = if player_name.to_lowercase().contains("applemusic") {
                            // apple music on windows puts the album in the artist field separated by an em dash, so we need to split it
                            vec![track.artist[0].replace(" — ", " ")]
                        } else {
                            track.artist
                        };
                        MediaInfo {
                            title: Some(track.title),
                            album: track
                                .album
                                .map(|album| Self::sanitize_apple_music_album_name(&album)),
                            artist: Some(artist.join(", ")),
                            elapsed_time: player.position.map(|p| p.as_secs() as u32),
                            cover_artwork: track.art_url,
                            is_playing: player.playback_state == nowhear::PlaybackState::Playing,
                            duration: track.duration.map(|d| d.as_secs() as u32),
                            isrc: None,
                        }
                    }
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
        let last_track = self.last_track.load();
        let elapsed_offset = self.elapsed_offset.clone();
        let play_state = self.play_state_notify.clone();
        let tick = Duration::from_secs(5);

        tauri::async_runtime::spawn(async move {
            let mut is_playing = false;
            loop {
                if !is_playing {
                    play_state.notified().await;
                    is_playing = last_track.as_ref().is_some_and(|t| t.is_playing);
                    continue;
                }
                tokio::select! {
                    _ = tokio::time::sleep(tick) => {
                        let snapshot = last_track.clone();
                        let Some(base) = snapshot.as_ref() else {
                            is_playing = false;
                            continue;
                        };
                        if !base.is_playing {
                            is_playing = false;
                            continue;
                        }

                        elapsed_offset.fetch_add(tick.as_secs() as u32, Ordering::Relaxed);
                        let offset = elapsed_offset.load(Ordering::Relaxed);
                        let base_elapsed = base.elapsed_time.unwrap_or(0);
                        let effective = base_elapsed.saturating_add(offset);

                        let mut track = if snapshot.is_some() {
                            Arc::unwrap_or_clone(snapshot.unwrap())
                        } else {
                            is_playing = false;
                            continue;
                        };
                        track.elapsed_time = Some(effective);
                        let _ = tx.send(TrackUpdateEvent::PositionChanged(Arc::new(track)));
                    }
                    _ = play_state.notified() => {
                        // A media event arrived. Whatever it said is the new
                        // ground truth; reset our offset so the next tick
                        // adds on top of the OS-reported position.
                        elapsed_offset.store(0, Ordering::Relaxed);
                        is_playing = last_track.as_ref()
                            .is_some_and(|t| t.is_playing);
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
        let deezer_client = self.deezer_client.clone();
        let play_state_notify = self.play_state_notify.clone();
        let last_track = self.last_track.load_full();
        let inner_self = self.clone();

        now_playing.subscribe(move |event| {
            let event = event.clone();
            let tx = tx.clone();
            let deezer_client = deezer_client.clone();
            let play_state_notify = play_state_notify.clone();
            let last_track = last_track.clone();
            let inner_self = inner_self.clone();
            tauri::async_runtime::spawn(async move {
                let Some(media) = event else { return };
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
                let enriched_track = deezer_client
                    .enrich_media_info(&media_info_clone, false)
                    .await
                    .unwrap_or(media_info);

                if !Self::media_info_equal(last_track.as_deref(), &enriched_track) {
                    tx.send(TrackUpdateEvent::PlaybackStateChange(Arc::new(
                        enriched_track.clone(),
                    )))
                    .unwrap();
                } else {
                    tx.send(TrackUpdateEvent::NewTrack(Arc::new(enriched_track.clone())))
                        .unwrap();
                };
                inner_self.last_track.store(Some(Arc::new(enriched_track)));
                play_state_notify.notify_one();
            });
        });
        self.clone().start_position_ticker();
        println!("started position ticker");

        *self.macos_listener.lock() = Some(now_playing);
    }

    pub fn start_scrobbling_task(self: Arc<Self>) {
        println!("starting scrobbling task");
        let scrobblers = self.scrobblers.load_full();
        let mut rx = self.get_rx();
        let mut task_guard = self.scrobbling_task_handle.lock();
        if let Some(task_handle) = task_guard.take() {
            task_handle.abort();
        };
        println!(
            "spawning scrobbling task with {} scrobblers",
            scrobblers.len()
        );
        *task_guard = Some(tauri::async_runtime::spawn(async move {
            let scrobblers = scrobblers.clone();
            let mut last_scrobble: Option<MediaInfo> = None;
            loop {
                let event = match rx.changed().await {
                    Ok(()) => rx.borrow_and_update().clone(),
                    _ => break,
                };
                match event {
                    TrackUpdateEvent::NewTrack(track) => {
                        // when it's a new track, we do now playing
                        futures::future::join_all(
                            scrobblers
                                .iter()
                                .map(|scrobbler| scrobbler.now_playing(&track)),
                        )
                        .await;
                    }
                    TrackUpdateEvent::PositionChanged(track) => {
                        if track.elapsed_time.is_none() || track.duration.is_none() {
                            continue;
                        }
                        if track.elapsed_time.unwrap() > (track.duration.unwrap() / 2) {
                            let already_scrobbled = if let Some(last_track) = &last_scrobble {
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
                            futures::future::join_all(
                                scrobblers
                                    .iter()
                                    .map(|scrobbler| scrobbler.scrobble(&track)),
                            )
                            .await;
                        } else if last_scrobble.is_none() {
                            futures::future::join_all(
                                scrobblers
                                    .iter()
                                    .map(|scrobbler| scrobbler.now_playing(&track)),
                            )
                            .await;
                        }
                        last_scrobble.replace(track.deref().clone());
                    }
                    _ => {}
                };
            }
        }));
    }

    fn sanitize_apple_music_album_name(album_name: &str) -> String {
        let mut sanitized_name = album_name;
        sanitized_name = sanitized_name
            .strip_suffix(" - Single")
            .unwrap_or(sanitized_name);
        sanitized_name = sanitized_name
            .strip_suffix(" - EP")
            .unwrap_or(sanitized_name);
        sanitized_name.trim().to_string()
    }
}
