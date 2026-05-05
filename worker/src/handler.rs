//! `serve(request_bytes) -> Io<Error, ResponseBytes>` entry point.
//!
//! All logic is composed inside `Io` and never executed here.  The
//! Cloudflare Workers `#[event(fetch)]` shim runs the returned `Io`
//! exactly once at the edge, satisfying the delay-`run` rule from
//! `CLAUDE.md`.
//!
//! v0.1 wires the full pure pipeline end-to-end:
//!
//! 1. `parse_request`: JSON request body is parsed into [`NttRequest`].
//! 2. `quote_for`: requested degree is gated against [`FREE_TIER_MAX_LOG2`].
//! 3. `backend::compute_wasm`: NTT runs over Plonky3's Goldilocks field.
//! 4. `build_response_bytes`: the [`ComputeOutcome`] is encoded as a
//!    JSON response body containing the NTT output, the SHA-256
//!    proof-of-execution, the backend tier, and an echo of the
//!    request fields.

use crate::backend::{self, ComputeOutcome};
use crate::error::{Error, ParseError};
use crate::quote;
use crate::types::{
    Backend, ChallengeNonce, Direction, Field, MaxDegreeLog2, NttRequest, PolyDegreeLog2,
    PrivacyMode, RequestBytes, ResponseBytes,
};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use comp_cat_rs::effect::io::Io;

/// Cap on `log2(n)` enforced by the free, in-Worker tier.  Donor and
/// cloud tiers raise this in v0.2 and v0.3.
pub const FREE_TIER_MAX_LOG2: u8 = 12;

/// Top-level entry point.  Pure construction of an `Io` pipeline; no
/// side effects until the boundary calls `.run()`.
#[must_use]
pub fn serve(request: &RequestBytes) -> Io<Error, ResponseBytes> {
    parse_and_quote(request.as_slice())
        .map_or_else(|e| Io::suspend(move || Err(e)), run_compute_to_response)
}

fn parse_and_quote(bytes: &[u8]) -> Result<NttRequest, Error> {
    parse_request(bytes).and_then(|req| {
        let cap = MaxDegreeLog2::new(FREE_TIER_MAX_LOG2);
        quote::quote_for(req.degree_log2(), cap)
            .map(|_| req)
            .map_err(Error::from)
    })
}

fn run_compute_to_response(req: NttRequest) -> Io<Error, ResponseBytes> {
    let coeffs = req.coefficients().to_vec();
    backend::compute_wasm(req.field(), req.direction(), req.degree_log2(), coeffs)
        .map(move |outcome| build_response_bytes(&req, &outcome))
}

/// Parse a JSON NTT request body into [`NttRequest`].
///
/// # Errors
///
/// Returns [`Error::Parse`] when the body is not valid JSON, when
/// required fields are missing or wrongly typed, or when scalar
/// values fall outside their domain.
pub fn parse_request(bytes: &[u8]) -> Result<NttRequest, Error> {
    let json: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| Error::Parse(ParseError::SchemaMismatch(format!("invalid JSON: {e}"))))?;
    let field = parse_field(&json)?;
    let direction = parse_direction(&json)?;
    let degree_log2 = parse_degree(&json)?;
    let coefficients = parse_coefficients(&json)?;
    Ok(NttRequest::new(
        field,
        direction,
        degree_log2,
        coefficients,
        ChallengeNonce::new([0u8; 32]),
        PrivacyMode::Logged,
    ))
}

fn parse_field(json: &serde_json::Value) -> Result<Field, ParseError> {
    json.get("field")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ParseError::SchemaMismatch("missing or non-string 'field'".to_string()))
        .and_then(|s| match s {
            "Goldilocks" => Ok(Field::Goldilocks),
            "BabyBear" => Ok(Field::BabyBear),
            other => Err(ParseError::SchemaMismatch(format!(
                "unknown field '{other}', expected Goldilocks or BabyBear"
            ))),
        })
}

fn parse_direction(json: &serde_json::Value) -> Result<Direction, ParseError> {
    json.get("direction")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ParseError::SchemaMismatch("missing or non-string 'direction'".to_string()))
        .and_then(|s| match s {
            "Forward" => Ok(Direction::Forward),
            "Inverse" => Ok(Direction::Inverse),
            other => Err(ParseError::SchemaMismatch(format!(
                "unknown direction '{other}', expected Forward or Inverse"
            ))),
        })
}

fn parse_degree(json: &serde_json::Value) -> Result<PolyDegreeLog2, ParseError> {
    json.get("degree_log2")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            ParseError::SchemaMismatch("missing or non-integer 'degree_log2'".to_string())
        })
        .and_then(|n| {
            u8::try_from(n)
                .map_err(|_| ParseError::SchemaMismatch(format!("degree_log2 {n} out of u8 range")))
        })
        .and_then(PolyDegreeLog2::new)
}

fn parse_coefficients(json: &serde_json::Value) -> Result<Vec<u64>, ParseError> {
    json.get("coefficients")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            ParseError::SchemaMismatch("missing or non-array 'coefficients'".to_string())
        })
        .and_then(|arr| {
            arr.iter()
                .map(|v| {
                    v.as_u64().ok_or_else(|| {
                        ParseError::SchemaMismatch(
                            "coefficient is not a non-negative integer".to_string(),
                        )
                    })
                })
                .collect()
        })
}

/// Encode a [`ComputeOutcome`] plus a request echo as JSON bytes.
#[must_use]
pub fn build_response_bytes(req: &NttRequest, outcome: &ComputeOutcome) -> ResponseBytes {
    let proof_hex = hex_encode(outcome.proof().as_bytes());
    let outputs = outcome
        .output()
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let backend_tag = backend_tag(outcome.backend());
    let field_tag = field_tag(req.field());
    let direction_tag = direction_tag(req.direction());
    let degree = req.degree_log2().get();
    let body = format!(
        concat!(
            r#"{{"output":[{}],"#,
            r#""proof_of_execution":"{}","#,
            r#""backend":"{}","#,
            r#""field":"{}","#,
            r#""direction":"{}","#,
            r#""degree_log2":{}}}"#,
        ),
        outputs, proof_hex, backend_tag, field_tag, direction_tag, degree,
    );
    ResponseBytes::new(body.into_bytes())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|b| [nibble_to_hex(b >> 4), nibble_to_hex(b & 0xf)])
        .collect()
}

fn nibble_to_hex(n: u8) -> char {
    match n {
        0..=9 => char::from(b'0' + n),
        10..=15 => char::from(b'a' + n - 10),
        _ => '?',
    }
}

const fn backend_tag(b: Backend) -> &'static str {
    match b {
        Backend::Wasm => "Wasm",
        Backend::Donor(_) => "Donor",
        Backend::CloudFpga(_) => "CloudFpga",
    }
}

const fn field_tag(f: Field) -> &'static str {
    match f {
        Field::Goldilocks => "Goldilocks",
        Field::BabyBear => "BabyBear",
    }
}

const fn direction_tag(d: Direction) -> &'static str {
    match d {
        Direction::Forward => "Forward",
        Direction::Inverse => "Inverse",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_request_body(degree: u8) -> Vec<u8> {
        let n = 1u64 << degree;
        let coeffs: Vec<String> = (0..n).map(|i| (i + 1).to_string()).collect();
        let body = format!(
            r#"{{"field":"Goldilocks","direction":"Forward","degree_log2":{degree},"coefficients":[{}]}}"#,
            coeffs.join(",")
        );
        body.into_bytes()
    }

    #[test]
    fn parse_request_round_trips_basic_fields() -> Result<(), String> {
        let body = ok_request_body(2);
        let req = parse_request(&body).map_err(|e| format!("parse: {e}"))?;
        let goldilocks = matches!(req.field(), Field::Goldilocks);
        let forward = matches!(req.direction(), Direction::Forward);
        let degree_two = req.degree_log2().get() == 2;
        let four_coeffs = req.coefficients().len() == 4;
        (goldilocks && forward && degree_two && four_coeffs)
            .then_some(())
            .ok_or_else(|| format!("parse fields wrong: {req:?}"))
    }

    #[test]
    fn parse_request_rejects_bad_json() -> Result<(), String> {
        parse_request(b"{not json")
            .err()
            .filter(|e| matches!(e, Error::Parse(ParseError::SchemaMismatch(_))))
            .map(|_| ())
            .ok_or_else(|| "expected SchemaMismatch on bad JSON".to_string())
    }

    #[test]
    fn parse_request_rejects_unknown_field() -> Result<(), String> {
        let body = br#"{"field":"Mersenne31","direction":"Forward","degree_log2":2,"coefficients":[1,2,3,4]}"#;
        parse_request(body)
            .err()
            .filter(|e| matches!(e, Error::Parse(ParseError::SchemaMismatch(_))))
            .map(|_| ())
            .ok_or_else(|| "expected SchemaMismatch on unknown field".to_string())
    }

    #[test]
    fn parse_request_rejects_missing_direction() -> Result<(), String> {
        let body = br#"{"field":"Goldilocks","degree_log2":2,"coefficients":[1,2,3,4]}"#;
        parse_request(body)
            .err()
            .filter(|e| matches!(e, Error::Parse(ParseError::SchemaMismatch(_))))
            .map(|_| ())
            .ok_or_else(|| "expected SchemaMismatch on missing direction".to_string())
    }

    #[test]
    fn parse_request_rejects_zero_degree() -> Result<(), String> {
        let body =
            br#"{"field":"Goldilocks","direction":"Forward","degree_log2":0,"coefficients":[1]}"#;
        parse_request(body)
            .err()
            .filter(|e| matches!(e, Error::Parse(ParseError::DegreeOutOfRange(0))))
            .map(|_| ())
            .ok_or_else(|| "expected DegreeOutOfRange(0)".to_string())
    }

    #[test]
    fn serve_runs_full_pipeline_n4() -> Result<(), String> {
        let body = ok_request_body(2);
        let resp = serve(&RequestBytes::new(body))
            .run()
            .map_err(|e| format!("serve: {e}"))?;
        let text = core::str::from_utf8(resp.as_slice()).map_err(|e| format!("utf8: {e}"))?;
        let has_output = text.contains(r#""output":["#);
        let has_proof = text.contains(r#""proof_of_execution":""#);
        let has_backend = text.contains(r#""backend":"Wasm""#);
        let has_field = text.contains(r#""field":"Goldilocks""#);
        let has_direction = text.contains(r#""direction":"Forward""#);
        let has_degree = text.contains(r#""degree_log2":2"#);
        (has_output && has_proof && has_backend && has_field && has_direction && has_degree)
            .then_some(())
            .ok_or_else(|| format!("response missing expected keys: {text}"))
    }

    #[test]
    fn serve_rejects_above_free_tier() -> Result<(), String> {
        let body = ok_request_body(13);
        serve(&RequestBytes::new(body))
            .run()
            .err()
            .filter(|e| matches!(e, Error::OutOfTier(_)))
            .map(|_| ())
            .ok_or_else(|| "expected OutOfTier for log2(n) = 13".to_string())
    }

    #[test]
    fn serve_rejects_invalid_json() -> Result<(), String> {
        serve(&RequestBytes::new(b"not json".to_vec()))
            .run()
            .err()
            .filter(|e| matches!(e, Error::Parse(_)))
            .map(|_| ())
            .ok_or_else(|| "expected Parse error".to_string())
    }

    #[test]
    fn round_trip_via_serve_recovers_input() -> Result<(), String> {
        let degree = 3u8;
        let n: u64 = 1u64 << degree;
        let inputs: Vec<u64> = (1..=n).collect();
        let inputs_str: Vec<String> = inputs.iter().map(u64::to_string).collect();
        let fwd_body = format!(
            r#"{{"field":"Goldilocks","direction":"Forward","degree_log2":{degree},"coefficients":[{}]}}"#,
            inputs_str.join(",")
        );
        let fwd_resp = serve(&RequestBytes::new(fwd_body.into_bytes()))
            .run()
            .map_err(|e| format!("forward serve: {e}"))?;
        let fwd_text =
            core::str::from_utf8(fwd_resp.as_slice()).map_err(|e| format!("forward utf8: {e}"))?;
        let fwd_outputs = extract_output_array(fwd_text)?;
        let fwd_outputs_str: Vec<String> = fwd_outputs.iter().map(u64::to_string).collect();
        let inv_body = format!(
            r#"{{"field":"Goldilocks","direction":"Inverse","degree_log2":{degree},"coefficients":[{}]}}"#,
            fwd_outputs_str.join(",")
        );
        let inv_resp = serve(&RequestBytes::new(inv_body.into_bytes()))
            .run()
            .map_err(|e| format!("inverse serve: {e}"))?;
        let inv_text =
            core::str::from_utf8(inv_resp.as_slice()).map_err(|e| format!("inverse utf8: {e}"))?;
        let inv_outputs = extract_output_array(inv_text)?;
        (inv_outputs == inputs)
            .then_some(())
            .ok_or_else(|| format!("round-trip mismatch: got {inv_outputs:?}, want {inputs:?}"))
    }

    fn extract_output_array(json: &str) -> Result<Vec<u64>, String> {
        let v: serde_json::Value =
            serde_json::from_str(json).map_err(|e| format!("response parse: {e}"))?;
        v.get("output")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "no output array".to_string())
            .and_then(|arr| {
                arr.iter()
                    .map(|n| n.as_u64().ok_or_else(|| "non-u64 output".to_string()))
                    .collect()
            })
    }
}
