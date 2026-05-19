use std::time::Duration;

use gtk::glib::{self, ControlFlow, Priority, WeakRef};
use gtk::subclass::prelude::ObjectSubclassIsExt;

use crate::app::{self, get_label};
use crate::config::LyricDisplayMode;
use crate::log::*;
use crate::lyric_providers::{LyricLineOwned, LyricOwned};

use crate::sync::{LyricState, TrackState, LYRIC, TRACK_PLAYING_STATE};
use crate::utils::reset_lyric_labels;

pub fn register_lyric_display(app: WeakRef<app::Window>, interval: Duration) {
    glib::timeout_add_local_full(interval, Priority::HIGH, move || {
        let Some(window) = app.upgrade() else {
            return ControlFlow::Break;
        };

        let (paused, metainfo_not_found) = TRACK_PLAYING_STATE.with_borrow(
            |TrackState {
                 metainfo, paused, ..
             }| (*paused, metainfo.is_none()),
        );

        if metainfo_not_found {
            return ControlFlow::Continue;
        }

        trace!("refresh lyric with paused = {paused}");
        refresh_lyric(&window, paused);

        ControlFlow::Continue
    });
}

fn set_lyric_with_mode(
    window: &app::Window,
    translation: Option<&LyricLineOwned>,
    origin: Option<&LyricLineOwned>,
) {
    match window.imp().lyric_display_mode.get() {
        LyricDisplayMode::ShowBoth => {
            set_lyric(window, translation.or(origin), "above");
            set_lyric(window, translation.and(origin), "below");
        }
        LyricDisplayMode::ShowBothRev => {
            set_lyric(window, origin, "above");
            set_lyric(window, translation, "below");
        }
        LyricDisplayMode::Origin => {
            set_lyric(window, origin, "above");
            set_lyric(window, None, "below");
        }
        LyricDisplayMode::PreferTranslation => {
            set_lyric(window, translation.or(origin), "above");
            set_lyric(window, None, "below");
        }
    }
}

fn set_lyric(window: &app::Window, text: Option<&LyricLineOwned>, position: &str) {
    let text = text
        .map(|LyricLineOwned { text, .. }| text.as_str().trim())
        .unwrap_or_default();
if position == "above" || position == "below" {
        thread_local! {
            static CURRENT_ABOVE: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
            static CURRENT_BELOW: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
            static LAST_WRITTEN: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
        }

        if position == "above" {
            CURRENT_ABOVE.with(|a| *a.borrow_mut() = text.to_string());
        } else {
            CURRENT_BELOW.with(|b| *b.borrow_mut() = text.to_string());
        }

        let full_content = CURRENT_ABOVE.with(|a| {
            CURRENT_BELOW.with(|b| {
                format!("{}\n{}", a.borrow(), b.borrow())
            })
        });

        LAST_WRITTEN.with(|lw| {
            let mut lw_borrow = lw.borrow_mut();
            if *lw_borrow != full_content {
                *lw_borrow = full_content.clone();

                std::thread::spawn(move || {
                    let _ = std::fs::write("/tmp/me/current_waylyrics.txt", full_content);
                });
            }
        });
    }
    get_label(window, position).set_label(text);
}

pub fn refresh_lyric(window: &app::Window, paused: bool) {
    if paused {
        if !window.imp().show_lyric_on_pause.get() {
            reset_lyric_labels(window, Some(""));
        }
        return;
    }

    LYRIC.with_borrow(
        |LyricState {
             origin,
             translation,
         }| {
            let system_time = window.imp().lyric_start.get().unwrap();
            let elapsed = system_time.elapsed().ok();
            let Some(elapsed) = elapsed else {
                return;
            };
            match (origin, translation) {
                (
                    LyricOwned::LineTimestamp(origin_lyric),
                    LyricOwned::LineTimestamp(translation_lyric),
                ) => {
                    let translation =
                        crate::lyric_providers::utils::find_next_lyric(&elapsed, translation_lyric);
                    let origin =
                        crate::lyric_providers::utils::find_next_lyric(&elapsed, origin_lyric);
                    set_lyric_with_mode(window, translation, origin);
                }
                (LyricOwned::LineTimestamp(origin_lyric), _) => {
                    let origin =
                        crate::lyric_providers::utils::find_next_lyric(&elapsed, origin_lyric);
                    set_lyric_with_mode(window, None, origin);
                }
                _ => (),
            }
        },
    );
}