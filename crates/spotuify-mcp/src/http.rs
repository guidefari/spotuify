//! Streamable HTTP transport for MCP.

use std::net::{IpAddr, SocketAddr};

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HOST, ORIGIN};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::Value;
use tokio::net::TcpListener;

use crate::{server::handle_request, RpcError, RpcRequest, RpcResponse};

type HeaderResult = Result<(), Box<Response>>;

#[derive(Clone)]
struct HttpState {
    token: String,
}

pub async fn serve(addr: SocketAddr) -> anyhow::Result<()> {
    ensure_loopback(addr)?;
    let token = std::env::var("SPOTUIFY_MCP_TOKEN")
        .map_err(|_| anyhow::anyhow!("SPOTUIFY_MCP_TOKEN is required for HTTP MCP transport"))?;
    let listener = TcpListener::bind(addr).await?;
    serve_listener(listener, token).await
}

async fn serve_listener(listener: TcpListener, token: String) -> anyhow::Result<()> {
    axum::serve(listener, app(token)).await?;
    Ok(())
}

fn app(token: String) -> Router {
    Router::new()
        .route("/mcp", post(post_mcp).get(get_mcp))
        .with_state(HttpState { token })
}

fn ensure_loopback(addr: SocketAddr) -> anyhow::Result<()> {
    match addr.ip() {
        IpAddr::V4(ip) if ip.is_loopback() => Ok(()),
        IpAddr::V6(ip) if ip.is_loopback() => Ok(()),
        _ => Err(anyhow::anyhow!(
            "MCP HTTP must bind to a loopback address; got {addr}"
        )),
    }
}

async fn get_mcp(State(state): State<HttpState>, headers: HeaderMap) -> Response {
    if let Err(response) = validate_headers(&state, &headers, false) {
        return *response;
    }

    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(axum::http::header::ALLOW, "POST")],
        "SSE is not supported by this endpoint",
    )
        .into_response()
}

async fn post_mcp(State(state): State<HttpState>, request: Request<Body>) -> Response {
    let (parts, body) = request.into_parts();
    if let Err(response) = validate_headers(&state, &parts.headers, true) {
        return *response;
    }

    let bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(err) => return json_error(StatusCode::BAD_REQUEST, None, format!("body: {err}")),
    };
    let request: RpcRequest = match serde_json::from_slice(&bytes) {
        Ok(request) => request,
        Err(err) => return json_error(StatusCode::BAD_REQUEST, None, format!("parse: {err}")),
    };

    let response = handle_request(request).await;
    (
        StatusCode::OK,
        [(CONTENT_TYPE, "application/json")],
        Json(response),
    )
        .into_response()
}

fn validate_headers(state: &HttpState, headers: &HeaderMap, require_accept: bool) -> HeaderResult {
    validate_auth(state, headers)?;
    validate_host(headers)?;
    validate_origin(headers)?;
    if require_accept {
        validate_accept(headers)?;
    }
    Ok(())
}

fn validate_auth(state: &HttpState, headers: &HeaderMap) -> HeaderResult {
    let expected = format!("Bearer {}", state.token);
    let actual = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if actual == Some(expected.as_str()) {
        return Ok(());
    }
    Err(Box::new(json_error(
        StatusCode::UNAUTHORIZED,
        None,
        "missing or invalid bearer token",
    )))
}

fn validate_host(headers: &HeaderMap) -> HeaderResult {
    let Some(host) = headers.get(HOST).and_then(|value| value.to_str().ok()) else {
        return Err(Box::new(json_error(
            StatusCode::FORBIDDEN,
            None,
            "invalid Host",
        )));
    };
    let Ok(url) = url::Url::parse(&format!("http://{host}/")) else {
        return Err(Box::new(json_error(
            StatusCode::FORBIDDEN,
            None,
            "invalid Host",
        )));
    };
    let allowed = matches!(
        url.host_str(),
        Some("127.0.0.1") | Some("localhost") | Some("::1")
    );
    if allowed {
        Ok(())
    } else {
        Err(Box::new(json_error(
            StatusCode::FORBIDDEN,
            None,
            "invalid Host",
        )))
    }
}

fn validate_origin(headers: &HeaderMap) -> HeaderResult {
    let Some(origin) = headers.get(ORIGIN).and_then(|value| value.to_str().ok()) else {
        return Ok(());
    };
    let Ok(url) = url::Url::parse(origin) else {
        return Err(Box::new(json_error(
            StatusCode::FORBIDDEN,
            None,
            "invalid Origin",
        )));
    };
    let allowed = matches!(
        url.host_str(),
        Some("127.0.0.1") | Some("localhost") | Some("::1")
    );
    if allowed {
        Ok(())
    } else {
        Err(Box::new(json_error(
            StatusCode::FORBIDDEN,
            None,
            "invalid Origin",
        )))
    }
}

fn validate_accept(headers: &HeaderMap) -> HeaderResult {
    let accept = headers
        .get(ACCEPT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if accept.contains("application/json") && accept.contains("text/event-stream") {
        Ok(())
    } else {
        Err(Box::new(json_error(
            StatusCode::NOT_ACCEPTABLE,
            None,
            "Accept must include application/json and text/event-stream",
        )))
    }
}

fn json_error(status: StatusCode, id: Option<Value>, message: impl Into<String>) -> Response {
    let response = RpcResponse {
        jsonrpc: "2.0",
        id: id.unwrap_or(Value::Null),
        result: None,
        error: Some(RpcError::invalid_request(message.into())),
    };
    (status, [(CONTENT_TYPE, "application/json")], Json(response)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn http_post_requires_bearer_token_and_origin_safety() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let addr = listener.local_addr().expect("listener has local addr");
        let server = tokio::spawn(serve_listener(listener, "secret".to_string()));

        let client = reqwest::Client::new();
        let url = format!("http://{addr}/mcp");
        let body = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});

        let unauthorized = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .expect("unauthorized request should complete");
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let forbidden = client
            .post(&url)
            .header(AUTHORIZATION, "Bearer secret")
            .header(ORIGIN, "https://example.com")
            .header(ACCEPT, "application/json, text/event-stream")
            .json(&body)
            .send()
            .await
            .expect("forbidden request should complete");
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

        let rebinding = client
            .post(&url)
            .header(AUTHORIZATION, "Bearer secret")
            .header(HOST, "example.com")
            .header(ACCEPT, "application/json, text/event-stream")
            .json(&body)
            .send()
            .await
            .expect("rebinding request should complete");
        assert_eq!(rebinding.status(), StatusCode::FORBIDDEN);

        let ok = client
            .post(&url)
            .header(AUTHORIZATION, "Bearer secret")
            .header(ORIGIN, "http://127.0.0.1")
            .header(ACCEPT, "application/json, text/event-stream")
            .json(&body)
            .send()
            .await
            .expect("authorized request should complete");
        assert_eq!(ok.status(), StatusCode::OK);
        let value: Value = ok.json().await.expect("response should be JSON");
        assert_eq!(value["result"]["serverInfo"]["name"], "spotuify-mcp");

        server.abort();
    }

    #[tokio::test]
    async fn http_get_returns_405_when_sse_is_not_supported() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let addr = listener.local_addr().expect("listener has local addr");
        let server = tokio::spawn(serve_listener(listener, "secret".to_string()));

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/mcp"))
            .header(AUTHORIZATION, "Bearer secret")
            .header(ACCEPT, "text/event-stream")
            .send()
            .await
            .expect("GET request should complete");
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

        server.abort();
    }

    #[test]
    fn http_bind_rejects_non_loopback_addresses() {
        let addr: SocketAddr = "0.0.0.0:7777".parse().expect("valid socket addr");
        let err = ensure_loopback(addr).expect_err("wildcard bind must be rejected");
        assert!(err.to_string().contains("loopback"));
    }
}
