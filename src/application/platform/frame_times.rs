// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Record the frame times of the window while `RUNEBENDER_FRAME_TIMES` is set.
//!
//! The recorder is passive: it asks for no frame and changes no pacing, so the numbers
//! describe the session as the user works in it. `RUNEBENDER_FRAME_STATS` is the other
//! instrument: it renders every frame to measure what the display can deliver.
//!
//! The event loop in `launch.rs` reports each window event, the application driver reports
//! each run of the application logic, and the canvas widgets report their paint. Once a second
//! the recorder writes one line of means and worst cases, and it writes a line at once for any
//! single step that is slow. The footer shows the last line in short form.
//!
//! The stages, all in milliseconds:
//!
//! - `interval`: the time between the starts of two consecutive redraws.
//! - `redraw`: one redraw from start to end.
//!   This is the Masonry paint pass, the Vello render, the wait for the next drawable of the
//!   display, and the present call.
//!   With vertical synchronization the wait is most of it, and the pinned Xilem does not
//!   report the parts separately.
//! - `input`: the handling of one pointer, scroll or key event, with the logic it starts.
//! - `logic`: one run of the application logic and the view rebuild.
//! - `paint`: one paint of the editor canvas or of the glyph grid, which is part of a redraw.
//! - `latency`: the time from the start of the newest input event of a frame to the end of
//!   the redraw that shows it.
//!   The compositor and the display add their own delay after that.

use std::ffi::OsStr;
use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The environment variable that turns the recorder on: `1` for stderr, or a file path.
pub(crate) const ENV: &str = "RUNEBENDER_FRAME_TIMES";

/// An input, logic or paint step longer than this gets a line of its own.
const SLOW_STEP: Duration = Duration::from_millis(8);
/// A redraw longer than this gets a line of its own.
/// It is above one frame of a 30 Hz display, which a redraw fills when it waits for the
/// drawable.
const SLOW_REDRAW: Duration = Duration::from_millis(50);
/// A gap between redraws longer than this is idle time, not a frame interval.
const IDLE_GAP: Duration = Duration::from_millis(250);
/// The length of one report.
const REPORT: Duration = Duration::from_secs(1);

static ENABLED: AtomicBool = AtomicBool::new(false);
static RECORDER: Mutex<Option<Recorder>> = Mutex::new(None);

/// The count, the sum and the worst case of one stage in one report.
#[derive(Clone, Copy, Default)]
struct Span {
    count: u32,
    total: Duration,
    worst: Duration,
}

impl Span {
    fn add(&mut self, duration: Duration) {
        self.count += 1;
        self.total += duration;
        self.worst = self.worst.max(duration);
    }

    fn mean(self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            milliseconds(self.total) / f64::from(self.count)
        }
    }

    fn worst(self) -> f64 {
        milliseconds(self.worst)
    }
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e3
}

/// The stages of one report.
#[derive(Clone, Copy, Default)]
struct Stages {
    interval: Span,
    redraw: Span,
    input: Span,
    logic: Span,
    paint: Span,
    latency: Span,
}

impl Stages {
    fn is_empty(&self) -> bool {
        self.redraw.count + self.input.count + self.logic.count + self.paint.count == 0
    }
}

struct Recorder {
    sink: Box<dyn Write + Send>,
    launched: Instant,
    report_started: Instant,
    stages: Stages,
    last_redraw: Option<Instant>,
    newest_input: Option<Instant>,
    context: String,
    readout: Option<String>,
}

impl Recorder {
    fn new(mut sink: Box<dyn Write + Send>, now: Instant) -> Self {
        let _ = writeln!(
            sink,
            "# Runebender frame times, in milliseconds, as mean / worst for each second.\n\
             # interval: between redraws.  redraw: paint, render, wait for the display, present.\n\
             # input: one pointer, scroll or key event.  logic: one application logic run.\n\
             # paint: the editor canvas or the glyph grid.  latency: newest input to end of redraw."
        );
        Self {
            sink,
            launched: now,
            report_started: now,
            stages: Stages::default(),
            last_redraw: None,
            newest_input: None,
            context: String::new(),
            readout: None,
        }
    }

    fn seconds(&self, at: Instant) -> f64 {
        at.duration_since(self.launched).as_secs_f64()
    }

    /// Write the report that is open when it is a second old.
    fn roll(&mut self, now: Instant) {
        if now.duration_since(self.report_started) >= REPORT {
            self.write_report();
            self.report_started = now;
        }
    }

    fn write_report(&mut self) {
        let stages = std::mem::take(&mut self.stages);
        if stages.is_empty() {
            return;
        }
        let pair = |span: Span| format!("{:>5.1} / {:>5.1}", span.mean(), span.worst());
        let _ = writeln!(
            self.sink,
            "{:>8.1} s  frames {:>3}  interval {}  redraw {}  input {:>4} {}  logic {:>3} {}  \
             paint {}  latency {}  | {}",
            self.seconds(self.report_started),
            stages.redraw.count,
            pair(stages.interval),
            pair(stages.redraw),
            stages.input.count,
            pair(stages.input),
            stages.logic.count,
            pair(stages.logic),
            pair(stages.paint),
            pair(stages.latency),
            self.context,
        );
        let _ = self.sink.flush();
        if stages.redraw.count > 0 {
            self.readout = Some(format!(
                "{} frames, interval {:.1} ms, redraw {:.1} ms, logic {:.1} ms, paint {:.1} ms, \
                 latency {:.1} ms",
                stages.redraw.count,
                stages.interval.mean(),
                stages.redraw.mean(),
                stages.logic.worst(),
                stages.paint.worst(),
                stages.latency.mean(),
            ));
        }
    }

    fn slow(&mut self, name: &str, started: Instant, duration: Duration) {
        let _ = writeln!(
            self.sink,
            "{:>8.3} s  slow {name} {:.1}  | {}",
            self.seconds(started),
            milliseconds(duration),
            self.context,
        );
    }

    fn input(&mut self, kind: &'static str, started: Instant, now: Instant, user: bool) {
        self.roll(started);
        let duration = now.duration_since(started);
        self.stages.input.add(duration);
        if user {
            self.newest_input = Some(started);
        }
        if duration >= SLOW_STEP {
            self.slow(kind, started, duration);
        }
    }

    fn logic(&mut self, started: Instant, now: Instant) {
        let duration = now.duration_since(started);
        self.stages.logic.add(duration);
        if duration >= SLOW_STEP {
            self.slow("logic", started, duration);
        }
    }

    fn paint(&mut self, started: Instant, now: Instant) {
        let duration = now.duration_since(started);
        self.stages.paint.add(duration);
        if duration >= SLOW_STEP {
            self.slow("paint", started, duration);
        }
    }

    fn redraw(&mut self, started: Instant, now: Instant) {
        self.roll(started);
        let duration = now.duration_since(started);
        self.stages.redraw.add(duration);
        if let Some(last) = self.last_redraw.replace(started) {
            let interval = started.duration_since(last);
            if interval < IDLE_GAP {
                self.stages.interval.add(interval);
            }
        }
        if let Some(input) = self.newest_input.take() {
            self.stages.latency.add(now.duration_since(input));
        }
        if duration >= SLOW_REDRAW {
            self.slow("redraw", started, duration);
        }
    }
}

fn with(record: impl FnOnce(&mut Recorder)) {
    if let Ok(mut recorder) = RECORDER.lock()
        && let Some(recorder) = recorder.as_mut()
    {
        record(recorder);
    }
}

/// Turn the recorder on. `target` is `1` or empty for stderr, or the path of a file to replace.
pub(crate) fn enable(target: &OsStr) {
    let sink: Box<dyn Write + Send> = if target.is_empty() || target == "1" {
        Box::new(std::io::stderr())
    } else {
        match std::fs::File::create(target) {
            Ok(file) => Box::new(file),
            Err(error) => {
                eprintln!("{ENV}: {}: {error}", target.to_string_lossy());
                Box::new(std::io::stderr())
            }
        }
    };
    if let Ok(mut recorder) = RECORDER.lock() {
        *recorder = Some(Recorder::new(sink, Instant::now()));
        ENABLED.store(true, Ordering::Relaxed);
    }
}

/// The start of a step to time, or `None` while the recorder is off.
pub(crate) fn start() -> Option<Instant> {
    ENABLED.load(Ordering::Relaxed).then(Instant::now)
}

/// Record a window event other than a redraw. `user` marks pointer, scroll and key events.
pub(crate) fn input(kind: &'static str, started: Option<Instant>, user: bool) {
    if let Some(started) = started {
        with(|recorder| recorder.input(kind, started, Instant::now(), user));
    }
}

/// Record one run of the application logic and the view rebuild.
pub(crate) fn logic(started: Option<Instant>) {
    if let Some(started) = started {
        with(|recorder| recorder.logic(started, Instant::now()));
    }
}

/// Times one paint of a canvas widget, from its creation to its drop.
pub(crate) struct PaintTimer(Option<Instant>);

impl PaintTimer {
    /// Start the timer; it records nothing while the recorder is off.
    pub(crate) fn start() -> Self {
        Self(start())
    }
}

impl Drop for PaintTimer {
    fn drop(&mut self) {
        if let Some(started) = self.0 {
            with(|recorder| recorder.paint(started, Instant::now()));
        }
    }
}

/// Record one redraw of the window.
pub(crate) fn redraw(started: Option<Instant>) {
    if let Some(started) = started {
        with(|recorder| recorder.redraw(started, Instant::now()));
    }
}

/// Name what the user has open, for the lines that follow.
pub(crate) fn context(describe: impl FnOnce() -> String) {
    if ENABLED.load(Ordering::Relaxed) {
        let context = describe();
        with(|recorder| recorder.context = context);
    }
}

/// The last report in short form, for the footer; `None` while the recorder is off.
pub(crate) fn readout() -> Option<String> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    RECORDER
        .lock()
        .ok()?
        .as_ref()
        .map(|recorder| recorder.readout.clone().unwrap_or_default())
}

/// Write the report that is open, as the event loop ends.
pub(crate) fn finish() {
    with(Recorder::write_report);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Clone, Default)]
    struct Lines(Arc<Mutex<Vec<u8>>>);

    impl Write for Lines {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("the test owns the sink").extend(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Lines {
        fn text(&self) -> String {
            let text = String::from_utf8(self.0.lock().expect("the test owns the sink").clone())
                .expect("the recorder writes text");
            text.lines()
                .filter(|line| !line.starts_with('#'))
                .map(|line| format!("{line}\n"))
                .collect()
        }
    }

    fn recorder() -> (Recorder, Lines, Instant) {
        let lines = Lines::default();
        let now = Instant::now();
        (Recorder::new(Box::new(lines.clone()), now), lines, now)
    }

    fn ms(milliseconds: u64) -> Duration {
        Duration::from_millis(milliseconds)
    }

    #[test]
    fn a_second_of_frames_gives_one_line_and_a_readout() {
        let (mut recorder, lines, start) = recorder();
        recorder.context = "editor a Select".into();
        for frame in 0..3 {
            let at = start + ms(frame * 20);
            recorder.input("pointer move", at, at + ms(1), true);
            recorder.logic(at, at + ms(1));
            recorder.redraw(at + ms(2), at + ms(12));
        }
        assert_eq!(lines.text(), "", "a report is written when its second ends");
        recorder.redraw(start + ms(1100), start + ms(1110));
        let text = lines.text();
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(text.contains("frames   3"), "{text}");
        assert!(text.contains("interval  20.0 /  20.0"), "{text}");
        assert!(text.contains("redraw  10.0 /  10.0"), "{text}");
        assert!(text.contains("latency  12.0 /  12.0"), "{text}");
        assert!(text.ends_with("| editor a Select\n"), "{text}");
        let readout = recorder
            .readout
            .expect("a report with frames sets the readout");
        assert!(
            readout.starts_with("3 frames, interval 20.0 ms"),
            "{readout}"
        );
    }

    #[test]
    fn a_slow_step_gets_a_line_at_once() {
        let (mut recorder, lines, start) = recorder();
        recorder.logic(start, start + ms(3));
        assert_eq!(lines.text(), "");
        recorder.logic(start + ms(10), start + ms(31));
        recorder.input("key", start + ms(10), start + ms(32), true);
        recorder.redraw(start + ms(40), start + ms(70));
        recorder.redraw(start + ms(80), start + ms(140));
        let text = lines.text();
        let slow: Vec<&str> = text.lines().collect();
        assert_eq!(slow.len(), 3, "{text}");
        assert!(slow[0].contains("slow logic 21.0"), "{text}");
        assert!(slow[1].contains("slow key 22.0"), "{text}");
        assert!(slow[2].contains("slow redraw 60.0"), "{text}");
    }

    #[test]
    fn an_idle_gap_is_not_a_frame_interval() {
        let (mut recorder, _lines, start) = recorder();
        recorder.redraw(start, start + ms(5));
        recorder.redraw(start + ms(600), start + ms(605));
        assert_eq!(recorder.stages.interval.count, 0);
        recorder.redraw(start + ms(616), start + ms(621));
        assert_eq!(recorder.stages.interval.count, 1);
    }

    #[test]
    fn a_redraw_with_no_input_has_no_latency() {
        let (mut recorder, _lines, start) = recorder();
        recorder.input("resize", start, start + ms(1), false);
        recorder.redraw(start + ms(2), start + ms(4));
        assert_eq!(recorder.stages.latency.count, 0);
    }

    #[test]
    fn an_empty_second_writes_nothing() {
        let (mut recorder, lines, _start) = recorder();
        recorder.write_report();
        assert_eq!(lines.text(), "");
        assert!(recorder.readout.is_none());
    }
}
