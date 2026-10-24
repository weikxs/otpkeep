use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{BODY_LIMIT, test_router};
use crate::api::{Asset, Backend, Copied, EntryEdit, GateEntry, GateError};

const PORT: u16 = 4321;
const HOST: &str = "127.0.0.1:4321";
const ORIGIN: &str = "http://127.0.0.1:4321";

struct Demo {
    entries: Mutex<Vec<GateEntry>>,
    copied: Mutex<Vec<String>>,
}

fn demo_entry(id: &str, title: &str) -> GateEntry {
    GateEntry {
        id: id.to_owned(),
        title: title.to_owned(),
        issuer: "Example".to_owned(),
        account: "demo".to_owned(),
        note: String::new(),
        pinned: false,
        kind: "totp".to_owned(),
        shortcut: None,
        default_shortcut: None,
        code: Some("123456".to_owned()),
        next_code: None,
        period_s: Some(30),
        valid_until_ms: Some(1_020_000),
    }
}

impl Backend for Demo {
    fn entries(&self, query: &str) -> Result<Vec<GateEntry>, GateError> {
        let entries = self.entries.lock().unwrap();
        Ok(entries
            .iter()
            .filter(|e| e.title.to_lowercase().contains(&query.to_lowercase()))
            .cloned()
            .collect())
    }

    fn edit(&self, id: &str, edit: EntryEdit) -> Result<GateEntry, GateError> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or(GateError::NotFound)?;
        if edit.title.as_deref() == Some("") {
            return Err(GateError::Invalid("TITLE_EMPTY"));
        }
        if let Some(title) = edit.title {
            entry.title = title;
        }
        if let Some(note) = edit.note {
            entry.note = note;
        }
        Ok(entry.clone())
    }

    fn set_shortcut(&self, id: &str, shortcut: Option<&str>) -> Result<GateEntry, GateError> {
        if shortcut == Some("taken") {
            return Err(GateError::ShortcutTaken("Other".to_owned()));
        }
        let mut entries = self.entries.lock().unwrap();
        let entry = entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or(GateError::NotFound)?;
        entry.shortcut = shortcut.map(str::to_owned);
        Ok(entry.clone())
    }

    fn copy(&self, id: &str) -> Result<Copied, GateError> {
        if id == "hotp" {
            return Err(GateError::NoCodeYet);
        }
        self.entries
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.id == id)
            .ok_or(GateError::NotFound)?;
        self.copied.lock().unwrap().push(id.to_owned());
        Ok(Copied { clears_in_s: 30 })
    }

    fn asset(&self, path: &str) -> Option<Asset> {
        (path == "gate.html" || path == "assets/app.js").then(|| Asset {
            bytes: path.as_bytes().to_vec(),
            mime: "text/html".to_owned(),
        })
    }
}

struct Page {
    router: Router,
    link: String,
}

fn page() -> (Page, super::TestHandle) {
    let backend = Arc::new(Demo {
        entries: Mutex::new(vec![demo_entry("e1", "GitHub"), demo_entry("e2", "Mail")]),
        copied: Mutex::new(Vec::new()),
    });
    let (router, handle) = test_router(backend, Arc::new(|| 1_000_000), PORT);
    let link = handle.link();
    (Page { router, link }, handle)
}

async fn send(router: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn api(method: &str, uri: &str, session: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, HOST)
        .header(header::ORIGIN, ORIGIN);
    if let Some(session) = session {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {session}"));
    }
    match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn session(page: &Page) -> String {
    let (status, body) = send(
        &page.router,
        api(
            "POST",
            "/api/session",
            None,
            Some(json!({ "link": page.link })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    body["session"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn serves_the_page_without_a_token_and_with_safe_headers() {
    let (page, _) = page();
    let request = Request::get("/")
        .header(header::HOST, HOST)
        .body(Body::empty())
        .unwrap();

    let response = page.router.clone().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert!(
        headers[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("default-src 'self'")
    );
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
}

#[tokio::test]
async fn refuses_other_host_names() {
    let (page, _) = page();
    let request = Request::get("/")
        .header(header::HOST, "evil.example:4321")
        .body(Body::empty())
        .unwrap();

    let (status, body) = send(&page.router, request).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "HOST_REFUSED");
}

#[tokio::test]
async fn refuses_path_escapes_and_unknown_files() {
    let (page, _) = page();
    for uri in ["/assets/../secret", "/assets/missing.js"] {
        let request = Request::get(uri)
            .header(header::HOST, HOST)
            .body(Body::empty())
            .unwrap();
        let (status, _) = send(&page.router, request).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn api_needs_a_session() {
    let (page, _) = page();

    let (status, body) = send(&page.router, api("GET", "/api/entries", None, None)).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "NO_SESSION");
}

#[tokio::test]
async fn a_link_works_once() {
    let (page, _) = page();
    session(&page).await;

    let (status, body) = send(
        &page.router,
        api(
            "POST",
            "/api/session",
            None,
            Some(json!({ "link": page.link })),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "LINK_USED");
}

#[tokio::test]
async fn refuses_requests_from_other_pages() {
    let (page, _) = page();
    let session = session(&page).await;
    let request = Request::get("/api/entries")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, "http://evil.example")
        .header(header::AUTHORIZATION, format!("Bearer {session}"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = send(&page.router, request).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "ORIGIN_REFUSED");
}

#[tokio::test]
async fn changes_need_an_origin() {
    let (page, _) = page();
    let request = Request::post("/api/session")
        .header(header::HOST, HOST)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "link": page.link }).to_string()))
        .unwrap();

    let (status, _) = send(&page.router, request).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn lists_and_searches_entries() {
    let (page, _) = page();
    let session = session(&page).await;

    let (status, body) = send(
        &page.router,
        api("GET", "/api/entries?q=git", Some(&session), None),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entries"].as_array().unwrap().len(), 1);
    assert_eq!(body["entries"][0]["title"], "GitHub");
    // Codes are shown with the app's clock, so countdowns line up.
    assert_eq!(body["entries"][0]["code"], "123456");
    assert_eq!(body["entries"][0]["validUntilMs"], 1_020_000);
    assert_eq!(body["nowMs"], 1_000_000);
}

#[tokio::test]
async fn edits_names_and_notes() {
    let (page, _) = page();
    let session = session(&page).await;

    let (status, body) = send(
        &page.router,
        api(
            "PATCH",
            "/api/entries/e1",
            Some(&session),
            Some(json!({ "title": "GitHub work", "note": "备用码在抽屉里" })),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entry"]["title"], "GitHub work");
    assert_eq!(body["entry"]["note"], "备用码在抽屉里");
}

#[tokio::test]
async fn reports_refused_edits_with_a_code() {
    let (page, _) = page();
    let session = session(&page).await;

    let (status, body) = send(
        &page.router,
        api(
            "PATCH",
            "/api/entries/e1",
            Some(&session),
            Some(json!({ "title": "" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "TITLE_EMPTY");

    let (status, _) = send(
        &page.router,
        api(
            "PATCH",
            "/api/entries/nope",
            Some(&session),
            Some(json!({ "note": "x" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn refuses_unknown_fields() {
    let (page, _) = page();
    let session = session(&page).await;

    let (status, _) = send(
        &page.router,
        api(
            "PATCH",
            "/api/entries/e1",
            Some(&session),
            Some(json!({ "secret": "x" })),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn sets_and_clears_a_shortcut_and_names_the_holder_of_a_taken_one() {
    let (page, _) = page();
    let session = session(&page).await;

    let (status, body) = send(
        &page.router,
        api(
            "PUT",
            "/api/entries/e1/shortcut",
            Some(&session),
            Some(json!({ "shortcut": "control+alt+super+KeyG" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entry"]["shortcut"], "control+alt+super+KeyG");

    let (status, body) = send(
        &page.router,
        api(
            "PUT",
            "/api/entries/e2/shortcut",
            Some(&session),
            Some(json!({ "shortcut": "taken" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "SHORTCUT_TAKEN");
    assert_eq!(body["error"]["detail"], "Other");

    let (status, body) = send(
        &page.router,
        api(
            "PUT",
            "/api/entries/e1/shortcut",
            Some(&session),
            Some(json!({ "shortcut": null })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["entry"].get("shortcut").is_none());
}

#[tokio::test]
async fn copies_inside_the_app_without_sending_the_code() {
    let (page, _) = page();
    let session = session(&page).await;

    let (status, body) = send(
        &page.router,
        api("POST", "/api/entries/e1/copy", Some(&session), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "clearsInS": 30 }));

    let (status, body) = send(
        &page.router,
        api("POST", "/api/entries/hotp/copy", Some(&session), None),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "NO_CODE_YET");

    let (status, _) = send(
        &page.router,
        api("POST", "/api/entries/e1/copy", None, None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refuses_oversized_bodies() {
    let (page, _) = page();
    let session = session(&page).await;
    let note = "x".repeat(BODY_LIMIT + 1);

    let (status, _) = send(
        &page.router,
        api(
            "PATCH",
            "/api/entries/e1",
            Some(&session),
            Some(json!({ "note": note })),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn ending_the_session_refuses_later_requests() {
    let (page, _) = page();
    let session = session(&page).await;

    let (status, _) = send(
        &page.router,
        api("POST", "/api/heartbeat", Some(&session), None),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    send(&page.router, api("POST", "/api/end", Some(&session), None)).await;

    let (status, _) = send(
        &page.router,
        api("GET", "/api/entries", Some(&session), None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_real_server_listens_on_loopback_and_hands_out_links() {
    let backend = Arc::new(Demo {
        entries: Mutex::new(Vec::new()),
        copied: Mutex::new(Vec::new()),
    });
    let gate = super::RunningGate::start(backend, Arc::new(|| 1_000_000))
        .await
        .unwrap();

    let link = gate.new_link().unwrap().unwrap();

    assert!(link.starts_with(&format!("http://127.0.0.1:{}/#k=", gate.port())));
    gate.stop();
    assert!(gate.is_stopped());
    assert_eq!(gate.new_link().unwrap(), None);
}
