//! Solana-mainnet x402 facilitator client.
//!
//! Both `verify` and `settle` are expressed as `Io` stubs in v0; the
//! real JSON-RPC calls land once the Workers async edge is wired in
//! v0.1.  Keeping the surface as `Io` now means the edge will compose
//! these without escaping the effect midstream.

use crate::error::{Error, NotYetImplemented};
use crate::types::{NttCallPriceMicrosUsdc, PaymentTxHash};
use alloc::string::String;
use alloc::vec::Vec;
use comp_cat_rs::effect::io::Io;

/// Opaque envelope encoding the buyer's signed `X-Payment` header.
#[derive(Debug, Clone)]
pub struct PaymentEnvelope(Vec<u8>);

impl PaymentEnvelope {
    #[must_use]
    pub const fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

/// Facilitator endpoint URL.
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

/// Verify a payment envelope against the facilitator's `/verify`.
#[must_use]
pub fn verify(
    _facilitator: FacilitatorUrl,
    _envelope: PaymentEnvelope,
    _expected: NttCallPriceMicrosUsdc,
) -> Io<Error, ()> {
    Io::suspend(|| {
        Err(Error::NotYetImplemented(NotYetImplemented::new(
            "x402::verify (facilitator JSON-RPC)",
        )))
    })
}

/// Settle a verified payment envelope, returning the on-chain
/// transaction signature on success.
#[must_use]
pub fn settle(
    _facilitator: FacilitatorUrl,
    _envelope: PaymentEnvelope,
) -> Io<Error, PaymentTxHash> {
    Io::suspend(|| {
        Err(Error::NotYetImplemented(NotYetImplemented::new(
            "x402::settle (facilitator JSON-RPC)",
        )))
    })
}
