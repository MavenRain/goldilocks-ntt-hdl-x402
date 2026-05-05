//! Devnet faucet pull, scheduled via Cloudflare Workers cron triggers.
//!
//! `pull_airdrop` is intended to fire every ~22 hours from a Cron
//! Trigger and top up the receipt-signing wallet so on-chain receipt
//! writes never starve.  v0 ships the `Io` constructor; the cron
//! binding lands with the Workers async edge.

use crate::error::{Error, NotYetImplemented};
use alloc::string::String;
use comp_cat_rs::effect::io::Io;

/// Devnet faucet endpoint (e.g. CDP, Alchemy, Solana CLI airdrop).
#[derive(Debug, Clone)]
pub struct FaucetEndpoint(String);

impl FaucetEndpoint {
    #[must_use]
    pub const fn new(url: String) -> Self {
        Self(url)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Solana base58 address to top up.
#[derive(Debug, Clone)]
pub struct SignerAddress(String);

impl SignerAddress {
    #[must_use]
    pub const fn new(address: String) -> Self {
        Self(address)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Lamports requested from the faucet (1 SOL = `1_000_000_000` lamports).
#[derive(Debug, Clone, Copy)]
pub struct LamportRequest(u64);

impl LamportRequest {
    #[must_use]
    pub const fn new(lamports: u64) -> Self {
        Self(lamports)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Pull devnet SOL into the signer address.
#[must_use]
pub fn pull_airdrop(
    _endpoint: FaucetEndpoint,
    _signer: SignerAddress,
    _amount: LamportRequest,
) -> Io<Error, ()> {
    Io::suspend(|| {
        Err(Error::NotYetImplemented(NotYetImplemented::new(
            "faucet::pull_airdrop (devnet airdrop call)",
        )))
    })
}
