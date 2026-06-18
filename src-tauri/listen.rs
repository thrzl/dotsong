mod scrobble;
use media_remote::{NowPlayingInfo, NowPlayingPerl};
use objc2_core_foundation::CFRunLoop;
use std::{
    thread::{sleep, spawn},
    time::Duration,
};
use tray_icon::{
    TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, MenuItemKind},
};
use winit::{
    application::ApplicationHandler,
    event::StartCause,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

#[cfg(target_os = "macos")]
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};

#[derive(Debug)]
enum UserEvent {
    TrayIconEvent(TrayIconEvent),
    MenuEvent(MenuEvent),
    TrackEvent(NowPlayingInfo),
}

struct App {
    tray_icon: Option<TrayIcon>,
    menu: Option<Menu>,
    last_track_info: Option<NowPlayingInfo>,
}

impl App {
    fn make_menu() -> Menu {
        let menu = Menu::new();

        let quit_item = MenuItem::with_id("quit", "quit", true, None);
        let track_item = MenuItem::with_id("current-track", "loading...", false, None);
        menu.append(&track_item).unwrap();
        menu.append(&quit_item).unwrap();

        menu
    }

    fn new_tray_icon(&mut self) -> TrayIcon {
        let icon = load_icon();

        let menu = Self::make_menu();
        self.menu = Some(menu.clone());

        TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("dotsong")
            .with_icon(icon)
            .build()
            .unwrap()
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _id: WindowId,
        _event: winit::event::WindowEvent,
    ) {
    }

    // the icon is built here, once the loop is actually running, instead of in main()
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if cause == StartCause::Init {
            self.tray_icon = Some(self.new_tray_icon());

            let rl = CFRunLoop::main().unwrap();
            CFRunLoop::wake_up(&rl);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::MenuEvent(event) => {
                if event.id.0 == "quit" {
                    event_loop.exit();
                }
            }
            UserEvent::TrayIconEvent(_event) => {
                // right-click handling, hover state, etc. can go here
            }
            UserEvent::TrackEvent(track_info) => {
                if let Some(tray_icon) = &mut self.tray_icon {
                    if let Some(last_info) = &self.last_track_info {
                        if media_info_eq(&track_info, last_info) {
                            println!("track matches, not changing");
                            return;
                        }
                    }
                    self.last_track_info = Some(track_info.clone());
                    let title = track_info.title.unwrap_or("<none>".to_string());
                    let artist = track_info.artist.unwrap_or("<none>".to_string());
                    let text = format!("{} - {}", title, artist);
                    let menu = App::make_menu();
                    menu.items().iter().for_each(|item| match item {
                        MenuItemKind::MenuItem(row) => {
                            if row.id().0 == "current-track" {
                                row.set_text(text.clone());
                                return;
                            }
                        }
                        _ => {}
                    });
                    tray_icon.set_menu(Some(Box::new(menu)))
                }
            }
        }
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .with_activation_policy(ActivationPolicy::Accessory) // no dock icon
        .build()
        .unwrap();

    #[cfg(not(target_os = "macos"))]
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().unwrap();

    // forward tray/menu events into the winit event loop so they show up in user_event()
    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::TrayIconEvent(event));
    }));

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::MenuEvent(event));
    }));

    // your now-playing polling, unchanged — runs on its own thread,
    // doesn't need to touch the tray icon at all for this version
    let proxy = event_loop.create_proxy();

    spawn(move || {
        let now_playing = NowPlayingPerl::new();
        loop {
            sleep(Duration::from_millis(500));
            let Some(media) = now_playing.get_info().clone() else {
                continue;
            };
            if media.title.is_none() && media.album.is_none() {
                continue;
            }
            let _ = proxy.send_event(UserEvent::TrackEvent(media));
        }
    });

    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App {
        tray_icon: None,
        menu: None,
        last_track_info: None,
    };
    event_loop.run_app(&mut app).unwrap();
}

fn load_icon() -> tray_icon::Icon {
    let mut image = image::open("src/icon.png").unwrap();
    image.invert();
    let width = image.width();
    let height = image.height();
    tray_icon::Icon::from_rgba(image.into_rgba8().into_raw(), width, height).unwrap()
}

fn media_info_eq(a: &NowPlayingInfo, b: &NowPlayingInfo) -> bool {
    a.title == b.title && a.artist == b.artist && a.album == b.album
}
