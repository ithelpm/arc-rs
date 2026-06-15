use std::time::Duration;

use axum::{
    body::Body,
    http::{header, Request, Response, StatusCode},
    response::IntoResponse,
};
use bytes::Bytes;
use futures::StreamExt as _;

/// Proxies a full HTTP request to the upstream Jellyfin instance and streams
/// the response body back to the caller without buffering.
///
/// All request headers except `host` are forwarded. All response headers are
/// forwarded unchanged.
pub async fn proxy_to_jellyfin(
    http_client: &reqwest::Client,
    jellyfin_base: &str,
    req: Request<Body>,
) -> Response<Body> {
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");

    let target = format!("{}{}", jellyfin_base.trim_end_matches('/'), path_and_query);

    let method = req.method().clone();
    let req_headers = req.headers().clone();

    // Collect body bytes (needed for POST / non-streaming upstream calls)
    let body_bytes: Bytes = match axum::body::to_bytes(req.into_body(), 4 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("failed to read request body: {e}");
            Bytes::new()
        }
    };

    let mut builder = http_client.request(method, &target);

    for (name, value) in &req_headers {
        if name == header::HOST {
            continue; // reqwest sets the correct Host automatically
        }
        builder = builder.header(name, value);
    }

    if !body_bytes.is_empty() {
        builder = builder.body(body_bytes);
    }

    match builder.send().await {
        Ok(resp) => upstream_to_axum(resp).await,
        Err(e) => {
            tracing::error!("upstream proxy error: {e}");
            (StatusCode::BAD_GATEWAY, "upstream unreachable").into_response()
        }
    }
}

/// Proxies a Jellyfin video stream for at most `chunk_secs` seconds, then
/// terminates the connection. Used for per-second streaming billing.
///
/// `Range` and `Authorization` headers from the original request are forwarded
/// so the video player can seek correctly within a chunk.
pub async fn proxy_stream_chunk(
    http_client: &reqwest::Client,
    jellyfin_base: &str,
    item_id: &str,
    orig_headers: &axum::http::HeaderMap,
    chunk_secs: u64,
) -> Response<Body> {
    let target = format!(
        "{}/Videos/{}/stream",
        jellyfin_base.trim_end_matches('/'),
        item_id
    );

    let mut builder = http_client.get(&target);

    for (name, value) in orig_headers {
        let n = name.as_str();
        if n == "authorization" || n == "range" || n == "accept" || n == "x-emby-token" {
            builder = builder.header(name, value);
        }
    }

    let resp = match builder.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("jellyfin stream error: {e}");
            return (StatusCode::BAD_GATEWAY, "upstream unreachable").into_response();
        }
    };

    let status = resp.status();
    let resp_headers = resp.headers().clone();

    // Take bytes for chunk_secs duration then close
    let raw_stream = resp.bytes_stream();
    let deadline = tokio::time::sleep(Duration::from_secs(chunk_secs));
    let timed_stream = raw_stream.take_until(deadline);

    let body = Body::from_stream(timed_stream);

    let mut response = Response::builder().status(status.as_u16());

    for (name, value) in &resp_headers {
        let n = name.as_str();
        if n == "content-type"
            || n == "content-range"
            || n == "accept-ranges"
            || n == "transfer-encoding"
        {
            if let Ok(v) = value.to_str() {
                response = response.header(n, v);
            }
        }
    }

    response.body(body).unwrap_or_else(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "response build error").into_response()
    })
}

/// Converts a reqwest response into an axum response, streaming the body.
async fn upstream_to_axum(resp: reqwest::Response) -> Response<Body> {
    let status = resp.status();
    let headers = resp.headers().clone();
    let stream = resp.bytes_stream();
    let body = Body::from_stream(stream);

    let mut builder = Response::builder().status(status.as_u16());

    for (name, value) in &headers {
        if let Ok(v) = value.to_str() {
            builder = builder.header(name.as_str(), v);
        }
    }

    builder.body(body).unwrap_or_else(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "response build error").into_response()
    })
}
