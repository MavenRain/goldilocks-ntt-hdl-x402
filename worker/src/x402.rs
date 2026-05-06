//! x402 facilitator client.
//!
//! Pure helpers (`build_verify_body`, `parse_verify_response`,
//! `build_settle_body`, `parse_settle_response`, `decode_payment_payload`)
//! handle all request/response shaping and are unit-tested on the
//! host.  The `verify_async` and `settle_async` entry points are
//! gated to `wasm32` because they call `worker::Fetch` from inside
//! the Cloudflare Workers runtime.
//!
//! Edge.rs awaits these around `handler::serve`, so the Io-pure core
//! never has to know that an async hop happened.
//!
//! Wire shape (V1, per `@x402/core` zod schemas):
//! - The buyer's `X-Payment` header value is `safeBase64Encode(JSON.stringify(paymentPayload))`.
//! - The facilitator's `/verify` and `/settle` accept
//!   `{"x402Version":1,"paymentPayload":<decoded>,"paymentRequirements":<requirements>}`.
//! - The `paymentRequirements` value must match the JSON the seller
//!   advertised in the `accepts[0]` slot of the original 402 envelope,
//!   so callers pre-build it once (in `edge.rs`) and pass it to both
//!   the 402 response and the verify/settle bodies via
//!   [`PaymentRequirementsJson`].

use crate::error::{Error, SettlementError};
use crate::types::PaymentTxHash;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use base64::Engine;

/// Opaque envelope encoding the buyer's signed `X-Payment` header.
/// In x402 V1 the header value is `safeBase64Encode(JSON.stringify(paymentPayload))`,
/// so the canonical in-memory form is the original base64 string.
#[derive(Debug, Clone)]
pub struct PaymentEnvelope(String);

impl PaymentEnvelope {
    #[must_use]
    pub const fn new(base64: String) -> Self {
        Self(base64)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Pre-built `paymentRequirements` JSON object string.  This is the
/// exact byte sequence advertised in the seller's 402 envelope's
/// `accepts[0]` slot, and must be inlined verbatim into the verify
/// and settle bodies so the facilitator's deep-equality check passes.
#[derive(Debug, Clone)]
pub struct PaymentRequirementsJson(String);

impl PaymentRequirementsJson {
    #[must_use]
    pub const fn new(json: String) -> Self {
        Self(json)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Facilitator endpoint URL (e.g. `https://www.x402.org/facilitator`).
#[derive(Debug, Clone)]
pub struct FacilitatorUrl(String);

impl FacilitatorUrl {
    #[must_use]
    pub const fn new(url: String) -> Self {
        Self(url)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Decode the buyer's `X-Payment` header value into the inner
/// paymentPayload JSON.  The result is a JSON object string, ready
/// to inline into a verify or settle body.
///
/// # Errors
///
/// Returns [`SettlementError::VerifyRejected`] when the value is not
/// valid base64 or the decoded bytes are not valid UTF-8.
pub fn decode_payment_payload(envelope_b64: &str) -> Result<String, Error> {
    base64::engine::general_purpose::STANDARD
        .decode(envelope_b64.as_bytes())
        .map_err(|e| {
            Error::Settlement(SettlementError::VerifyRejected(format!(
                "X-Payment base64 decode: {e}"
            )))
        })
        .and_then(|bytes| {
            String::from_utf8(bytes).map_err(|e| {
                Error::Settlement(SettlementError::VerifyRejected(format!(
                    "X-Payment utf8 decode: {e}"
                )))
            })
        })
}

/// Build the JSON body for `POST /verify`.  Inlines the buyer's
/// decoded paymentPayload and the seller's pre-built
/// paymentRequirements.
///
/// # Errors
///
/// Returns [`SettlementError::VerifyRejected`] when the envelope
/// cannot be base64-decoded.
pub fn build_verify_body(
    envelope: &PaymentEnvelope,
    requirements: &PaymentRequirementsJson,
) -> Result<String, Error> {
    decode_payment_payload(envelope.as_str()).map(|payload_json| {
        format!(
            r#"{{"x402Version":1,"paymentPayload":{payload_json},"paymentRequirements":{requirements}}}"#,
            payload_json = payload_json,
            requirements = requirements.as_str(),
        )
    })
}

/// Build the JSON body for `POST /settle`.  Same shape as the verify
/// body; the facilitator uses both the payload and the requirements
/// to construct and submit the on-chain transaction.
///
/// # Errors
///
/// Returns [`SettlementError::SettleRejected`] when the envelope
/// cannot be base64-decoded.
pub fn build_settle_body(
    envelope: &PaymentEnvelope,
    requirements: &PaymentRequirementsJson,
) -> Result<String, Error> {
    decode_payment_payload(envelope.as_str())
        .map_err(|e| match e {
            Error::Settlement(SettlementError::VerifyRejected(detail)) => {
                Error::Settlement(SettlementError::SettleRejected(detail))
            }
            other => other,
        })
        .map(|payload_json| {
            format!(
                r#"{{"x402Version":1,"paymentPayload":{payload_json},"paymentRequirements":{requirements}}}"#,
                payload_json = payload_json,
                requirements = requirements.as_str(),
            )
        })
}

/// Parse the facilitator's `/verify` response.  Returns `Ok(())` if
/// the facilitator reports the payment as valid.
///
/// # Errors
///
/// Returns [`SettlementError::VerifyRejected`] when the response body
/// does not contain `"isValid":true`.
pub fn parse_verify_response(json: &str) -> Result<(), Error> {
    let normalized = strip_whitespace(json);
    if normalized.contains(r#""isValid":true"#) {
        Ok(())
    } else {
        Err(Error::Settlement(SettlementError::VerifyRejected(
            json.to_string(),
        )))
    }
}

/// Parse the facilitator's `/settle` response.  Extracts the on-chain
/// transaction signature.
///
/// # Errors
///
/// Returns [`SettlementError::SettleRejected`] when the response body
/// does not contain `"success":true` and a base58 `transaction` field
/// that decodes to a 64-byte Solana signature.
pub fn parse_settle_response(json: &str) -> Result<PaymentTxHash, Error> {
    let normalized = strip_whitespace(json);
    if normalized.contains(r#""success":true"#) {
        extract_string_field(&normalized, "transaction")
            .ok_or_else(|| {
                Error::Settlement(SettlementError::SettleRejected(format!(
                    "no transaction field in: {json}"
                )))
            })
            .and_then(|sig_b58| decode_solana_signature(&sig_b58))
            .map(PaymentTxHash::new)
    } else {
        Err(Error::Settlement(SettlementError::SettleRejected(
            json.to_string(),
        )))
    }
}

fn strip_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn extract_string_field(normalized: &str, key: &str) -> Option<String> {
    let needle = format!(r#""{key}":""#);
    normalized.find(&needle).and_then(|start| {
        let value_start = start + needle.len();
        normalized
            .get(value_start..)
            .and_then(|tail| tail.find('"').map(|end| (value_start, value_start + end)))
            .and_then(|(start_idx, end_idx)| normalized.get(start_idx..end_idx))
            .map(ToString::to_string)
    })
}

fn decode_solana_signature(s: &str) -> Result<[u8; 64], Error> {
    bs58::decode(s)
        .into_vec()
        .map_err(|e| {
            Error::Settlement(SettlementError::SettleRejected(format!(
                "base58 decode: {e}"
            )))
        })
        .and_then(|bytes: Vec<u8>| {
            <[u8; 64]>::try_from(bytes.as_slice()).map_err(|_| {
                Error::Settlement(SettlementError::SettleRejected(format!(
                    "signature not 64 bytes (got {})",
                    bytes.len()
                )))
            })
        })
}

#[cfg(target_arch = "wasm32")]
mod async_io {
    use super::{
        FacilitatorUrl, PaymentEnvelope, PaymentRequirementsJson, build_settle_body,
        build_verify_body, parse_settle_response, parse_verify_response,
    };
    use crate::error::{Error, SettlementError};
    use crate::types::PaymentTxHash;
    use alloc::format;
    use alloc::string::String;
    use worker::{Fetch, Headers, Method, Request, RequestInit};

    /// Verify a payment envelope against the facilitator's `/verify`.
    ///
    /// # Errors
    ///
    /// Returns [`SettlementError::Transport`] on network failure or
    /// [`SettlementError::VerifyRejected`] when the facilitator rejects.
    pub async fn verify_async(
        facilitator: &FacilitatorUrl,
        envelope: &PaymentEnvelope,
        requirements: &PaymentRequirementsJson,
    ) -> Result<(), Error> {
        let body = build_verify_body(envelope, requirements)?;
        let url = format!("{}/verify", facilitator.as_str());
        post_json(&url, body)
            .await
            .and_then(|text| parse_verify_response(&text))
    }

    /// Settle a verified payment envelope, returning the on-chain
    /// transaction signature on success.
    ///
    /// # Errors
    ///
    /// Returns [`SettlementError::Transport`] on network failure or
    /// [`SettlementError::SettleRejected`] when the facilitator rejects.
    pub async fn settle_async(
        facilitator: &FacilitatorUrl,
        envelope: &PaymentEnvelope,
        requirements: &PaymentRequirementsJson,
    ) -> Result<PaymentTxHash, Error> {
        let body = build_settle_body(envelope, requirements)?;
        let url = format!("{}/settle", facilitator.as_str());
        post_json(&url, body)
            .await
            .and_then(|text| parse_settle_response(&text))
    }

    async fn post_json(url: &str, body: String) -> Result<String, Error> {
        let req = build_post_request(url, body)?;
        let mut resp = Fetch::Request(req)
            .send()
            .await
            .map_err(|e| Error::Settlement(SettlementError::Transport(format!("{e}"))))?;
        resp.text()
            .await
            .map_err(|e| Error::Settlement(SettlementError::Transport(format!("{e}"))))
    }

    // FFI carve-out: `worker::RequestInit` exposes its builder
    // exclusively as `&mut self`, so a one-shot constructor is
    // impossible.  The mutation is quarantined to this function and
    // the result is consumed immediately, parallel to the
    // kan-hunt-anchor-runner solana-program-test carve-out documented
    // in CLAUDE.md.
    fn build_post_request(url: &str, body: String) -> Result<Request, Error> {
        let mut init = RequestInit::new();
        let init_ref = init
            .with_method(Method::Post)
            .with_body(Some(body.into()))
            .with_headers(json_post_headers());
        Request::new_with_init(url, init_ref)
            .map_err(|e| Error::Settlement(SettlementError::Transport(format!("{e}"))))
    }

    fn json_post_headers() -> Headers {
        [
            ("Content-Type", "application/json"),
            ("Accept", "application/json"),
        ]
        .into_iter()
        .collect()
    }
}

#[cfg(target_arch = "wasm32")]
pub use async_io::{settle_async, verify_async};

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn envelope_for(payload_json: &str) -> PaymentEnvelope {
        let b64 = base64::engine::general_purpose::STANDARD.encode(payload_json.as_bytes());
        PaymentEnvelope::new(b64)
    }

    fn requirements_with_amount(amount: &str) -> PaymentRequirementsJson {
        PaymentRequirementsJson::new(format!(
            r#"{{"scheme":"exact","network":"solana-devnet","asset":"4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU","maxAmountRequired":"{amount}","resource":"https://example.com/x","description":"x","payTo":"11111111111111111111111111111111","maxTimeoutSeconds":300,"extra":{{"feePayer":"CKPKJWNdJEqa81x7CkZ14BVPiY6y16Sxs7owznqtWYp5"}}}}"#
        ))
    }

    #[test]
    fn decode_payment_payload_extracts_inner_json() -> Result<(), String> {
        let env = envelope_for(r#"{"x402Version":1,"scheme":"exact"}"#);
        let decoded = decode_payment_payload(env.as_str()).map_err(|e| format!("decode: {e}"))?;
        (decoded == r#"{"x402Version":1,"scheme":"exact"}"#)
            .then_some(())
            .ok_or_else(|| format!("decoded mismatch: {decoded}"))
    }

    #[test]
    fn decode_payment_payload_rejects_bad_base64() -> Result<(), String> {
        decode_payment_payload("not!base64!")
            .err()
            .filter(|e| matches!(e, Error::Settlement(SettlementError::VerifyRejected(_))))
            .map(|_| ())
            .ok_or_else(|| "expected VerifyRejected on bad base64".to_string())
    }

    #[test]
    fn verify_body_inlines_payload_and_requirements() -> Result<(), String> {
        let env =
            envelope_for(r#"{"x402Version":1,"scheme":"exact","payload":{"transaction":"abc"}}"#);
        let req = requirements_with_amount("10000");
        let body = build_verify_body(&env, &req).map_err(|e| format!("build: {e}"))?;
        let has_payload = body.contains(r#""paymentPayload":{"x402Version":1,"scheme":"exact","payload":{"transaction":"abc"}}"#);
        let has_requirements =
            body.contains(r#""paymentRequirements":{"scheme":"exact","network":"solana-devnet""#);
        let has_version = body.contains(r#""x402Version":1"#);
        (has_payload && has_requirements && has_version)
            .then_some(())
            .ok_or_else(|| format!("body missing expected fields: {body}"))
    }

    #[test]
    fn settle_body_matches_verify_body_shape() -> Result<(), String> {
        let env = envelope_for(r#"{"x402Version":1,"scheme":"exact"}"#);
        let req = requirements_with_amount("10000");
        let v = build_verify_body(&env, &req).map_err(|e| format!("verify: {e}"))?;
        let s = build_settle_body(&env, &req).map_err(|e| format!("settle: {e}"))?;
        (v == s)
            .then_some(())
            .ok_or_else(|| format!("verify and settle bodies differ:\n  v={v}\n  s={s}"))
    }

    #[test]
    fn verify_response_accepts_valid() -> Result<(), String> {
        parse_verify_response(r#"{"isValid":true,"invalidReason":null}"#)
            .map_err(|e| format!("parse failed: {e}"))
    }

    #[test]
    fn verify_response_accepts_with_whitespace() -> Result<(), String> {
        parse_verify_response("{\n  \"isValid\": true,\n  \"invalidReason\": null\n}")
            .map_err(|e| format!("whitespace-tolerant parse failed: {e}"))
    }

    #[test]
    fn verify_response_rejects_invalid() -> Result<(), String> {
        parse_verify_response(r#"{"isValid":false,"invalidReason":"insufficient_funds"}"#)
            .err()
            .filter(|e| matches!(e, Error::Settlement(SettlementError::VerifyRejected(_))))
            .map(|_| ())
            .ok_or_else(|| "expected VerifyRejected".to_string())
    }

    #[test]
    fn settle_response_extracts_transaction_signature() -> Result<(), String> {
        let sig_bytes = [7u8; 64];
        let sig_b58 = bs58::encode(sig_bytes).into_string();
        let body = format!(
            r#"{{"success":true,"transaction":"{sig_b58}","network":"solana","errorReason":null}}"#
        );
        let tx = parse_settle_response(&body).map_err(|e| format!("parse failed: {e}"))?;
        (tx.as_bytes() == &sig_bytes)
            .then_some(())
            .ok_or_else(|| "tx hash mismatch".to_string())
    }

    #[test]
    fn settle_response_rejects_failure() -> Result<(), String> {
        parse_settle_response(r#"{"success":false,"errorReason":"on_chain_failure"}"#)
            .err()
            .filter(|e| matches!(e, Error::Settlement(SettlementError::SettleRejected(_))))
            .map(|_| ())
            .ok_or_else(|| "expected SettleRejected".to_string())
    }

    #[test]
    fn settle_response_rejects_bad_signature_length() -> Result<(), String> {
        let too_short = bs58::encode([1u8; 32]).into_string();
        let body = format!(r#"{{"success":true,"transaction":"{too_short}"}}"#);
        parse_settle_response(&body)
            .err()
            .filter(|e| matches!(e, Error::Settlement(SettlementError::SettleRejected(_))))
            .map(|_| ())
            .ok_or_else(|| "expected SettleRejected for short signature".to_string())
    }
}
