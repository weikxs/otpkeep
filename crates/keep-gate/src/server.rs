//! The page server: loopback only, one session, every API request authenticated (design/07-platform.md §2.5,
//! threats in design/08-security.md T3).

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::api::{Backend, EntryEdit, GateError, ShortcutChange};
use crate::session::{SessionError, Sessions};

/// Request bodies larger than this are refused.
pub const BODY_LIMIT: usize = 1024 * 1024;
/// How often the server checks whether it is still needed.
const IDLE_CHECK: Duration = Duration::from_secs(5);
const PAGE: &str = "gate.html";
const CSP: &str = "default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self'; \
                   script-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";

/// The clock, in Unix milliseconds.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

struct Shared {
    port: u16,
    backend: Arc<dyn Backend>,
    clock: Clock,
    state: Mutex<GateState>,
}

#[derive(Default)]
struct GateState {
    sessions: Sessions,
    stopped: bool,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }
}

/// A running page server. It stops by itself once no session is open and no link is waiting.
pub struct RunningGate {
    shared: Arc<Shared>,
    stop: watch::Sender<bool>,
}

impl RunningGate {
    /// Starts a server on a random loopback port.
    ///
    /// # Errors
    ///
    /// Returns the error from binding the port.
    pub async fn start(backend: Arc<dyn Backend>, clock: Clock) -> std::io::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared {
            port,
            backend,
            clock,
            state: Mutex::new(GateState::default()),
        });
        let (stop, mut stopped) = watch::channel(false);
        let app = router(Arc::clone(&shared));
        tokio::spawn(async move {
            let shutdown = async move {
                let _ = stopped.wait_for(|s| *s).await;
            };
            if let Err(error) = axum::serve(listener, app)
                .with_graceful_shutdown(shutdown)
                .await
            {
                tracing::warn!(%error, "browser page server stopped");
            }
        });
        tokio::spawn(stop_when_idle(Arc::clone(&shared), stop.clone()));
        tracing::info!("browser page server started");
        Ok(Self { shared, stop })
    }

    /// The port it listens on.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.shared.port
    }

    /// A new one-time link to open in the browser, or `None` when the server has already stopped.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Random`] when no token can be made.
    pub fn new_link(&self) -> Result<Option<String>, SessionError> {
        let now = self.shared.now();
        let mut state = self.shared.state();
        if state.stopped {
            return Ok(None);
        }
        let token = state.sessions.issue_link(now)?;
        Ok(Some(format!(
            "http://127.0.0.1:{}/#k={}",
            self.shared.port,
            token.as_str()
        )))
    }

    /// Ends the session and stops the server.
    pub fn stop(&self) {
        let mut state = self.shared.state();
        state.sessions.end();
        state.stopped = true;
        let _ = self.stop.send(true);
    }

    /// Whether it has stopped.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.shared.state().stopped
    }
}

async fn stop_when_idle(shared: Arc<Shared>, stop: watch::Sender<bool>) {
    let mut tick = tokio::time::interval(IDLE_CHECK);
    loop {
        tick.tick().await;
        let now = shared.now();
        let mut state = shared.state();
        if state.stopped || state.sessions.is_idle(now) {
            state.stopped = true;
            let _ = stop.send(true);
            tracing::info!("browser page server stopped: no session");
            return;
        }
    }
}

fn router(shared: Arc<Shared>) -> Router {
    let api = Router::new()
        .route("/api/session", post(open_session))
        .route("/api/heartbeat", post(heartbeat))
        .route("/api/end", post(end_session))
        .route("/api/entries", get(list_entries))
        .route("/api/entries/{id}", patch(edit_entry))
        .route("/api/entries/{id}/shortcut", put(set_shortcut))
        .route("/api/entries/{id}/copy", post(copy_code))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&shared),
            check_origin,
        ));
    Router::new()
        .route("/", get(page))
        .route("/assets/{*path}", get(asset))
        .merge(api)
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&shared),
            check_host,
        ))
        .layer(middleware::from_fn(safe_headers))
        .with_state(shared)
}

/// Only `127.0.0.1:<port>` and `localhost:<port>`: a page on another name that resolves here (DNS rebinding)
/// is refused.
async fn check_host(State(shared): State<Arc<Shared>>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok());
    if !host.is_some_and(|h| is_own(h, shared.port)) {
        return refuse(StatusCode::FORBIDDEN, "HOST_REFUSED");
    }
    next.run(request).await
}

/// API requests from another page are refused; requests that change something must say where they come from.
async fn check_origin(State(shared): State<Arc<Shared>>, request: Request, next: Next) -> Response {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .map(|o| o.to_str().unwrap_or_default());
    let allowed = match origin {
        Some(origin) => origin
            .strip_prefix("http://")
            .is_some_and(|host| is_own(host, shared.port)),
        None => request.method() == Method::GET,
    };
    if !allowed {
        return refuse(StatusCode::FORBIDDEN, "ORIGIN_REFUSED");
    }
    next.run(request).await
}

fn is_own(host: &str, port: u16) -> bool {
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

async fn safe_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in [
        (header::CACHE_CONTROL, "no-store"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::CONTENT_SECURITY_POLICY, CSP),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}

async fn page(State(shared): State<Arc<Shared>>) -> Response {
    serve_asset(&shared, PAGE)
}

async fn asset(State(shared): State<Arc<Shared>>, Path(path): Path<String>) -> Response {
    serve_asset(&shared, &format!("assets/{path}"))
}

fn serve_asset(shared: &Shared, path: &str) -> Response {
    if path.contains("..") {
        return refuse(StatusCode::NOT_FOUND, "NOT_FOUND");
    }
    match shared.backend.asset(path) {
        Some(asset) => {
            let mime = HeaderValue::from_str(&asset.mime)
                .unwrap_or(HeaderValue::from_static("application/octet-stream"));
            ([(header::CONTENT_TYPE, mime)], Body::from(asset.bytes)).into_response()
        }
        None => refuse(StatusCode::NOT_FOUND, "NOT_FOUND"),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenSession {
    link: String,
}

async fn open_session(
    State(shared): State<Arc<Shared>>,
    axum::Json(body): axum::Json<OpenSession>,
) -> Response {
    let now = shared.now();
    match shared.state().sessions.redeem(&body.link, now) {
        Ok(session) => axum::Json(json!({ "session": session.as_str() })).into_response(),
        Err(error) => session_refused(error),
    }
}

async fn heartbeat(State(shared): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    match authorize(&shared, &headers, false) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(response) => response,
    }
}

async fn end_session(State(shared): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize(&shared, &headers, false) {
        return response;
    }
    shared.state().sessions.end();
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct ListQuery {
    #[serde(default)]
    q: String,
}

async fn list_entries(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Response {
    if let Err(response) = authorize(&shared, &headers, true) {
        return response;
    }
    let backend = Arc::clone(&shared.backend);
    let now_ms = shared.now();
    // The app's clock goes along, so the page's countdowns match the codes.
    blocking(move || {
        backend
            .entries(&query.q)
            .map(|entries| json!({ "entries": entries, "nowMs": now_ms }))
    })
    .await
}

async fn edit_entry(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(edit): axum::Json<EntryEdit>,
) -> Response {
    if let Err(response) = authorize(&shared, &headers, true) {
        return response;
    }
    let backend = Arc::clone(&shared.backend);
    blocking(move || {
        backend
            .edit(&id, edit)
            .map(|entry| json!({ "entry": entry }))
    })
    .await
}

async fn set_shortcut(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(change): axum::Json<ShortcutChange>,
) -> Response {
    if let Err(response) = authorize(&shared, &headers, true) {
        return response;
    }
    let backend = Arc::clone(&shared.backend);
    blocking(move || {
        backend
            .set_shortcut(&id, change.shortcut.as_deref())
            .map(|entry| json!({ "entry": entry }))
    })
    .await
}

async fn copy_code(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(response) = authorize(&shared, &headers, true) {
        return response;
    }
    let backend = Arc::clone(&shared.backend);
    blocking(move || backend.copy(&id).map(|copied| json!(copied))).await
}

/// Runs a backend call off the async threads: it may wait for the vault and write files.
async fn blocking(
    call: impl FnOnce() -> Result<serde_json::Value, GateError> + Send + 'static,
) -> Response {
    match tokio::task::spawn_blocking(call).await {
        Ok(Ok(value)) => axum::Json(value).into_response(),
        Ok(Err(error)) => gate_error(&error),
        Err(_) => refuse(StatusCode::INTERNAL_SERVER_ERROR, "SAVE_FAILED"),
    }
}

#[allow(
    clippy::result_large_err,
    reason = "the error is the response itself, returned straight away"
)]
fn authorize(shared: &Shared, headers: &HeaderMap, is_action: bool) -> Result<(), Response> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .unwrap_or_default();
    let now = shared.now();
    shared
        .state()
        .sessions
        .check(bearer, now, is_action)
        .map_err(session_refused)
}

fn session_refused(error: SessionError) -> Response {
    let code = match error {
        SessionError::LinkUsed => "LINK_USED",
        SessionError::LinkExpired => "LINK_EXPIRED",
        SessionError::NoSession => "NO_SESSION",
        SessionError::Random => return refuse(StatusCode::INTERNAL_SERVER_ERROR, "RANDOM"),
    };
    refuse(StatusCode::UNAUTHORIZED, code)
}

fn gate_error(error: &GateError) -> Response {
    let status = match error {
        GateError::NotFound => StatusCode::NOT_FOUND,
        GateError::Invalid(_) | GateError::ShortcutInvalid => StatusCode::UNPROCESSABLE_ENTITY,
        GateError::ShortcutTaken(_)
        | GateError::ShortcutRejected
        | GateError::ReadOnly
        | GateError::NoCodeYet => StatusCode::CONFLICT,
        GateError::NeedsPassword => StatusCode::LOCKED,
        GateError::Save | GateError::Clipboard => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let detail = match error {
        GateError::ShortcutTaken(by) => Some(by.clone()),
        _ => None,
    };
    (
        status,
        axum::Json(json!({ "error": { "code": error.code(), "detail": detail } })),
    )
        .into_response()
}

fn refuse(status: StatusCode, code: &str) -> Response {
    (status, axum::Json(json!({ "error": { "code": code } }))).into_response()
}

#[cfg(test)]
pub(crate) fn test_router(
    backend: Arc<dyn Backend>,
    clock: Clock,
    port: u16,
) -> (Router, TestHandle) {
    let shared = Arc::new(Shared {
        port,
        backend,
        clock,
        state: Mutex::new(GateState::default()),
    });
    (router(Arc::clone(&shared)), TestHandle(shared))
}

/// Lets tests issue links without a real socket.
#[cfg(test)]
pub(crate) struct TestHandle(Arc<Shared>);

#[cfg(test)]
impl TestHandle {
    pub(crate) fn link(&self) -> String {
        let now = self.0.now();
        self.0
            .state()
            .sessions
            .issue_link(now)
            .unwrap()
            .as_str()
            .to_owned()
    }
}

#[cfg(test)]
mod tests;
