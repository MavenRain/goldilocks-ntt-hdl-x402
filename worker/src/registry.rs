//! Solana-devnet registry client (receipt + donor + pricing).
//!
//! Each call is an `Io` stub in v0; real RPC and PDA serialization
//! land once the Workers async edge ships.

use crate::error::{Error, NotYetImplemented};
use crate::types::{DonorId, NttCallPriceMicrosUsdc, PolyDegreeLog2, Receipt};
use alloc::string::String;
use alloc::vec::Vec;
use comp_cat_rs::effect::io::Io;

/// Solana-devnet RPC endpoint.
#[derive(Debug, Clone)]
pub struct RpcUrl(String);

impl RpcUrl {
    #[must_use]
    pub const fn new(url: String) -> Self {
        Self(url)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// On-chain program addresses for the three registry programs.
#[derive(Debug, Clone)]
pub struct ProgramIds {
    receipt_registry: String,
    donor_registry: String,
    pricing_schedule: String,
}

impl ProgramIds {
    #[must_use]
    pub const fn new(
        receipt_registry: String,
        donor_registry: String,
        pricing_schedule: String,
    ) -> Self {
        Self {
            receipt_registry,
            donor_registry,
            pricing_schedule,
        }
    }

    #[must_use]
    pub fn receipt_registry(&self) -> &str {
        &self.receipt_registry
    }

    #[must_use]
    pub fn donor_registry(&self) -> &str {
        &self.donor_registry
    }

    #[must_use]
    pub fn pricing_schedule(&self) -> &str {
        &self.pricing_schedule
    }
}

/// Read the on-chain pricing tier for a given degree.
#[must_use]
pub fn read_pricing(
    _rpc: RpcUrl,
    _programs: ProgramIds,
    _degree: PolyDegreeLog2,
) -> Io<Error, NttCallPriceMicrosUsdc> {
    Io::suspend(|| {
        Err(Error::NotYetImplemented(NotYetImplemented::new(
            "registry::read_pricing (Solana devnet RPC)",
        )))
    })
}

/// Emit a receipt event to the receipt-registry program.
#[must_use]
pub fn emit_receipt(_rpc: RpcUrl, _programs: ProgramIds, _receipt: Receipt) -> Io<Error, ()> {
    Io::suspend(|| {
        Err(Error::NotYetImplemented(NotYetImplemented::new(
            "registry::emit_receipt (Solana devnet RPC)",
        )))
    })
}

/// Read the active donor list from the donor-registry program.
#[must_use]
pub fn list_donors(_rpc: RpcUrl, _programs: ProgramIds) -> Io<Error, Vec<DonorId>> {
    Io::suspend(|| {
        Err(Error::NotYetImplemented(NotYetImplemented::new(
            "registry::list_donors (Solana devnet RPC)",
        )))
    })
}
