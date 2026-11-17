// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reload when the sources change underneath us.
//!
//! A font project is edited by more than this program: a build script
//! writes a master, another editor saves, or a git checkout moves the tree.
//!
//! The pump is the same shape as the menu one, and for the same reason:
//! the events come from somewhere that is not winit's event loop, and
//! Xilem exposes no hook to drain them, so a `task` view does it and
//! posts the result back into the application.

use std::path::PathBuf;
use std::time::Duration;

use crate::application::workspace::Workspace;

/// Sent when the sources on disk have changed and settled.
#[derive(Debug)]
pub(crate) struct SourcesChanged;

/// Runs `view`, and alongside it watches the font's sources.
pub(crate) fn with_watch<V: xilem::WidgetView<Workspace>>(
    view: V,
    paths: Vec<PathBuf>,
) -> impl xilem::WidgetView<Workspace> + use<V> {
    use xilem::core::fork;
    use xilem::view::task_raw;

    fork(
        view,
        task_raw(
            move |proxy: xilem::core::MessageProxy<SourcesChanged>, _: &mut Workspace| {
                let paths = paths.clone();
                async move {
                    // The watcher's thread sends into an async channel, so this task sleeps
                    // until a file changes. It used to poll a blocking channel every 16 ms,
                    // which woke the process 60 times a second while idle.
                    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                    let mut watcher = match notify::recommended_watcher(
                        move |result: Result<notify::Event, notify::Error>| {
                            if result.is_ok() {
                                let _ = tx.send(());
                            }
                        },
                    ) {
                        Ok(watcher) => watcher,
                        Err(_) => return,
                    };
                    for path in &paths {
                        let _ = notify::Watcher::watch(
                            &mut watcher,
                            path,
                            notify::RecursiveMode::Recursive,
                        );
                    }
                    // A save is many file events. Wait for them to stop before saying
                    // anything, or one save reloads five times.
                    const SETTLE: Duration = Duration::from_millis(400);
                    loop {
                        if rx.recv().await.is_none() {
                            return;
                        }
                        // Something changed. Keep reading until the events stop for `SETTLE`.
                        loop {
                            match tokio::time::timeout(SETTLE, rx.recv()).await {
                                Ok(Some(())) => {}
                                Ok(None) => return,
                                Err(_) => break,
                            }
                        }
                        if proxy.message(SourcesChanged).is_err() {
                            return;
                        }
                    }
                }
            },
            |app: &mut Workspace, _: SourcesChanged| app.reload_from_disk(),
        ),
    )
}
