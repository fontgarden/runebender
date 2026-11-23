// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Private Unix socket mailbox. Only the editor thread executes queued font operations.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::{
    fs::DirBuilderExt,
    net::{UnixListener, UnixStream},
};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    mpsc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{
    agent::ToolCall,
    agent_cancellation::{
        AgentCancellationAdmission, AgentCancellationError, AgentCancellationIdentity,
        AgentCancellationOutcome, AgentCancellationRegistry,
    },
    agent_edit::{AgentEditRequest, results::AgentCancelResponse},
};
use serde::Deserialize;
use serde_json::Value;

/// Maximum serialized request or response frame, excluding the trailing newline.
pub const MAX_FRAME_BYTES: u64 = 8 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);
const STOP_POLL: Duration = Duration::from_millis(25);
const ACCEPT_RETRY: Duration = Duration::from_millis(20);
/// Maximum number of calls waiting for serialized application dispatch.
pub const MAX_PENDING_REQUESTS: usize = 16;
/// Maximum number of simultaneous socket connection handlers.
pub const MAX_LIVE_CONNECTIONS: usize = 32;
/// Maximum number of exact cancellation identities retained for one endpoint lifetime.
pub const CANCELLATION_CAPACITY: usize = 2048;
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Lists endpoint paths in this user's temporary directory, without contacting editors.
/// A crashed editor may leave a stale entry; a call to it fails rather than using disk.
pub fn sessions() -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(std::env::temp_dir())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("runebender-live-")
        })
        .map(|entry| entry.path().join("session.sock"))
        .filter(|path| path.exists())
        .collect();
    paths.sort();
    paths
}

/// How old a session directory must be before pruning may remove it.
/// A socket that refuses a connection in its first moments, or while its editor is closing,
/// belongs to a live editor; a socket left by a crash stays refused.
const PRUNE_MIN_AGE: Duration = Duration::from_secs(30);

/// Removes session directories whose socket no longer answers, which a
/// crashed or killed editor leaves behind. Returns how many were removed.
/// Only entries this user owns in the temporary directory are touched, and
/// a socket that merely times out is left alone in case its editor is busy.
/// A directory newer than `PRUNE_MIN_AGE` is left alone as well, so an editor that is
/// starting or closing at the same moment keeps its socket.
pub fn prune_stale() -> usize {
    prune(sessions(), PRUNE_MIN_AGE)
}

fn prune(paths: impl IntoIterator<Item = PathBuf>, min_age: Duration) -> usize {
    let mut removed = 0;
    for path in paths {
        let Some(directory) = path.parent() else {
            continue;
        };
        let old_enough = std::fs::metadata(directory)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= min_age);
        if !old_enough {
            continue;
        }
        let dead = match UnixStream::connect(&path) {
            Ok(_) => false,
            Err(error) => matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            ),
        };
        if dead && std::fs::remove_file(&path).is_ok() && std::fs::remove_dir(directory).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// A pending operation, with a deadline checked before the editor invokes it.
#[derive(Debug)]
pub struct Pending {
    call: ToolCall,
    epoch: String,
    deadline: Instant,
    reply: mpsc::Sender<Value>,
}

impl Pending {
    /// Executes a still-current request on the caller's thread and sends its result.
    /// Expired requests are dropped without invoking `handle`.
    pub fn respond(mut self, handle: impl FnOnce(&ToolCall) -> Value) {
        if Instant::now() < self.deadline {
            let expected = self.call.arguments.get("expected_document_epoch").cloned();
            // Strict application edit requests retain their required epoch in the typed payload.
            if !matches!(
                self.call.name.as_str(),
                "agent_apply"
                    | "agent_receipt"
                    | "agent_history"
                    | "review_proof"
                    | "proof_start"
                    | "proof_status"
                    | "proof_cancel"
                    | "proof_release"
                    | "graph_discover"
                    | "graph_snapshot"
                    | "graph_mutate"
                    | "graph_run"
                    | "graph_status"
                    | "graph_cancel"
                    | "graph_release"
                    | "graph_apply"
                    | "graph_image"
                    | "graph_trace"
                    | "graph_trace_status"
                    | "graph_trace_cancel"
                    | "graph_trace_release"
            ) && let Some(args) = self.call.arguments.as_object_mut()
            {
                args.remove("expected_document_epoch");
            }
            let mut result = if expected
                .as_ref()
                .is_some_and(|value| value.as_str() != Some(&self.epoch))
            {
                serde_json::json!({"ok":false,"error":"document epoch mismatch; reconnect and read the intended document", "error_code":"stale_document"})
            } else {
                handle(&self.call)
            };
            result["document_epoch"] = serde_json::json!(self.epoch);
            result["live_schema_version"] = serde_json::json!(1);
            result["server_version"] = serde_json::json!(env!("CARGO_PKG_VERSION"));
            let _ = self.reply.send(result);
        }
    }
}

/// An editor's socket and incoming queue. Dropping it stops accepting connections.
#[derive(Debug)]
pub struct Server {
    path: PathBuf,
    epoch: String,
    receiver: mpsc::Receiver<Pending>,
    pending: Arc<AtomicBool>,
    cancellations: AgentCancellationRegistry,
    stop: Arc<AtomicBool>,
    active_connections: Arc<AtomicUsize>,
    listener_worker: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    /// Creates a private directory and socket in the system temporary directory.
    /// No source data is written there. The path identifies this document lifetime.
    pub fn start() -> io::Result<Self> {
        // Each start tidies what earlier editors left; 376 dead sockets had
        // piled up for one live editor on 2026-10-17.
        let _ = prune_stale();
        let epoch = format!(
            "{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        use sha2::{Digest as _, Sha256};
        // macOS gives Unix sockets a short path budget, including the user temp root.
        // Exclusive directory creation rejects even an unlikely digest collision.
        let suffix = format!("{:x}", Sha256::digest(epoch.as_bytes()));
        let directory = std::env::temp_dir().join(format!("runebender-live-{}", &suffix[..16]));
        std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let path = directory.join("session.sock");
        let listener = match UnixListener::bind(&path) {
            Ok(listener) => listener,
            Err(error) => {
                let _ = std::fs::remove_dir(&directory);
                return Err(error);
            }
        };
        // The listener blocks in `accept`, so an idle editor does not wake to poll it.
        // Dropping the server connects once to wake it; see `Drop`.
        let (sender, receiver) = mpsc::sync_channel(MAX_PENDING_REQUESTS);
        let pending = Arc::new(AtomicBool::new(false));
        let cancellations = AgentCancellationRegistry::new(&epoch, CANCELLATION_CAPACITY)
            .map_err(io::Error::other)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker_epoch = epoch.clone();
        let worker_cancellations = cancellations.clone();
        let active_connections = Arc::new(AtomicUsize::new(0));
        let worker_connections = active_connections.clone();
        let worker_pending = pending.clone();
        let listener_worker = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if stopping.load(Ordering::Relaxed) {
                            break;
                        }
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                        // This listener is the only thread that increments the count;
                        // connection handlers can only decrease it between this check and admission.
                        if worker_connections.load(Ordering::Acquire) >= MAX_LIVE_CONNECTIONS {
                            let _ = write_frame(
                                &mut stream,
                                &serde_json::json!({"ok":false,"error_code":"connection_capacity",
                                    "error":"live connection capacity exhausted"}),
                            );
                            continue;
                        }
                        worker_connections.fetch_add(1, Ordering::AcqRel);
                        let sender = sender.clone();
                        let epoch = worker_epoch.clone();
                        let cancellations = worker_cancellations.clone();
                        let pending = worker_pending.clone();
                        let stop = stopping.clone();
                        let active = worker_connections.clone();
                        std::thread::spawn(move || {
                            answer(
                                &mut stream,
                                &sender,
                                &epoch,
                                &cancellations,
                                &pending,
                                &stop,
                            );
                            active.fetch_sub(1, Ordering::AcqRel);
                        });
                    }
                    // A failed accept, such as a connection reset before acceptance or a
                    // temporary descriptor shortage, does not close the endpoint.
                    // The short pause keeps a persistent failure from spinning.
                    Err(_) => std::thread::sleep(ACCEPT_RETRY),
                }
            }
        });
        Ok(Self {
            path,
            epoch,
            receiver,
            pending,
            cancellations,
            stop,
            active_connections,
            listener_worker: Some(listener_worker),
        })
    }

    /// The explicit endpoint clients pass to `--session`.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Opaque document lifetime used by request guards and response envelopes.
    /// Application receipts and proof jobs must retain this exact identity.
    pub fn document_epoch(&self) -> &str {
        &self.epoch
    }

    /// Shared cancellation state used by the application's pre-commit hook.
    pub fn cancellations(&self) -> AgentCancellationRegistry {
        self.cancellations.clone()
    }

    /// Shared signal set after a socket worker queues an application request.
    pub fn pending_signal(&self) -> Arc<AtomicBool> {
        self.pending.clone()
    }

    /// Takes the next request without blocking the UI thread.
    pub fn try_recv(&self) -> Option<Pending> {
        match self.receiver.try_recv() {
            Ok(request) => Some(request),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => {
                self.pending.store(false, Ordering::Release);
                None
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // The listener is blocked in `accept`; one connection wakes it to see the stop flag.
        // If the socket file is already gone the listener cannot be woken, so it is left
        // behind and ends with the process.
        let woken = UnixStream::connect(&self.path).is_ok();
        let _ = std::fs::remove_file(&self.path);
        if let Some(worker) = self.listener_worker.take()
            && woken
        {
            let _ = worker.join();
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.active_connections.load(Ordering::Acquire) != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

fn read_frame(stream: &mut UnixStream, limit: u64) -> io::Result<String> {
    let mut line = String::new();
    BufReader::new(stream.take(limit + 1)).read_line(&mut line)?;
    if line.len() as u64 > limit || !line.ends_with('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid or oversized frame",
        ));
    }
    Ok(line)
}

/// A socket request that ends without an editor result.
/// The connection handler sends it to the client as one structured error frame.
#[derive(Debug)]
struct LiveError {
    code: &'static str,
    message: String,
}

impl LiveError {
    fn new(code: &'static str, message: impl ToString) -> Self {
        Self {
            code,
            message: message.to_string(),
        }
    }

    fn into_value(self) -> Value {
        serde_json::json!({"ok":false,"error_code":self.code,"error":self.message})
    }
}

/// Serve one connection and write exactly one response frame, success or error.
/// A write error may occur after partial bytes, so it is not retried.
fn answer(
    stream: &mut UnixStream,
    sender: &mpsc::SyncSender<Pending>,
    epoch: &str,
    cancellations: &AgentCancellationRegistry,
    pending: &AtomicBool,
    stop: &AtomicBool,
) {
    let result = serve(stream, sender, epoch, cancellations, pending, stop)
        .unwrap_or_else(LiveError::into_value);
    let _ = write_envelope(stream, result, epoch);
}

fn invalid_arguments(error: impl ToString) -> Value {
    LiveError::new("invalid_arguments", error).into_value()
}

fn serve(
    stream: &mut UnixStream,
    sender: &mpsc::SyncSender<Pending>,
    epoch: &str,
    cancellations: &AgentCancellationRegistry,
    pending: &AtomicBool,
    stop: &AtomicBool,
) -> Result<Value, LiveError> {
    let frame = read_frame(stream, MAX_FRAME_BYTES)
        .map_err(|error| LiveError::new("invalid_request", error))?;
    let call: ToolCall =
        serde_json::from_str(&frame).map_err(|error| LiveError::new("invalid_request", error))?;
    if matches!(call.name.as_str(), "agent_reserve" | "agent_release") {
        let reserve = call.name == "agent_reserve";
        let request: AgentEditRequest = match serde_json::from_value(call.arguments) {
            Ok(request) => request,
            Err(error) => return Ok(invalid_arguments(error)),
        };
        let payload_digest = request.payload_digest();
        let identity = match AgentCancellationIdentity::new(
            request.expected_document_epoch,
            request.actor,
            request.operation_key,
        ) {
            Ok(identity) => identity,
            Err(error) => return Ok(invalid_arguments(error)),
        };
        let result = if identity.document_epoch() != epoch {
            serde_json::json!({"ok":false,"error_code":"stale_document",
                "error":"document epoch mismatch; reconnect explicitly"})
        } else if reserve {
            match cancellations.admit(&identity, payload_digest) {
                Ok(AgentCancellationAdmission::New) => {
                    serde_json::json!({"ok":true,"reservation_status":"new"})
                }
                Ok(AgentCancellationAdmission::Existing) => {
                    serde_json::json!({"ok":true,"reservation_status":"existing"})
                }
                Err(error) => cancellation_admission_error(error),
            }
        } else {
            match cancellations.release_unqueued(&identity, payload_digest) {
                Ok(()) => serde_json::json!({"ok":true,"reservation_status":"released"}),
                Err(error) => cancellation_admission_error(error),
            }
        };
        return Ok(result);
    }
    if call.name == "agent_cancel" {
        let request: CancellationRequest = match serde_json::from_value(call.arguments) {
            Ok(request) => request,
            Err(error) => return Ok(invalid_arguments(error)),
        };
        let identity = match AgentCancellationIdentity::new(
            request.expected_document_epoch,
            request.actor,
            request.operation_key,
        ) {
            Ok(identity) => identity,
            Err(error) => return Ok(invalid_arguments(error)),
        };
        let result = if identity.document_epoch() != epoch {
            serde_json::json!({"ok":false,"error_code":"stale_document",
                "error":"document epoch mismatch; reconnect explicitly"})
        } else {
            match cancellations.cancel(&identity) {
                Ok(outcome) => cancellation_result(outcome),
                Err(error) => cancellation_admission_error(error),
            }
        };
        return Ok(result);
    }

    let cancellation = if call.name == "agent_apply"
        && let Ok(request) = serde_json::from_value::<AgentEditRequest>(call.arguments.clone())
    {
        let payload_digest = request.payload_digest();
        let identity = match AgentCancellationIdentity::new(
            request.expected_document_epoch,
            request.actor,
            request.operation_key,
        ) {
            Ok(identity) => identity,
            Err(error) => return Ok(invalid_arguments(error)),
        };
        if identity.document_epoch() == epoch {
            let admission = match cancellations.admit(&identity, payload_digest) {
                Ok(admission) => admission,
                Err(error) => return Ok(cancellation_admission_error(error)),
            };
            Some((identity, payload_digest, admission))
        } else {
            // Preserve the application dispatch path for its structured stale-document response.
            None
        }
    } else {
        None
    };
    let (reply, receive) = mpsc::channel();
    if let Err(error) = sender.try_send(Pending {
        call,
        epoch: epoch.to_owned(),
        deadline: Instant::now() + TIMEOUT,
        reply,
    }) {
        if let Some((identity, payload_digest, AgentCancellationAdmission::New)) = &cancellation {
            let _ = cancellations.release_unqueued(identity, *payload_digest);
        }
        return Err(match error {
            mpsc::TrySendError::Full(_) => LiveError::new(
                "mailbox_full",
                format!(
                    "the editor already holds {MAX_PENDING_REQUESTS} waiting requests; retry later"
                ),
            ),
            mpsc::TrySendError::Disconnected(_) => {
                LiveError::new("editor_closed", "the editor stopped accepting requests")
            }
        });
    }
    pending.store(true, Ordering::Release);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match receive.recv_timeout(STOP_POLL) {
            Ok(result) => return Ok(result),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(LiveError::new(
                    "editor_closed",
                    "editor stopped before responding",
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) if stop.load(Ordering::Relaxed) => {
                return Err(LiveError::new(
                    "editor_closing",
                    "editor endpoint is shutting down",
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                return Err(LiveError::new(
                    "editor_timeout",
                    "editor did not respond; inspect receipts before retrying",
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn cancellation_admission_error(error: AgentCancellationError) -> Value {
    match error {
        AgentCancellationError::CapacityExhausted { .. } => {
            serde_json::json!({"ok":false,"error_code":"cancellation_capacity",
                "error":"cancellation registry capacity exhausted; existing operation state remains available"})
        }
        AgentCancellationError::PayloadMismatch => {
            serde_json::json!({"ok":false,"error_code":"payload_mismatch",
                "error":error.to_string()})
        }
        AgentCancellationError::StaleDocument => {
            serde_json::json!({"ok":false,"error_code":"stale_document",
                "error":error.to_string()})
        }
        AgentCancellationError::InvalidCapacity { .. } => {
            serde_json::json!({"ok":false,"error_code":"invalid_arguments",
                "error":error.to_string()})
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancellationRequest {
    expected_document_epoch: String,
    actor: String,
    operation_key: String,
}

fn cancellation_result(outcome: AgentCancellationOutcome) -> Value {
    serde_json::to_value(AgentCancelResponse::from(outcome))
        .expect("typed cancellation response serializes")
}

fn write_envelope(stream: &mut UnixStream, result: Value, epoch: &str) -> io::Result<()> {
    let envelope = |mut result: Value| {
        result["document_epoch"] = serde_json::json!(epoch);
        result["live_schema_version"] = serde_json::json!(1);
        result["server_version"] = serde_json::json!(env!("CARGO_PKG_VERSION"));
        result
    };
    // A response at or above the frame limit is replaced by an error frame before any byte
    // is written, so the client still receives one complete frame.
    let frame = match encode_frame(&envelope(result)) {
        Ok(frame) => frame,
        Err(error) => encode_frame(&envelope(
            LiveError::new("response_too_large", error).into_value(),
        ))?,
    };
    stream.write_all(&frame)
}

fn write_frame(stream: &mut UnixStream, value: &Value) -> io::Result<()> {
    stream.write_all(&encode_frame(value)?)
}

fn encode_frame(value: &Value) -> io::Result<Vec<u8>> {
    let mut frame = serde_json::to_vec(value)?;
    if frame.len() >= OUTPUT_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "response meets or exceeds the 8 MiB live frame limit",
        ));
    }
    frame.push(b'\n');
    Ok(frame)
}

/// Sends one bounded call to an explicit editor endpoint. Never falls back to disk.
pub fn call(path: &Path, call: &ToolCall) -> io::Result<Value> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(TIMEOUT + Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = serde_json::to_string(call)?;
    if request.len() as u64 >= MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "request too large",
        ));
    }
    stream.write_all(request.as_bytes())?;
    stream.write_all(b"\n")?;
    Ok(serde_json::from_str(&read_frame(
        &mut stream,
        OUTPUT_LIMIT as u64,
    )?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_requires_editor_dispatch_and_drop_removes_endpoint() {
        use std::os::unix::fs::PermissionsExt;
        let server = Server::start().unwrap();
        let path = server.path().to_path_buf();
        assert_eq!(
            std::fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let client_path = path.clone();
        let client = std::thread::spawn(move || {
            call(
                &client_path,
                &ToolCall {
                    name: "read_glyph".into(),
                    arguments: serde_json::json!({"glyph": "n"}),
                },
            )
            .unwrap()
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(pending) = server.try_recv() {
                pending.respond(
                    |request| serde_json::json!({"ok": true, "glyph": request.arguments["glyph"]}),
                );
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let result = client.join().unwrap();
        assert_eq!(result["glyph"], "n");
        assert_eq!(result["document_epoch"], server.document_epoch());
        drop(server);
        assert!(!path.exists());
    }

    #[test]
    fn pending_signal_tracks_queued_requests_until_the_mailbox_is_drained() {
        let server = Server::start().unwrap();
        let signal = server.pending_signal();
        assert!(!signal.load(Ordering::Acquire));
        let path = server.path().to_path_buf();
        let client = std::thread::spawn(move || {
            call(
                &path,
                &ToolCall {
                    name: "read_glyph".into(),
                    arguments: serde_json::json!({"glyph":"n"}),
                },
            )
            .unwrap()
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while !signal.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let pending = loop {
            if let Some(pending) = server.try_recv() {
                break pending;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        pending.respond(|_| serde_json::json!({"ok":true}));
        assert!(signal.load(Ordering::Acquire));
        assert!(server.try_recv().is_none());
        assert!(!signal.load(Ordering::Acquire));
        assert_eq!(client.join().unwrap()["ok"], true);
    }

    #[test]
    fn connection_capacity_is_reusable_after_a_handler_finishes() {
        let server = Server::start().unwrap();
        let mut clients = Vec::new();
        let mut requests = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        for _ in 0..MAX_LIVE_CONNECTIONS {
            let mut client = UnixStream::connect(server.path()).unwrap();
            client
                .write_all(b"{\"name\":\"font_info\",\"arguments\":{}}\n")
                .unwrap();
            let request = loop {
                if let Some(request) = server.try_recv() {
                    break request;
                }
                assert!(Instant::now() < deadline, "connection was not admitted");
                std::thread::sleep(Duration::from_millis(5));
            };
            clients.push(client);
            requests.push(request);
        }
        let mut rejected = UnixStream::connect(server.path()).unwrap();
        rejected
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let result: Value =
            serde_json::from_str(&read_frame(&mut rejected, MAX_FRAME_BYTES).unwrap()).unwrap();
        assert_eq!(result["error_code"], "connection_capacity");

        requests
            .pop()
            .unwrap()
            .respond(|_| serde_json::json!({"ok":true}));
        read_frame(&mut clients.pop().unwrap(), MAX_FRAME_BYTES).unwrap();
        while server.active_connections.load(Ordering::Acquire) == MAX_LIVE_CONNECTIONS {
            assert!(
                Instant::now() < deadline,
                "completed handler retained its slot"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let result = call(server.path(), &cancel_call(&server, "capacity", "unknown")).unwrap();
        assert_eq!(result["cancellation_status"], "unknown");
    }

    #[test]
    fn large_response_is_one_complete_bounded_frame() {
        let server = Server::start().unwrap();
        let path = server.path().to_path_buf();
        let client = std::thread::spawn(move || {
            call(
                &path,
                &ToolCall {
                    name: "proof_status".into(),
                    arguments: serde_json::json!({}),
                },
            )
            .unwrap()
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(pending) = server.try_recv() {
                pending.respond(|_| {
                    let image = "x\\\"\n".repeat(1024 * 1024);
                    serde_json::json!({"ok":true,"image":image})
                });
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            client.join().unwrap()["image"].as_str().unwrap().len(),
            4 * 1024 * 1024
        );
    }

    #[test]
    fn every_graph_tool_retains_its_required_epoch_through_dispatch() {
        for tool in super::super::agent_graph::tools() {
            let (reply, receive) = mpsc::channel();
            Pending {
                call: ToolCall {
                    name: tool.name.clone(),
                    arguments: serde_json::json!({"expected_document_epoch":"current"}),
                },
                epoch: "current".into(),
                deadline: Instant::now() + TIMEOUT,
                reply,
            }
            .respond(|call| {
                assert_eq!(
                    call.arguments["expected_document_epoch"], "current",
                    "{} must reach its strict application adapter with its epoch",
                    tool.name
                );
                serde_json::json!({"ok":true})
            });
            assert_eq!(receive.recv().unwrap()["ok"], true);
        }
    }

    #[test]
    fn mismatched_epoch_rejects_before_dispatch() {
        let (reply, receive) = mpsc::channel();
        Pending {
            call: ToolCall {
                name: "proposal_install".into(),
                arguments: serde_json::json!({"expected_document_epoch":"old"}),
            },
            epoch: "current".into(),
            deadline: Instant::now() + TIMEOUT,
            reply,
        }
        .respond(|_| panic!("stale document must never execute"));
        let result = receive.recv().unwrap();
        assert_eq!(result["error_code"], "stale_document");
        assert_eq!(result["document_epoch"], "current");
    }

    #[test]
    fn expired_request_never_executes() {
        let (reply, _) = mpsc::channel();
        Pending {
            call: ToolCall {
                name: "propose_edits".into(),
                arguments: Value::Null,
            },
            epoch: "expired".into(),
            deadline: Instant::now() - Duration::from_secs(1),
            reply,
        }
        .respond(|_| panic!("expired mutation must not run"));
    }

    fn apply_call(server: &Server, actor: &str, key: &str) -> ToolCall {
        ToolCall {
            name: "agent_apply".into(),
            arguments: serde_json::json!({
                "expected_document_epoch":server.document_epoch(),
                "actor":actor,
                "operation_key":key,
                "authorization":"user-approved",
                "source":0,
                "history_name":"socket routing fixture",
                "edits":[{
                    "target":{"glyph":"A","glyph_id":"fixture","layer":"public.default",
                        "expected_revision":"fixture"},
                    "operations":[{"op":"set_width","width":500.0}]
                }]
            }),
        }
    }

    fn cancel_call(server: &Server, actor: &str, key: &str) -> ToolCall {
        ToolCall {
            name: "agent_cancel".into(),
            arguments: serde_json::json!({
                "expected_document_epoch":server.document_epoch(),
                "actor":actor,
                "operation_key":key
            }),
        }
    }

    #[test]
    fn cancel_uses_an_independent_connection_while_apply_waits() {
        let server = Server::start().unwrap();
        let path = server.path().to_owned();
        let apply_request = apply_call(&server, "actor", "queued");
        let apply_path = path.clone();
        let apply = std::thread::spawn(move || call(&apply_path, &apply_request).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        let pending = loop {
            if let Some(pending) = server.try_recv() {
                break pending;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        let cancelled = call(&path, &cancel_call(&server, "actor", "queued")).unwrap();
        assert_eq!(cancelled["cancellation_status"], "prevented");
        let identity =
            AgentCancellationIdentity::new(server.document_epoch(), "actor", "queued").unwrap();
        assert_eq!(
            server.cancellations().claim_commit(&identity).unwrap(),
            super::super::agent_cancellation::AgentCommitClaim::Prevented
        );
        pending.respond(|_| serde_json::json!({"ok":false,"status":"cancelled"}));
        assert_eq!(apply.join().unwrap()["status"], "cancelled");
    }

    #[test]
    fn commit_claim_reports_too_late_then_committed_without_cross_actor_effects() {
        let server = Server::start().unwrap();
        let identity =
            AgentCancellationIdentity::new(server.document_epoch(), "actor", "race").unwrap();
        server
            .cancellations()
            .admit(
                &identity,
                super::super::agent_session::AgentPayloadDigest::sha256(b"race"),
            )
            .unwrap();
        assert_eq!(
            server.cancellations().claim_commit(&identity).unwrap(),
            super::super::agent_cancellation::AgentCommitClaim::Claimed
        );
        assert_eq!(
            call(server.path(), &cancel_call(&server, "actor", "race")).unwrap()["cancellation_status"],
            "too_late"
        );
        server
            .cancellations()
            .finish(
                &identity,
                super::super::agent_cancellation::AgentCancellationTerminal::Committed,
            )
            .unwrap();
        assert_eq!(
            call(server.path(), &cancel_call(&server, "actor", "race")).unwrap()["cancellation_status"],
            "committed"
        );
        assert_eq!(
            call(server.path(), &cancel_call(&server, "other", "race")).unwrap()["cancellation_status"],
            "unknown"
        );
    }

    #[test]
    fn dropping_server_interrupts_waiting_connection_and_removes_endpoint() {
        let server = Server::start().unwrap();
        let path = server.path().to_owned();
        let request = apply_call(&server, "actor", "shutdown");
        let client_path = path.clone();
        let client = std::thread::spawn(move || call(&client_path, &request));
        let deadline = Instant::now() + Duration::from_secs(5);
        let _pending = loop {
            if let Some(pending) = server.try_recv() {
                break pending;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        drop(server);
        let result = client.join().unwrap().unwrap();
        assert_eq!(result["ok"], false);
        assert!(
            matches!(
                result["error_code"].as_str(),
                Some("editor_closing" | "editor_closed")
            ),
            "{result}"
        );
        assert!(!path.exists());
    }

    #[test]
    fn malformed_request_receives_one_error_frame() {
        let server = Server::start().unwrap();
        let mut client = UnixStream::connect(server.path()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        client.write_all(b"{not json\n").unwrap();
        let result: Value =
            serde_json::from_str(&read_frame(&mut client, MAX_FRAME_BYTES).unwrap()).unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(result["error_code"], "invalid_request");
        assert_eq!(result["document_epoch"], server.document_epoch());
        assert!(server.try_recv().is_none());
    }

    #[test]
    fn full_mailbox_receives_one_error_frame() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let (reply, _) = mpsc::channel();
        sender
            .try_send(Pending {
                call: ToolCall {
                    name: "font_info".into(),
                    arguments: serde_json::json!({}),
                },
                epoch: "current".into(),
                deadline: Instant::now() + TIMEOUT,
                reply,
            })
            .unwrap();
        let cancellations = AgentCancellationRegistry::new("current", 4).unwrap();
        let (mut client, mut handler) = UnixStream::pair().unwrap();
        client
            .write_all(b"{\"name\":\"font_info\",\"arguments\":{}}\n")
            .unwrap();
        let pending = AtomicBool::new(false);
        answer(
            &mut handler,
            &sender,
            "current",
            &cancellations,
            &pending,
            &AtomicBool::new(false),
        );
        let result: Value =
            serde_json::from_str(&read_frame(&mut client, MAX_FRAME_BYTES).unwrap()).unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(result["error_code"], "mailbox_full");
        assert_eq!(result["document_epoch"], "current");
        assert!(!pending.load(Ordering::Acquire));
    }
}

#[cfg(test)]
mod prune_tests {
    use super::*;

    #[test]
    fn prune_removes_dead_sessions_and_keeps_live_ones() {
        let live = Server::start().expect("a live server");
        let live_path = live.path().to_path_buf();
        let dead_dir =
            std::env::temp_dir().join(format!("runebender-live-dead{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dead_dir);
        std::fs::create_dir(&dead_dir).unwrap();
        let dead_sock = dead_dir.join("session.sock");
        drop(UnixListener::bind(&dead_sock).unwrap());
        assert!(dead_sock.exists());
        // A new directory is too young for `prune_stale`, which protects editors that are
        // starting, so prune these two paths with no age limit.
        assert_eq!(
            prune([dead_sock.clone(), live_path.clone()], PRUNE_MIN_AGE),
            0
        );
        assert!(dead_sock.exists(), "a new session is not pruned");
        assert_eq!(
            prune([dead_sock.clone(), live_path.clone()], Duration::ZERO),
            1
        );
        assert!(!dead_sock.exists() && !dead_dir.exists());
        assert!(
            live_path.exists(),
            "a live editor's socket survives pruning"
        );
        drop(live);
    }
}
