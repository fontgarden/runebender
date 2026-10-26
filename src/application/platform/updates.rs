// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Native Sparkle UI and update installation, confined to the `AppKit` main thread.
//!
//! The signed bundle supplies the feed and public verification key.
//! Sparkle verifies and installs updates; Runebender protects the unsaved document
//! before allowing an update-driven restart. Headless commands never start it.

#[cfg(feature = "macos-updater")]
mod native {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use sparkle_updater::{MainThreadMarker, RelaunchContinuation, SparkleUpdater, UpdaterConfig};

    use crate::application::workspace::AppState;

    thread_local! {
        static STARTED: Cell<bool> = const { Cell::new(false) };
        static UPDATER: RefCell<Option<SparkleUpdater>> = const { RefCell::new(None) };
        static RESTART: RefCell<Option<RelaunchContinuation>> = const { RefCell::new(None) };
        static PROMPT: Cell<bool> = const { Cell::new(false) };
    }

    fn start() {
        let Some(main_thread) = MainThreadMarker::new() else {
            return;
        };
        if STARTED.replace(true) {
            return;
        }
        let config = UpdaterConfig {
            relaunch_handler: Some(Rc::new(|_, continuation| {
                // Do not open a modal prompt inside Sparkle's delegate callback.
                RESTART.with(|slot| *slot.borrow_mut() = Some(continuation));
                PROMPT.set(true);
            })),
            ..Default::default()
        };
        match SparkleUpdater::new(main_thread, config) {
            Ok(Some(updater)) => {
                // The plist supplies first-run defaults. Respect a user's later opt-out.
                if updater.automatically_checks_for_updates().unwrap_or(false)
                    && let Err(error) = updater.check_for_updates_in_background()
                {
                    eprintln!("Update check could not start: {error}");
                }
                UPDATER.with(|slot| *slot.borrow_mut() = Some(updater));
            }
            Ok(None) => {} // `cargo run` and standalone executables are not app bundles.
            Err(error) => eprintln!("Automatic updates are unavailable: {error}"),
        }
    }

    fn resume_restart(app: &mut AppState) {
        if !app.confirm_update_restart() {
            app.notice = Some(
                "Update postponed. Save your work, then choose Runebender > Check for Updates… to continue."
                    .into(),
            );
            return;
        }
        if let Some(main_thread) = MainThreadMarker::new()
            && let Some(continuation) = RESTART.with(|slot| slot.borrow_mut().take())
        {
            continuation.resume(main_thread);
        }
    }

    /// Start after the event loop begins and handle deferred restart prompts.
    pub(crate) fn poll(app: &mut AppState) {
        start();
        if PROMPT.replace(false) {
            resume_restart(app);
        }
    }

    /// Whether the menu can start a check or continue a postponed restart.
    pub(crate) fn can_check() -> bool {
        RESTART.with(|slot| slot.borrow().is_some())
            || UPDATER.with(|slot| {
                slot.borrow()
                    .as_ref()
                    .is_some_and(|updater| updater.can_check_for_updates().unwrap_or(false))
            })
    }

    /// Present Sparkle's native check UI, including offline and up-to-date results.
    pub(crate) fn check(app: &mut AppState) {
        start();
        if RESTART.with(|slot| slot.borrow().is_some()) {
            resume_restart(app);
            return;
        }
        UPDATER.with(|slot| {
            if let Some(updater) = slot.borrow().as_ref()
                && let Err(error) = updater.check_for_updates()
            {
                app.notice = Some(format!("Could not check for updates: {error}"));
            }
        });
    }
}

#[cfg(feature = "macos-updater")]
pub(crate) use native::{can_check, check, poll};

#[cfg(not(feature = "macos-updater"))]
pub(crate) fn can_check() -> bool {
    false
}

#[cfg(not(feature = "macos-updater"))]
pub(crate) fn poll(_app: &mut crate::application::workspace::AppState) {}

#[cfg(not(feature = "macos-updater"))]
pub(crate) fn check(_app: &mut crate::application::workspace::AppState) {}
