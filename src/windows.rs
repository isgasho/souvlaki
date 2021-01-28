use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use raw_window_handle::windows::WindowsHandle;

use windows_bindings::windows;
use windows::foundation::TypedEventHandler;
use windows::win32::media_transport::ISystemMediaTransportControlsInterop;
use windows::win32::windows_and_messaging::HWND;
use windows::media::*;
use windows::{Abi, Interface};

use crate::{MediaControlEvent, MediaControls, MediaPlayer};

pub struct WindowsMediaControls {
    controls: SystemMediaTransportControls,
    display_updater: SystemMediaTransportControlsDisplayUpdater,
    events: Arc<RwLock<VecDeque<MediaControlEvent>>>,
}

#[repr(i32)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum WindowsMediaPlaybackStatus {
    Playing = 3,
    Paused = 4,
    // TODO: implement the rest, if necessary.
}

impl<S: MediaPlayer> MediaControls<S> for WindowsMediaControls {
    type Error = windows::Error;
    type Args = WindowsHandle;

    fn new(_: &S, args: Self::Args) -> windows::Result<Self> {
        let interop: ISystemMediaTransportControlsInterop =
            windows::factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>(
            )?;

        let mut smtc: Option<SystemMediaTransportControls> = None;
        unsafe {
            interop.GetForWindow(
                HWND(args.hwnd as isize),
                &SystemMediaTransportControls::IID as *const _,
                smtc.set_abi(),
            );
        }

        let controls = smtc.unwrap();
        controls.set_is_enabled(true)?;
        controls.set_is_play_enabled(true)?;
        controls.set_is_pause_enabled(true)?;
        controls.set_is_next_enabled(true)?;
        controls.set_is_previous_enabled(true)?;

        let display_updater = controls.display_updater()?;
        display_updater.set_type(MediaPlaybackType::Music)?;

        // FIXME: Maybe this could just be Rc instead of Arc.
        let events = Arc::new(RwLock::new(VecDeque::new()));
        let events2 = events.clone();

        let handler = TypedEventHandler::new(move |_, args: &Option<_>| {
            let args: &SystemMediaTransportControlsButtonPressedEventArgs = args.as_ref().unwrap();
            let button = args.button()?;
            let event = if button == SystemMediaTransportControlsButton::Play {
                MediaControlEvent::Play
            } else if button == SystemMediaTransportControlsButton::Pause {
                MediaControlEvent::Pause
            } else if button == SystemMediaTransportControlsButton::Next {
                MediaControlEvent::Next
            } else if button == SystemMediaTransportControlsButton::Previous {
                MediaControlEvent::Previous
            } else {
                unimplemented!()
            };

            events2.write().unwrap().push_back(event);
            Ok(())
        });

        controls.button_pressed(handler)?;

        Ok(Self {
            controls,
            display_updater,
            events,
        })
    }
    fn poll(&mut self, state: &mut S) {
        if let Ok(mut events) = self.events.try_write() {
            while let Some(event) = events.pop_front() {
                match dbg!(event) {
                    MediaControlEvent::Play => state.play(),
                    MediaControlEvent::Pause => state.pause(),
                    _ => unimplemented!(),
                }
            }
        }
        // TODO: optimize all of this so it only runs when necessary.
        // Update metadata
        let metadata = state.metadata();
        let properties = self.display_updater.music_properties().unwrap();

        properties.set_title(metadata.title).unwrap();
        properties.set_artist(metadata.artist).unwrap();
        properties.set_album_title(metadata.album).unwrap();
        self.display_updater.update().unwrap();

        // Updates playback status.
        let status = if state.playing() {
            WindowsMediaPlaybackStatus::Playing as i32
        } else {
            WindowsMediaPlaybackStatus::Paused as i32
        };
        self.controls
            .set_playback_status(MediaPlaybackStatus(status))
            .unwrap();
    }
}