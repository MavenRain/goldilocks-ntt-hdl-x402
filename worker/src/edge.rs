//! Cloudflare Workers `#[event(fetch)]` shim.
//!
//! This is the single boundary that bridges async I/O (Workers
//! request handling, Solana RPC, x402 facilitator HTTP) to the
//! synchronous `Io`-pure core in [`crate::handler`] and friends.  All
//! `Io` pipelines are run exactly once per request, satisfying the
//! delay-`run` rule from `CLAUDE.md`.
//!
//! Routing surface in v0.1:
//!
//! - `GET /`: human-readable JSON service descriptor.
//! - `GET /.well-known/x402`: machine-readable Bazaar discovery manifest.
//! - `POST /ntt` without `X-Payment`: `402 Payment Required` with x402 envelope.
//! - `POST /ntt` with `X-Payment`: forwards request bytes into
//!   [`crate::handler::serve`].  v0.1 stubs return `501 Not Implemented` until
//!   the compute, verify, and settle subsystems land.

use crate::error::Error;
use crate::handler::serve;
use crate::types::RequestBytes;
use alloc::format;
use alloc::string::{String, ToString};
use worker::{Context, Env, Request, Response, Result as WorkerResult, event};

const X_PAYMENT_HEADER: &str = "X-Payment";

const SERVICE_DESCRIPTOR: &str = r#"{
  "service": "ntt-x402",
  "version": "0.1.0",
  "fields": ["Goldilocks"],
  "directions": ["Forward", "Inverse"],
  "max_degree_log2_free_tier": 12,
  "x402_endpoint": {"method": "POST", "path": "/ntt"},
  "settlement_chain": "solana-mainnet",
  "state_chain": "solana-devnet",
  "tiers": [
    {"max_log2": 8,  "micros_usdc": 1000},
    {"max_log2": 12, "micros_usdc": 10000},
    {"max_log2": 16, "micros_usdc": 100000},
    {"max_log2": 30, "micros_usdc": 1000000}
  ]
}"#;

const BAZAAR_MANIFEST: &str = r#"{
  "x402Version": "1",
  "endpoints": [
    {
      "method": "POST",
      "path": "/ntt",
      "asset": "USDC",
      "network": "solana",
      "maxAmountMicros": 10000,
      "description": "Goldilocks NTT compute (forward and inverse), log2(n) <= 12 on the free tier"
    }
  ]
}"#;

/// Top-level Workers fetch handler.  Sets the panic hook once per
/// isolate, then dispatches by method and path.
///
/// # Errors
///
/// Returns `worker::Error` only if the underlying `Response` builder
/// fails (out-of-memory or impossible header allocation).  Domain
/// errors (parse, payment, settlement, registry, backend, faucet) are
/// converted to HTTP responses via [`error_status_and_body`] and
/// returned as `Ok(Response)`.
#[event(fetch)]
pub async fn fetch(req: Request, _env: Env, _ctx: Context) -> WorkerResult<Response> {
    console_error_panic_hook::set_once();
    dispatch(req).await
}

async fn dispatch(req: Request) -> WorkerResult<Response> {
    let path = req.path();
    let method = req.method();
    match (method, path.as_str()) {
        (worker::Method::Get, "/") => json_ok(SERVICE_DESCRIPTOR),
        (worker::Method::Get, "/.well-known/x402") => json_ok(BAZAAR_MANIFEST),
        (worker::Method::Post, "/ntt") => handle_ntt(req).await,
        (
            worker::Method::Get
            | worker::Method::Post
            | worker::Method::Put
            | worker::Method::Delete
            | worker::Method::Head
            | worker::Method::Options
            | worker::Method::Patch
            | worker::Method::Connect
            | worker::Method::Trace,
            _,
        ) => Response::error("not found", 404),
    }
}

async fn handle_ntt(mut req: Request) -> WorkerResult<Response> {
    let payment_present = req.headers().get(X_PAYMENT_HEADER).ok().flatten().is_some();
    if payment_present {
        req.bytes().await.and_then(run_serve_pipeline)
    } else {
        respond_payment_required()
    }
}

fn run_serve_pipeline(body: alloc::vec::Vec<u8>) -> WorkerResult<Response> {
    serve(RequestBytes::new(body))
        .run()
        .map_err(|e| error_to_worker_error(&e))
        .and_then(|response_bytes| {
            Response::from_bytes(response_bytes.into_inner())
                .map(|r| r.with_headers(json_headers()))
        })
        .or_else(|_| {
            Response::error("internal: response build failed", 500)
                .map(|r| r.with_headers(json_headers()))
        })
}

fn error_to_worker_error(err: &Error) -> worker::Error {
    let (status, body) = error_status_and_body(err);
    worker::Error::RustError(format!("{status} {body}"))
}

fn error_status_and_body(err: &Error) -> (u16, String) {
    match err {
        Error::Parse(e) => (400, format!("parse error: {e}")),
        Error::OutOfTier(e) => (413, format!("out of tier: {e}")),
        Error::Settlement(e) => (402, format!("settlement error: {e}")),
        Error::Registry(e) => (502, format!("registry error: {e}")),
        Error::Backend(e) => (500, format!("backend error: {e}")),
        Error::Faucet(e) => (502, format!("faucet error: {e}")),
        Error::NotYetImplemented(e) => (501, format!("not yet implemented: {e}")),
    }
}

fn respond_payment_required() -> WorkerResult<Response> {
    let body = r#"{
  "x402Version": "1",
  "accepts": [
    {
      "scheme": "exact",
      "network": "solana",
      "asset": "USDC",
      "maxAmountRequired": "10000",
      "resource": "/ntt",
      "description": "Goldilocks NTT compute, log2(n) <= 12 on the free tier"
    }
  ]
}"#;
    Response::error(body.to_string(), 402).map(|r| r.with_headers(json_headers()))
}

fn json_ok(body: &str) -> WorkerResult<Response> {
    Response::ok(body.to_string()).map(|r| r.with_headers(json_headers()))
}

// FFI carve-out: `worker::Headers::append` is `&mut self`.  This is
// the sole `let mut` in the worker crate, parallel to the
// kan-hunt-anchor-runner solana-program-test exception.
fn json_headers() -> worker::Headers {
    let mut h = worker::Headers::new();
    let _ = h.append("Content-Type", "application/json");
    let _ = h.append("X-Service", "ntt-x402");
    h
}
