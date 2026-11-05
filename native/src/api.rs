//! The API routes: what the Convert, Settings, Logs and About pages ask for.
//! server.rs has already refused anyone but this PC; every route here also
//! goes through the browser-forgery check (auth.rs) before it runs.

use std::net::IpAddr;

use axum::body::to_bytes;
use axum::extract::Request;
use axum::http::{header, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{json, Value};

use crate::{auth, config, detect};

pub fn json_resp(code: u16, v: &Value) -> Response {
    (
        StatusCode::from_u16(code).unwrap_or(StatusCode::OK),
        [(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "no-store")],
        serde_json::to_vec(v).unwrap_or_default(),
    )
        .into_response()
}

fn q_flag(query: &str, key: &str) -> bool {
    q_get(query, key).as_deref() == Some("1")
}

/// First value of `key` in a query string, percent-decoded.
pub fn q_get(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (pct(k) == key).then(|| pct(v))
    })
}

fn pct(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    Ok(x) => { out.push(x); i += 2; }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// An AC1 track's picture for the Convert page: none, the app converts cars.
fn track_image(_: &str, _: &str, _: bool) -> Option<std::path::PathBuf> {
    None
}

/// One dragged-in file, streamed into its drop staging folder. ⚠ Exactly
/// Content-Length bytes: an HTTP/1.1 browser keeps the connection open and
/// never sends EOF, and a short upload must not be handed on as the file.
async fn drop_part(req: Request, query: &str) -> Response {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let did = q_get(query, "id").unwrap_or_default();
    let name = q_get(query, "name").or_else(|| req.headers().get("X-Filename").and_then(|v| v.to_str().ok()).map(String::from)).unwrap_or_default();
    let size: u64 = req.headers().get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
    let (d2, n2) = (did.clone(), name.clone());
    let opened = tokio::task::spawn_blocking(move || crate::staging::drop_part_open(&d2, &n2)).await;
    let (dest, f) = match opened {
        Ok(Ok(x)) => x,
        Ok(Err(v)) => return json_resp(400, &v),
        Err(e) => return json_resp(500, &json!({"ok": false, "error": e.to_string()})),
    };
    let mut f = tokio::fs::File::from_std(f);
    let mut stream = req.into_body().into_data_stream();
    let mut wrote: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else { break };
        wrote += chunk.len() as u64;
        if wrote > crate::staging::DROP_LIMIT {
            drop(f);
            let _ = tokio::fs::remove_file(&dest).await;
            return json_resp(400, &json!({"ok": false, "error": "file is larger than 8 GB"}));
        }
        if let Err(e) = f.write_all(&chunk).await {
            return json_resp(400, &json!({"ok": false, "error": e.to_string()}));
        }
        if size != 0 && wrote >= size {
            break;
        }
    }
    let _ = f.flush().await;
    drop(f);
    if size != 0 && wrote != size {
        let _ = tokio::fs::remove_file(&dest).await;
        return json_resp(400, &json!({"ok": false, "error": format!("upload stopped early ({wrote} of {size} bytes)")}));
    }
    let shown = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    json_resp(200, &json!({"ok": true, "name": shown, "bytes": wrote, "size": if size != 0 { size } else { wrote }}))
}

/// The picture of an AC1 car or track on the Convert page (`thumb=1`: the
/// small one).
async fn preview(path: &str, query: &str) -> Response {
    let car = q_get(query, "car").unwrap_or_default();
    let track = path.ends_with("track_preview");
    let small = q_flag(query, "thumb");
    let (layout, outline) = (q_get(query, "layout").unwrap_or_default(), q_flag(query, "outline"));
    let hit = tokio::task::spawn_blocking(move || {
        let p = if track { track_image(&car, &layout, outline) } else { crate::convert::preview(&car) }?;
        if small { crate::convert::thumb(&p).or(Some(p)) } else { Some(p) }
    }).await.ok().flatten();
    match hit.and_then(|p| std::fs::read(&p).ok().map(|b| (p, b))) {
        Some((p, b)) => {
            let ct = if p.extension().map(|e| e.eq_ignore_ascii_case("png")).unwrap_or(false) { "image/png" } else { "image/jpeg" };
            ([(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, "max-age=3600")], b).into_response()
        }
        None => json_resp(404, &json!({"error": "no image"})),
    }
}

static SHOW: std::sync::OnceLock<Box<dyn Fn() -> bool + Send + Sync>> = std::sync::OnceLock::new();

/// How a second launch raises this instance's window (false: none to raise).
pub fn set_show(f: impl Fn() -> bool + Send + Sync + 'static) {
    let _ = SHOW.set(Box::new(f));
}

/// Run file-system work off the async threads.
async fn blocking<F: FnOnce() -> Value + Send + 'static>(f: F) -> Value {
    tokio::task::spawn_blocking(f).await.unwrap_or_else(|e| json!({"error": e.to_string()}))
}

/// Answer one API request.
pub async fn handle(peer: IpAddr, req: Request) -> Response {
    let path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();
    let write = req.method() == Method::POST;
    if !write && req.method() != Method::GET {
        return json_resp(404, &json!({"ok": false, "error": "not part of LSP Converter"}));
    }
    if !auth::browser_ok(write, &peer, req.headers()) {
        let (c, v) = auth::deny_browser();
        return json_resp(c, &v);
    }
    // a drag-drop STREAMS the file: never read it as a JSON body
    if write && path == "/api/drop/part" {
        return drop_part(req, &query).await;
    }
    if !write && (path == "/api/convert/preview" || path == "/api/convert/track_preview") {
        return preview(&path, &query).await;
    }
    let bytes = if write { to_bytes(req.into_body(), 16 << 20).await.unwrap_or_default() } else { Default::default() };
    let body: Value = if write {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}))
    } else {
        Value::Null
    };
    let bs = |k: &str| body.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match (write, path.as_str()) {
        (false, "/api/config") => json_resp(200, &config::public_cfg(&config::load())),
        (true, "/api/config") => match config::save(&body) {
            Ok(v) => json_resp(200, &v),
            Err(e) => json_resp(500, &json!({"error": format!("OSError: {e}")})),
        },
        (false, "/api/detect") => {
            let refresh = q_flag(&query, "refresh");
            json_resp(200, &blocking(move || detect::all_paths(refresh)).await)
        }
        (false, "/api/version") => json_resp(200, &json!({
            // ⚠ main.rs tells our own copy on the port by this name
            "name": crate::edition::NAME,
            "version": env!("CARGO_PKG_VERSION"),
            // the GPL's corresponding source: this release's tag in the public repository
            "source": format!("{}/tree/v{}", env!("CARGO_PKG_REPOSITORY"), env!("CARGO_PKG_VERSION")),
        })),
        (false, "/api/logs") => {
            let n = q_get(&query, "lines").and_then(|v| v.parse().ok()).unwrap_or(300);
            let f = q_get(&query, "file").unwrap_or_else(|| crate::logs::FILE.into());
            json_resp(200, &crate::logs::tail(n, &f))
        }
        (false, "/api/logs/files") => json_resp(200, &crate::logs::files()),
        (true, "/api/app/show") => {
            // a second launch asks the window owner to raise it, rather
            // than opening a rival webview on the same profile
            let ok = SHOW.get().map(|f| f()).unwrap_or(false);
            json_resp(200, &json!({"ok": ok}))
        }
        // ---- AC1 car conversion (convert.rs)
        (false, "/api/convert") => json_resp(200, &blocking(crate::convert::info).await),
        (false, "/api/convert/cars") => json_resp(200, &blocking(crate::convert::cars).await),
        (false, "/api/convert/status") => json_resp(200, &crate::convert::status()),
        // ⚠ opens a dialog on THIS machine: never for a remote client
        (true, "/api/pick_folder") if !auth::is_loopback(&peer) => json_resp(403, &json!({"ok": false, "error": "only on the PC running LSP Converter"})),
        (true, "/api/pick_folder") => {
            let start = bs("start");
            json_resp(200, &blocking(move || crate::convert::pick_folder(&start)).await)
        }
        // a link clicked in the window opens in the browser or mail app
        // (procs.rs): on THIS machine only, and only https or mailto
        (true, "/api/open") if !auth::is_loopback(&peer) => json_resp(403, &json!({"ok": false, "error": "only on the PC running LSP Converter"})),
        (true, "/api/open") => {
            let url = bs("url");
            if !crate::procs::link_ok(&url) {
                return json_resp(400, &json!({"ok": false, "error": "only web and email links open from the app"}));
            }
            json_resp(200, &json!({"ok": blocking(move || json!(crate::procs::open_url(url.trim()))).await == json!(true)}))
        }
        (true, "/api/convert/import") => { let id = bs("id"); json_resp(200, &blocking(move || crate::convert::import_dropped(&id)).await) }
        (true, "/api/convert/settings") => { let b = body.clone(); json_resp(200, &blocking(move || crate::convert::save_settings(&b)).await) }
        (true, "/api/convert/start") => { let b = body.clone(); json_resp(200, &blocking(move || crate::convert::start(&b)).await) }
        (true, "/api/convert/cancel") => json_resp(200, &crate::convert::cancel()),
        (true, "/api/convert/queue") => { let b = body.clone(); json_resp(200, &blocking(move || crate::convert::queue(&b)).await) }
        (true, "/api/convert/queue_clear") => json_resp(200, &crate::convert::queue_clear()),
        (true, "/api/convert/uninstall") => { let b = body.clone(); json_resp(200, &blocking(move || crate::convert::uninstall(&b)).await) }
        (false, "/api/convert/earlier") => {
            let (k, f) = (q_get(&query, "kind").unwrap_or_default(), q_get(&query, "folder").unwrap_or_default());
            json_resp(200, &blocking(move || crate::convert::earlier_api(&k, &f)).await)
        }
        // ⚠ no track conversion (installing one rewrites tables inside the
        // game's content.kspkg, LEGAL-REVIEW.md)
        (_, p) if p.starts_with("/api/convert/track") => json_resp(404, &json!({"ok": false,
            "error": "track conversion isn't part of LSP Converter: it converts cars"})),
        _ => json_resp(404, &json!({"ok": false, "error": "not part of LSP Converter"})),
    }
}
