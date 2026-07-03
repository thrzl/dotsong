mod source;

use crate::config::Scrobbler;
use crate::models::{self, CoverArtwork, MediaInfo};
use arc_swap::{ArcSwap, ArcSwapOption};
use futures::FutureExt;
use parking_lot::{Mutex, RwLock};
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::{sync::Notify, time::Duration};
use unicase::UniCase;

pub static BROWSERS: &[&str] = &[
    "chrome", "firefox", "safari", "msedge", "brave", "vivaldi", "helium", "opera", "orion",
    "chromium",
];

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
        let title_matches = match (previous.title.as_ref(), current.title.as_ref()) {
            (Some(prev_title), Some(curr_title)) => {
                UniCase::new(prev_title) == UniCase::new(curr_title)
            }
            (None, None) => true,
            _ => false,
        };
        let artist_matches = match (previous.artist.as_ref(), current.artist.as_ref()) {
            (Some(prev_artist), Some(curr_artist)) => {
                UniCase::new(prev_artist) == UniCase::new(curr_artist)
            }
            (None, None) => true,
            _ => false,
        };
        title_matches && artist_matches && current.elapsed_time.is_some_and(|d| d > 0)
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
        if media_info.title.as_ref().is_none_or(|t| t.is_empty())
            && media_info.artist.as_ref().is_none_or(|a| a.is_empty())
        {
            println!("ignoring event [empty]");
            return;
        }
        if media_info.is_browser() && !self.config.read().allow_browsers {
            println!("ignoring event [browsers disabled]");
            return;
        }
        let last_track = self.last_track.load_full();
        if Self::media_info_equal(last_track.as_deref(), &media_info) {
            let last_track = last_track.unwrap();

            // take last_track, since it's enriched, and just replace the possibly new info
            // first, let's get the elapsed time and is_playing from the new media_info, since
            // those are the only things that can change without changing the track
            let elapsed_time = media_info.elapsed_time;
            let is_playing = media_info.is_playing;
            let last_track_is_playing = last_track.is_playing;

            let mut media_info = Arc::unwrap_or_clone(last_track);
            media_info.elapsed_time = elapsed_time;
            media_info.is_playing = is_playing;

            let media_info = Arc::new(media_info);

            if is_playing != last_track_is_playing {
                // if they are the same track, but the playback state changed, we still want to send an event
                println!(
                    "playback state changed: {} -> {}",
                    last_track_is_playing, is_playing
                );
                self.track_tx
                    .send(TrackUpdateEvent::PlaybackStateChange(media_info.clone()))
                    .ok();
                self.play_state_notify.notify_one();
            } else {
                // if they're the same track and the playback state didn't change, that means the position changed
                println!("position changed");
                self.track_tx
                    .send(TrackUpdateEvent::PositionChanged(media_info.clone()))
                    .ok();
                self.play_state_notify.notify_one();
            }
            self.last_track.store(Some(media_info));
            return;
        }
        media_info.artist = media_info
            .artist
            .map(|t| t.strip_suffix(" - Topic").unwrap_or(&t).to_string());
        let mut media_info = match self.deezer_client.enrich_media_info(&media_info).await {
            Some(info) => info,
            None => {
                if media_info.is_browser() {
                    println!("ignoring unmatched event [browser]");
                    return;
                }
                media_info
            }
        };
        println!(
            "new track: {} - {}",
            media_info.title(),
            media_info.artist()
        );
        // upload cover artwork to litterbox if its not there
        if let Some(cover_artwork) = &media_info.cover_artwork {
            if self.config.read().upload_cover_artwork
                && cover_artwork.url().is_none()
                && cover_artwork.bytes().is_some()
            {
                match cover_artwork.into_uploaded().await {
                    Ok(uploaded_artwork) => {
                        println!(
                            "uploaded cover artwork to litterbox: {}",
                            uploaded_artwork.url().unwrap_or_else(|| "unknown".into())
                        );
                        media_info.cover_artwork = Some(uploaded_artwork);
                    }
                    Err(e) => {
                        println!("failed to upload cover artwork to litterbox: {}", e);
                    }
                }
            }
        }
        let media_info = Arc::new(media_info);
        self.track_tx
            .send(TrackUpdateEvent::NewTrack(media_info.clone()))
            .and_then(|_| {
                // ONLY if the track send succeeds, cus if it doesn't we want to run it again:

                // 1. save the last track
                self.last_track.store(Some(media_info));
                self.play_state_notify.notify_one();
                Ok(())
            })
            .ok();
    }

    fn start_position_ticker(self: &Arc<Self>) {
        println!("starting position ticker");
        let tx = self.track_tx.clone();
        let play_state = self.play_state_notify.clone();
        let tick = Duration::from_secs(10);
        let inner_self = self.clone();

        tauri::async_runtime::spawn(async move {
            let mut is_playing = false;
            loop {
                if !is_playing {
                    println!("not playing, waiting for play state change");
                    play_state.notified().await;
                    let last_track = inner_self.last_track.load();
                    is_playing = last_track.as_ref().is_some_and(|t| t.is_playing);
                    continue;
                }
                let tick_future = {
                    let last_track = inner_self.last_track.load();
                    // return the remaining time until the next tick, or 5 seconds if the track is longer than that
                    last_track
                        .as_ref()
                        .map(|track| {
                            let elapsed = track.elapsed_time.unwrap_or(0);
                            let duration = track.duration.unwrap_or(0);
                            let remaining = (duration / 2).saturating_sub(elapsed);
                            println!("calculating next tick: elapsed = {}, duration = {}, remaining = {}", elapsed, duration, remaining);
                            if remaining > 0 {
                                tokio::time::sleep(Duration::from_secs(remaining as u64)).boxed()
                            } else {
                                futures::future::pending().boxed() // just wait forever if the track is over, since we don't want to send a tick for a finished track
                            }
                        })
                        .unwrap_or_else(|| tokio::time::sleep(tick).boxed())
                };
                tokio::select! {
                    _ = tick_future => {
                        let snapshot = inner_self.last_track.load();
                        let Some(track) = snapshot.as_ref() else {
                            is_playing = false;
                            continue;
                        };
                        if !track.is_playing {
                            is_playing = false;
                            continue;
                        }

                        let mut track = Arc::unwrap_or_clone(track.clone());
                        track.elapsed_time = track.duration.map(|duration| duration / 2);
                        let track = Arc::new(track);
                        inner_self.last_track.store(Some(track.clone()));
                        let _ = tx.send(TrackUpdateEvent::Tick(track));
                    }
                    _ = play_state.notified() => {
                        is_playing = inner_self.last_track.load().as_ref()
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
                        println!("scrobbling task received position change or tick event for track: {} - {}", track.title(), track.artist());
                        if track.elapsed_time.is_none() || track.duration.is_none() {
                            continue;
                        }
                        if track.elapsed_time.unwrap() >= (track.duration.unwrap() / 2) {
                            let already_scrobbleed = if let Some(last_track) = &last_scrobble {
                                Self::media_info_equal(last_scrobble.as_ref(), &track)
                                    && (last_track.elapsed_time.unwrap()
                                        >= (last_track.duration.unwrap() / 2))
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
