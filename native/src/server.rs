//! The UI's HTTP server, for this PC only: the page's own files (web.rs) and
//! the API routes (api.rs).

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Request};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;

async fn dispatch(ConnectInfo(peer): ConnectInfo<SocketAddr>, req: Request) -> Response {
    // ⚠ our app is for this PC only: nothing on the network may use it
    if !crate::auth::is_loopback(&peer.ip()) {
        return (StatusCode::FORBIDDEN, "LSP Converter only answers this PC").into_response();
    }
    let path = req.uri().path();
    let get = req.method() == Method::GET || req.method() == Method::HEAD;
    if get && !path.starts_with("/api/") {
        return crate::web::serve(path);
    }
    crate::api::handle(peer.ip(), req).await
}

pub async fn serve(listener: tokio::net::TcpListener) {
    let app = Router::new().fallback(dispatch).into_make_service_with_connect_info::<SocketAddr>();
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("http server stopped: {e}");
    }
}
