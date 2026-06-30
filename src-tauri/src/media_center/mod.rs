mod source;

use crate::config::Scrobbler;
use crate::models::{self, MediaInfo};
use arc_swap::{ArcSwap, ArcSwapOption};
use parking_lot::{Mutex, RwLock};
use std::ops::Deref;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::{sync::Notify, time::Duration};

pub static BROWSERS: &[&str] = &["chrome", "firefox", "safari", "msedge", "brave", "vivaldi", "helium", "opera", "orion", "chromium"];

#[derive(Clone, Debug)]
pub enum TrackUpdateEvent {
    NewTrack(Arc<MediaInfo>),
    PlaybackStateChange(Arc<MediaInfo>),
    PositionChanged(Arc<MediaInfo>),
    /// every 5 seconds, even if the track hasn't changed, to update the elapsed time
    Tick(Arc<MediaInfo>),
}

pub struct MediaCenter {
    last_track: ArcSwapOption<MediaInfo>,
    elapsed_offset: Arc<AtomicU32>,
    track_tx: watch::Sender<TrackUpdateEvent>,
    scrobblers: ArcSwap<Vec<Scrobbler>>,
    scrobbling_task_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    media_source: Arc<dyn source::OsMediaSource>,
    deezer_client: Arc<models::deezer_api::DeezerClient>,
    config: Arc<RwLock<crate::config::Config>>,

    play_state_notify: Arc<Notify>,
}

impl MediaCenter {
    pub fn set_scrobblers(&self, scrobblers: Vec<Scrobbler>) {
        self.scrobblers.store(Arc::new(scrobblers));
    }
    pub fn new(scrobblers: Vec<Scrobbler>, config: Arc<RwLock<crate::config::Config>>) -> Self {
        let (tx, _) = watch::channel(TrackUpdateEvent::PlaybackStateChange(Arc::new(
            MediaInfo::default(),
        )));
        MediaCenter {
            last_track: ArcSwapOption::from(None),
            elapsed_offset: Arc::new(AtomicU32::new(0)),
            track_tx: tx,
            scrobblers: ArcSwap::new(Arc::new(scrobblers)),
            scrobbling_task_handle: Arc::new(Mutex::new(None)),
            media_source: source::create(),
            deezer_client: Arc::new(models::deezer_api::DeezerClient::new(100)),
            play_state_notify: Arc::new(Notify::new()),
            config,
        }
    }

    pub fn get_rx(&self) -> watch::Receiver<TrackUpdateEvent> {
        self.track_tx.subscribe()
    }

    fn media_info_equal(previous: Option<&MediaInfo>, current: &MediaInfo) -> bool {
        let Some(previous) = previous else {
            return false;
        };
        previous.title == current.title
            && previous.artist == current.artist
            && previous.is_playing == current.is_playing
            && current.elapsed_time.map(|d| d > 0).unwrap_or(false)
    }

    pub fn start_media_poller(self: Arc<Self>) {
        println!("starting media poller");
        let media_source = self.media_source.clone();
        let inner_self = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let Some(event) = media_source.next_event().await else {
                    continue;
                };
                inner_self.process_event(event).await;
            }
        });
        self.clone().start_position_ticker();
    }

    async fn process_event(self: &Arc<Self>, mut media_info: MediaInfo) {
        if media_info.title.as_ref().is_none_or(|t| t.is_empty()) && media_info.artist.as_ref().is_none_or(|a| a.is_empty()) {
            println!("ignoring event [empty]");
            return;
        }
        let mut useless_browser_track = false;
        if media_info.is_browser() {
            if !self.config.read().allow_browsers {
                println!("ignoring event [browser]");
                return
            }
            // if any of the following is true, we're good to continue:
            // 1. album is some and not empty
            // 2. the artist ends with "- Topic"
            if media_info.album.as_ref().is_none_or(|a| a.is_empty())
                && !media_info.artist.as_ref().is_some_and(|a| a.ends_with("- Topic"))
            {
                println!("ignoring event [browser topic]");
                useless_browser_track = true;
            }
        }
        media_info.artist = media_info.artist.as_ref().map(|t| t.trim_end_matches(" - Topic").to_string());

        if !media_info.is_playing {
            if let Some(track) = self.last_track.load().as_ref() {
                if track.title == media_info.title && track.artist == media_info.artist {
                    let mut paused = track.deref().clone();
                    paused.is_playing = false;
                    paused.elapsed_time = media_info.elapsed_time;
                    let paused_arc = Arc::new(paused);
                    self.last_track.store(Some(paused_arc.clone()));
                    self.track_tx
                        .send(TrackUpdateEvent::PlaybackStateChange(paused_arc))
                        .unwrap();
                }
            }
        }

        let last_track = self.last_track.load_full();
        let same_track = Self::media_info_equal(last_track.as_deref(), &media_info);

        let mut enriched = if same_track {
            let mut cached = last_track.as_ref().unwrap().deref().clone();
            cached.elapsed_time = media_info.elapsed_time;
            cached.duration = media_info.duration;
            cached.is_playing = media_info.is_playing;
            cached
        } else {
            self.deezer_client
                .enrich_media_info(&media_info)
                .await
                .unwrap_or(media_info.clone())
        };
        if useless_browser_track && enriched == media_info {
            println!("ignoring event [useless browser track]");
            return;
        }

        if !enriched
            .cover_artwork
            .as_ref()
            .is_some_and(|c| c.url().is_some())
        {
            enriched.cover_artwork = match enriched.cover_artwork.take() {
                Some(mut cover) if cover.bytes().is_some() => {
                    if !self.config.read().upload_cover_artwork {
                        None
                    } else {
                        match cover.upload_bytes().await {
                            Ok(_) => {
                                println!("uploaded cover artwork");
                                Some(cover)
                            }
                            Err(e) => {
                                println!("upload failed: {:?}", e);
                                None
                            }
                        }
                    }
                }
                _ => None,
            };
        }

        let is_same = Self::media_info_equal(last_track.as_ref().map(|v| &**v), &enriched);
        let play_state_changed =
            last_track.as_ref().map(|v| v.is_playing) != Some(enriched.is_playing);

        if is_same {
            if play_state_changed {
                self.track_tx
                    .send(TrackUpdateEvent::PlaybackStateChange(Arc::new(
                        enriched.clone(),
                    )))
                    .unwrap();
            } else {
                self.track_tx
                    .send(TrackUpdateEvent::PositionChanged(Arc::new(
                        enriched.clone(),
                    )))
                    .unwrap();
            }
        } else {
            self.elapsed_offset.store(0, Ordering::Relaxed);
            self.track_tx
                .send(TrackUpdateEvent::NewTrack(Arc::new(enriched.clone())))
                .unwrap();
        }
        self.last_track.store(Some(Arc::new(enriched)));
        self.play_state_notify.notify_one();
    }

    fn start_position_ticker(self: &Arc<Self>) {
        let tx = self.track_tx.clone();
        let elapsed_offset = self.elapsed_offset.clone();
        let play_state = self.play_state_notify.clone();
        let tick = Duration::from_secs(5);
        let inner_self = self.clone();

        tauri::async_runtime::spawn(async move {
            let mut is_playing = false;
            loop {
                if !is_playing {
                    play_state.notified().await;
                    let last_track = inner_self.last_track.load();
                    is_playing = last_track.as_ref().is_some_and(|t| t.is_playing);
                    continue;
                }
                tokio::select! {
                    _ = tokio::time::sleep(tick) => {
                        let snapshot = inner_self.last_track.load_full();
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
                        let _ = tx.send(TrackUpdateEvent::Tick(Arc::new(track)));
                    }
                    _ = play_state.notified() => {
                        elapsed_offset.store(0, Ordering::Relaxed);
                        is_playing = inner_self.last_track.load_full()
                            .is_some_and(|t| t.is_playing);
                    }
                }
            }
        });
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
                    TrackUpdateEvent::PositionChanged(track) | TrackUpdateEvent::Tick(track) => {
                        if track.elapsed_time.is_none() || track.duration.is_none() {
                            continue;
                        }
                        if track.elapsed_time.unwrap() > (track.duration.unwrap() / 2) {
                            let already_scrobbleed = if let Some(last_track) = &last_scrobble {
                                last_track.title == track.title
                                    && last_track.album == track.album
                                    && (last_track.elapsed_time.unwrap()
                                        > (last_track.duration.unwrap() / 2))
                            } else {
                                false
                            };

                            if already_scrobbleed {
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
}