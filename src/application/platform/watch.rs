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
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::application::workspace::Workspace;

/// Sent when the sources on disk have changed and settled.
#[derive(Debug)]
pub(crate) struct SourcesChanged;

/// What the watch task reads from its channel.
#[derive(Debug)]
enum Signal {
    /// A file under a watched path changed.
    Event,
    /// The open document has other source paths now.
    Paths(Vec<PathBuf>),
}

/// The paths the open document wants watched, and the channel of the running task.
///
/// Xilem starts the future of a task view once and never again, so the task cannot take its
/// paths from the view that made it: File > Open, New Font and Save As replace the document
/// inside the same view. Each rebuild states the wanted paths here, and a change reaches the
/// task as a [`Signal::Paths`].
#[derive(Debug, Default)]
struct Wanted {
    paths: Vec<PathBuf>,
    task: Option<UnboundedSender<Signal>>,
}

static WANTED: Mutex<Wanted> = Mutex::new(Wanted {
    paths: Vec::new(),
    task: None,
});

/// Record the paths of the open document, and tell the running task when they changed.
fn want(paths: &[PathBuf]) {
    let mut wanted = WANTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if wanted.paths == paths {
        return;
    }
    wanted.paths = paths.to_vec();
    if let Some(task) = &wanted.task {
        let _ = task.send(Signal::Paths(paths.to_vec()));
    }
}

/// Watch `paths`, follow each [`Signal::Paths`], and call `changed` when events have settled.
///
/// Returns when the channel closes or `changed` returns false.
async fn watch_sources(
    sender: UnboundedSender<Signal>,
    mut signals: UnboundedReceiver<Signal>,
    mut paths: Vec<PathBuf>,
    mut changed: impl FnMut() -> bool,
) {
    // The watcher's thread sends into an async channel, so this task sleeps
    // until a file changes. It used to poll a blocking channel every 16 ms,
    // which woke the process 60 times a second while idle.
    let mut watcher =
        match notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
            if result.is_ok() {
                let _ = sender.send(Signal::Event);
            }
        }) {
            Ok(watcher) => watcher,
            Err(_) => return,
        };
    let watch = |watcher: &mut notify::RecommendedWatcher, paths: &[PathBuf]| {
        for path in paths {
            // A path that does not exist yet, as for an untitled font, is not an error here.
            let _ = notify::Watcher::watch(watcher, path, notify::RecursiveMode::Recursive);
        }
    };
    let retarget =
        |watcher: &mut notify::RecommendedWatcher, paths: &mut Vec<PathBuf>, next: Vec<PathBuf>| {
            for path in paths.iter() {
                let _ = notify::Watcher::unwatch(watcher, path);
            }
            *paths = next;
            watch(watcher, paths);
        };
    watch(&mut watcher, &paths);
    // A save is many file events. Wait for them to stop before saying
    // anything, or one save reloads five times.
    const SETTLE: Duration = Duration::from_millis(400);
    loop {
        match signals.recv().await {
            None => return,
            Some(Signal::Paths(next)) => {
                retarget(&mut watcher, &mut paths, next);
                continue;
            }
            Some(Signal::Event) => {}
        }
        // Something changed. Keep reading until the events stop for `SETTLE`.
        loop {
            match tokio::time::timeout(SETTLE, signals.recv()).await {
                Ok(Some(Signal::Event)) => {}
                Ok(Some(Signal::Paths(next))) => retarget(&mut watcher, &mut paths, next),
                Ok(None) => return,
                Err(_) => break,
            }
        }
        if !changed() {
            return;
        }
    }
}

/// Runs `view`, and alongside it watches the sources of the open document.
pub(crate) fn with_watch<V: xilem::WidgetView<Workspace>>(
    view: V,
    paths: &[PathBuf],
) -> impl xilem::WidgetView<Workspace> + use<V> {
    use xilem::core::fork;
    use xilem::view::task_raw;

    want(paths);
    fork(
        view,
        task_raw(
            move |proxy: xilem::core::MessageProxy<SourcesChanged>, _: &mut Workspace| {
                let (sender, signals) = unbounded_channel();
                let paths = {
                    let mut wanted = WANTED
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    wanted.task = Some(sender.clone());
                    wanted.paths.clone()
                };
                watch_sources(sender, signals, paths, move || {
                    proxy.message(SourcesChanged).is_ok()
                })
            },
            |app: &mut Workspace, _: SourcesChanged| app.reload_from_disk(),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The task follows the document: after the paths change, a file under the new path is
    /// reported, where the task once kept the paths of the first document for its whole life.
    #[test]
    fn the_watch_follows_a_change_of_paths() {
        let root = std::env::temp_dir().join(format!(
            "runebender-watch-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let (first, second) = (root.join("first"), root.join("second"));
        std::fs::create_dir_all(&first).expect("first folder");
        std::fs::create_dir_all(&second).expect("second folder");
        // The watcher reports canonical paths; a symbolic link in the temp path is resolved.
        let first = first.canonicalize().expect("first path");
        let second = second.canonicalize().expect("second path");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("runtime");
        let (sender, signals) = unbounded_channel();
        let (seen_sender, seen) = std::sync::mpsc::channel();
        let control = sender.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            control
                .send(Signal::Paths(vec![second.clone()]))
                .expect("task is running");
            std::thread::sleep(Duration::from_millis(300));
            std::fs::write(second.join("changed.glif"), b"<glyph/>").expect("write");
            let reported = seen.recv_timeout(Duration::from_secs(20)).is_ok();
            // Dropping the last control sender does not end the task, because the watcher
            // holds one too, so the callback answer below ends it.
            reported
        });
        // The time limit ends the task when no change is reported, so a failure is an
        // assertion and not a test that never returns.
        runtime.block_on(async move {
            let task = watch_sources(sender, signals, vec![first], move || {
                let _ = seen_sender.send(());
                false
            });
            let _ = tokio::time::timeout(Duration::from_secs(25), task).await;
        });
        assert!(writer.join().expect("writer thread"), "the change is seen");
        let _ = std::fs::remove_dir_all(root);
    }
}
