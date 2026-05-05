//! Solana-mainnet x402 facilitator client.
//!
//! Pure helpers (`build_verify_body`, `parse_verify_response`,
//! `build_settle_body`, `parse_settle_response`) handle all
//! request/response shaping and are unit-tested on the host.  The
//! `verify_async` and `settle_async` entry points are gated to
//! `wasm32` because they call `worker::Fetch` from inside the
//! Cloudflare Workers runtime; on the host they don't exist, which
//! keeps `cargo check` and `cargo test` portable.
//!
//! Edge.rs awaits these around `handler::serve`, so the Io-pure core
//! never has to know that an async hop happened.

use crate::error::{Error, SettlementError};
use crate::types::{NttCallPriceMicrosUsdc, PaymentTxHash, SellerPubkey};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Opaque envelope encoding the buyer's signed `X-Payment` header.
/// In x402 the header value is a base64-encoded EIP-712-style payload,
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

/// Facilitator endpoint URL (e.g. `https://api.cdp.coinbase.com/x402/solana-devnet/v1`).
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

/// Build the JSON body for a `POST /verify` call to the CDP facilitator.
/// The shape is the x402 v1 verification request envelope.
#[must_use]
pub fn build_verify_body(
    envelope: &PaymentEnvelope,
    expected: NttCallPriceMicrosUsdc,
    seller: &SellerPubkey,
) -> String {
    format!(
        concat!(
            r#"{{"x402Version":"1","#,
            r#""paymentHeader":"{}","#,
            r#""paymentRequirements":{{"#,
            r#""scheme":"exact","#,
            r#""network":"solana","#,
            r#""asset":"USDC","#,
            r#""maxAmountRequired":"{}","#,
            r#""resource":"/ntt","#,
            r#""payTo":"{}""#,
            r#"}}}}"#,
        ),
        envelope.as_str(),
        expected.get(),
        seller.as_str(),
    )
}

/// Build the JSON body for a `POST /settle` call to the CDP facilitator.
#[must_use]
pub fn build_settle_body(envelope: &PaymentEnvelope, seller: &SellerPubkey) -> String {
    format!(
        concat!(
            r#"{{"x402Version":"1","#,
            r#""paymentHeader":"{}","#,
            r#""paymentRequirements":{{"#,
            r#""scheme":"exact","#,
            r#""network":"solana","#,
            r#""asset":"USDC","#,
            r#""resource":"/ntt","#,
            r#""payTo":"{}""#,
            r#"}}}}"#,
        ),
        envelope.as_str(),
        seller.as_str(),
    )
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
        FacilitatorUrl, PaymentEnvelope, build_settle_body, build_verify_body,
        parse_settle_response, parse_verify_response,
    };
    use crate::error::{Error, SettlementError};
    use crate::types::{NttCallPriceMicrosUsdc, PaymentTxHash, SellerPubkey};
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
        expected: NttCallPriceMicrosUsdc,
        seller: &SellerPubkey,
    ) -> Result<(), Error> {
        let body = build_verify_body(envelope, expected, seller);
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
        seller: &SellerPubkey,
    ) -> Result<PaymentTxHash, Error> {
        let body = build_settle_body(envelope, seller);
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

    fn envelope(s: &str) -> PaymentEnvelope {
        PaymentEnvelope::new(s.to_string())
    }

    fn seller(s: &str) -> SellerPubkey {
        SellerPubkey::new(s.to_string())
    }

    #[test]
    fn verify_body_round_trips_through_field_extractor() -> Result<(), String> {
        let env = envelope("dGVzdA==");
        let s = seller("11111111111111111111111111111111");
        let body = build_verify_body(&env, NttCallPriceMicrosUsdc::new(10_000), &s);
        let normalized = strip_whitespace(&body);
        extract_string_field(&normalized, "paymentHeader")
            .filter(|v| v == "dGVzdA==")
            .map(|_| ())
            .ok_or_else(|| format!("paymentHeader not found in body: {body}"))
    }

    #[test]
    fn verify_body_carries_expected_amount() -> Result<(), String> {
        let env = envelope("AAAA");
        let s = seller("Sysvar1nstructions1111111111111111111111111");
        let body = build_verify_body(&env, NttCallPriceMicrosUsdc::new(123_456), &s);
        body.contains(r#""maxAmountRequired":"123456""#)
            .then_some(())
            .ok_or_else(|| format!("amount not in body: {body}"))
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

    #[test]
    fn settle_body_contains_seller() -> Result<(), String> {
        let env = envelope("Zm9v");
        let s = seller("ABCDEFGHJKLMNPQRSTUVWXYZ1234567890abcdefghi");
        let body = build_settle_body(&env, &s);
        body.contains(r#""payTo":"ABCDEFGHJKLMNPQRSTUVWXYZ1234567890abcdefghi""#)
            .then_some(())
            .ok_or_else(|| format!("payTo missing from body: {body}"))
    }
}
