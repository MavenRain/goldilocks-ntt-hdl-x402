//! Cloudflare Workers `#[event(fetch)]` shim.
//!
//! This is the single boundary that bridges async I/O (Workers
//! request handling, x402 facilitator HTTP, eventual Solana RPC) to
//! the synchronous `Io`-pure core in [`crate::handler`] and friends.
//! All `Io` pipelines are run exactly once per request, satisfying
//! the delay-`run` rule from `CLAUDE.md`.
//!
//! Routing surface in v0.1:
//!
//! - `GET /`: human-readable JSON service descriptor.
//! - `GET /.well-known/x402`: machine-readable Bazaar discovery manifest.
//! - `POST /ntt` without `X-Payment`: `402 Payment Required` with x402 envelope.
//! - `POST /ntt` with `X-Payment`: awaits [`crate::x402::verify_async`] against
//!   the CDP Solana facilitator, runs the full [`crate::handler::serve`]
//!   pipeline (parse + tier-quote + Goldilocks NTT + JSON response),
//!   then awaits [`crate::x402::settle_async`].  Returns the NTT result
//!   bytes with the on-chain settlement signature in the
//!   `X-Payment-Response` header.

use crate::error::{Error, SettlementError};
use crate::handler::serve;
use crate::types::RequestBytes;
use crate::x402::{
    FacilitatorUrl, PaymentEnvelope, PaymentRequirementsJson, settle_async, verify_async,
};
use alloc::format;
use alloc::string::{String, ToString};
use worker::{Context, Env, Request, Response, Result as WorkerResult, event};

const X_PAYMENT_HEADER: &str = "X-Payment";
const X_PAYMENT_RESPONSE_HEADER: &str = "X-Payment-Response";
const FREE_TIER_MAX_MICROS_USDC: u64 = 10_000;
const RESOURCE_URL: &str = "https://ntt-x402.isurvivable.workers.dev/ntt";
const TIER_DESCRIPTION: &str = "Goldilocks NTT compute, log2(n) <= 12 on the free tier";
const MAX_TIMEOUT_SECONDS: u32 = 300;

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
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> WorkerResult<Response> {
    console_error_panic_hook::set_once();
    dispatch(req, env).await
}

async fn dispatch(req: Request, env: Env) -> WorkerResult<Response> {
    let path = req.path();
    let method = req.method();
    match (method, path.as_str()) {
        (worker::Method::Get, "/") => json_ok(SERVICE_DESCRIPTOR),
        (worker::Method::Get, "/.well-known/x402") => json_ok(BAZAAR_MANIFEST),
        (worker::Method::Post, "/ntt") => handle_ntt(req, &env).await,
        (
            worker::Method::Get
            | worker::Method::Post
            | worker::Method::Put
            | worker::Method::Delete
            | worker::Method::Head
            | worker::Method::Options
            | worker::Method::Patch
            | worker::Method::Connect
            | worker::Method::Trace
            | worker::Method::Report,
            _,
        ) => Response::error("not found", 404),
    }
}

async fn handle_ntt(req: Request, env: &Env) -> WorkerResult<Response> {
    let payment_value = req.headers().get(X_PAYMENT_HEADER).ok().flatten();
    match payment_value {
        Some(envelope_b64) => settle_and_serve(req, env, envelope_b64).await,
        None => respond_payment_required(env),
    }
}

async fn settle_and_serve(req: Request, env: &Env, envelope_b64: String) -> WorkerResult<Response> {
    let config = read_settlement_config(env);
    match config {
        Ok((facilitator, requirements)) => {
            let envelope = PaymentEnvelope::new(envelope_b64);
            let verified = verify_async(&facilitator, &envelope, &requirements).await;
            match verified {
                Ok(()) => after_verify(req, &facilitator, &envelope, &requirements).await,
                Err(e) => domain_error_response(&e),
            }
        }
        Err(e) => domain_error_response(&e),
    }
}

async fn after_verify(
    req: Request,
    facilitator: &FacilitatorUrl,
    envelope: &PaymentEnvelope,
    requirements: &PaymentRequirementsJson,
) -> WorkerResult<Response> {
    let body = read_request_body(req).await?;
    let request_bytes = RequestBytes::new(body);
    let serve_result = serve(&request_bytes).run();
    match serve_result {
        Ok(response_bytes) => {
            let settled = settle_async(facilitator, envelope, requirements).await;
            match settled {
                Ok(tx) => Response::from_bytes(response_bytes.into_inner()).map(|r| {
                    r.with_headers(payment_response_headers(&payment_response_value(&tx)))
                }),
                Err(e) => domain_error_response(&e),
            }
        }
        Err(e) => domain_error_response(&e),
    }
}

// FFI carve-out: `worker::Request::bytes` is `&mut self`, so
// reading the body forces `let mut`.  The mutation is quarantined
// to this helper, parallel to the kan-hunt-anchor-runner
// solana-program-test exception documented in CLAUDE.md.
async fn read_request_body(mut req: Request) -> WorkerResult<alloc::vec::Vec<u8>> {
    req.bytes().await
}

fn read_settlement_config(env: &Env) -> Result<(FacilitatorUrl, PaymentRequirementsJson), Error> {
    let facilitator = env
        .var("FACILITATOR_URL")
        .map_err(|e| {
            Error::Settlement(SettlementError::Transport(format!(
                "FACILITATOR_URL var: {e}"
            )))
        })?
        .to_string();
    let requirements = build_requirements_json(env);
    Ok((FacilitatorUrl::new(facilitator), requirements))
}

/// Build the canonical paymentRequirements JSON object.  This MUST
/// match byte-for-byte what is inlined into the 402 envelope's
/// `accepts[0]` slot, since the facilitator's deep-equality check on
/// `paymentRequirements` is strict.
fn build_requirements_json(env: &Env) -> PaymentRequirementsJson {
    let seller_pubkey = env.var("SELLER_PUBKEY").map_or_else(
        |_| "11111111111111111111111111111111".to_string(),
        |v| v.to_string(),
    );
    let usdc_mint = env.var("USDC_MINT").map_or_else(
        |_| "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU".to_string(),
        |v| v.to_string(),
    );
    let fee_payer = env.var("FACILITATOR_FEE_PAYER").map_or_else(
        |_| "CKPKJWNdJEqa81x7CkZ14BVPiY6y16Sxs7owznqtWYp5".to_string(),
        |v| v.to_string(),
    );
    let network = env.var("SOLANA_CLUSTER").ok().map_or("solana", |v| {
        if v.to_string() == "devnet" {
            "solana-devnet"
        } else {
            "solana"
        }
    });
    let json = format!(
        concat!(
            r#"{{"scheme":"exact","#,
            r#""network":"{network}","#,
            r#""asset":"{usdc_mint}","#,
            r#""maxAmountRequired":"{amount}","#,
            r#""resource":"{resource}","#,
            r#""description":"{description}","#,
            r#""payTo":"{seller}","#,
            r#""maxTimeoutSeconds":{timeout},"#,
            r#""extra":{{"feePayer":"{fee_payer}"}}"#,
            r#"}}"#
        ),
        network = network,
        usdc_mint = usdc_mint,
        amount = FREE_TIER_MAX_MICROS_USDC,
        resource = RESOURCE_URL,
        description = TIER_DESCRIPTION,
        seller = seller_pubkey,
        timeout = MAX_TIMEOUT_SECONDS,
        fee_payer = fee_payer,
    );
    PaymentRequirementsJson::new(json)
}

fn payment_response_value(tx: &crate::types::PaymentTxHash) -> String {
    let signature_b58 = bs58::encode(tx.as_bytes()).into_string();
    format!(r#"{{"network":"solana","transaction":"{signature_b58}"}}"#)
}

fn domain_error_response(err: &Error) -> WorkerResult<Response> {
    let (status, body) = error_status_and_body(err);
    Response::error(body, status).map(|r| r.with_headers(json_headers()))
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

fn respond_payment_required(env: &Env) -> WorkerResult<Response> {
    // Wrap the canonical paymentRequirements JSON in the V1
    // PaymentRequired envelope.  The same requirements bytes are
    // re-used by verify_async/settle_async so the facilitator's
    // deep-equality check passes.
    let requirements = build_requirements_json(env);
    let body = format!(
        r#"{{"x402Version":1,"accepts":[{}]}}"#,
        requirements.as_str()
    );
    Response::error(body, 402).map(|r| r.with_headers(json_headers()))
}

fn json_ok(body: &str) -> WorkerResult<Response> {
    Response::ok(body.to_string()).map(|r| r.with_headers(json_headers()))
}

fn json_headers() -> worker::Headers {
    [
        ("Content-Type", "application/json"),
        ("X-Service", "ntt-x402"),
    ]
    .into_iter()
    .collect()
}

fn payment_response_headers(payment_response: &str) -> worker::Headers {
    [
        ("Content-Type", "application/json"),
        ("X-Service", "ntt-x402"),
        (X_PAYMENT_RESPONSE_HEADER, payment_response),
    ]
    .into_iter()
    .collect()
}
