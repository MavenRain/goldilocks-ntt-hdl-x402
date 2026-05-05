//! Project-wide `Error` enum.
//!
//! One enum, hand-rolled `Display` and `core::error::Error`, no
//! `thiserror`, no `anyhow`.  Each variant wraps a typed sub-error so
//! callers can match exhaustively on the failure mode.

use alloc::string::String;
use core::fmt;

/// Top-level error for the worker crate.
#[derive(Debug)]
pub enum Error {
    /// Request bytes could not be parsed as a valid NTT request.
    Parse(ParseError),
    /// Request asks for a `log2(n)` outside the configured tier cap.
    OutOfTier(OutOfTierError),
    /// Settlement against the Solana-mainnet x402 facilitator failed.
    Settlement(SettlementError),
    /// Solana-devnet RPC for the receipt or donor registry failed.
    Registry(RegistryError),
    /// NTT compute backend failed (WASM simulator, donor, or cloud).
    Backend(BackendError),
    /// Devnet faucet pull failed.
    Faucet(FaucetError),
    /// A subsystem is not yet implemented in this milestone.
    NotYetImplemented(NotYetImplemented),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "parse error: {e}"),
            Self::OutOfTier(e) => write!(f, "out of tier: {e}"),
            Self::Settlement(e) => write!(f, "settlement error: {e}"),
            Self::Registry(e) => write!(f, "registry error: {e}"),
            Self::Backend(e) => write!(f, "backend error: {e}"),
            Self::Faucet(e) => write!(f, "faucet error: {e}"),
            Self::NotYetImplemented(e) => write!(f, "not yet implemented: {e}"),
        }
    }
}

impl core::error::Error for Error {}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

impl From<OutOfTierError> for Error {
    fn from(e: OutOfTierError) -> Self {
        Self::OutOfTier(e)
    }
}

impl From<SettlementError> for Error {
    fn from(e: SettlementError) -> Self {
        Self::Settlement(e)
    }
}

impl From<RegistryError> for Error {
    fn from(e: RegistryError) -> Self {
        Self::Registry(e)
    }
}

impl From<BackendError> for Error {
    fn from(e: BackendError) -> Self {
        Self::Backend(e)
    }
}

impl From<FaucetError> for Error {
    fn from(e: FaucetError) -> Self {
        Self::Faucet(e)
    }
}

impl From<NotYetImplemented> for Error {
    fn from(e: NotYetImplemented) -> Self {
        Self::NotYetImplemented(e)
    }
}

/// Parse-stage failures.
#[derive(Debug)]
pub enum ParseError {
    /// Body was not valid UTF-8 JSON.
    InvalidUtf8,
    /// Body was UTF-8 JSON but did not match the expected schema.
    SchemaMismatch(String),
    /// `log2(n)` was outside the absolute supported range `[1, 30]`.
    DegreeOutOfRange(u8),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUtf8 => f.write_str("body is not valid UTF-8 JSON"),
            Self::SchemaMismatch(detail) => write!(f, "schema mismatch: {detail}"),
            Self::DegreeOutOfRange(d) => write!(f, "log2(n) = {d} outside [1, 30]"),
        }
    }
}

impl core::error::Error for ParseError {}

/// Tier cap violation.
#[derive(Debug)]
pub struct OutOfTierError {
    requested_log2: u8,
    tier_cap_log2: u8,
}

impl OutOfTierError {
    #[must_use]
    pub const fn new(requested_log2: u8, tier_cap_log2: u8) -> Self {
        Self {
            requested_log2,
            tier_cap_log2,
        }
    }

    #[must_use]
    pub const fn requested_log2(&self) -> u8 {
        self.requested_log2
    }

    #[must_use]
    pub const fn tier_cap_log2(&self) -> u8 {
        self.tier_cap_log2
    }
}

impl fmt::Display for OutOfTierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "requested log2(n) = {} exceeds free-tier cap {}",
            self.requested_log2, self.tier_cap_log2
        )
    }
}

impl core::error::Error for OutOfTierError {}

/// x402 facilitator failures (Solana mainnet).
#[derive(Debug)]
pub enum SettlementError {
    /// Caller did not present a valid `X-Payment` header.
    MissingPayment,
    /// Facilitator `/verify` returned a negative response.
    VerifyRejected(String),
    /// Facilitator `/settle` returned a negative response.
    SettleRejected(String),
    /// Facilitator could not be reached.
    Transport(String),
}

impl fmt::Display for SettlementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPayment => f.write_str("no X-Payment header on request"),
            Self::VerifyRejected(d) => write!(f, "verify rejected: {d}"),
            Self::SettleRejected(d) => write!(f, "settle rejected: {d}"),
            Self::Transport(d) => write!(f, "facilitator transport error: {d}"),
        }
    }
}

impl core::error::Error for SettlementError {}

/// Solana-devnet registry failures.
#[derive(Debug)]
pub enum RegistryError {
    /// RPC endpoint unreachable.
    RpcTransport(String),
    /// On-chain account did not deserialize as the expected layout.
    AccountDeserialize(String),
    /// Receipt-emit instruction failed on-chain.
    ReceiptEmitFailed(String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RpcTransport(d) => write!(f, "rpc transport: {d}"),
            Self::AccountDeserialize(d) => write!(f, "account deserialize: {d}"),
            Self::ReceiptEmitFailed(d) => write!(f, "receipt emit failed: {d}"),
        }
    }
}

impl core::error::Error for RegistryError {}

/// NTT backend failures.
#[derive(Debug)]
pub enum BackendError {
    /// Verilator-WASM simulator returned an error.
    SimulatorFailed(String),
    /// Donor backend reachable but reported failure.
    DonorFailed(String),
    /// Cloud-FPGA backend reachable but reported failure.
    CloudFpgaFailed(String),
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SimulatorFailed(d) => write!(f, "verilator-wasm: {d}"),
            Self::DonorFailed(d) => write!(f, "donor backend: {d}"),
            Self::CloudFpgaFailed(d) => write!(f, "cloud fpga: {d}"),
        }
    }
}

impl core::error::Error for BackendError {}

/// Devnet faucet failures.
#[derive(Debug)]
pub enum FaucetError {
    /// Faucet endpoint unreachable.
    Transport(String),
    /// Faucet returned a rate-limit response.
    RateLimited,
}

impl fmt::Display for FaucetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(d) => write!(f, "faucet transport: {d}"),
            Self::RateLimited => f.write_str("faucet rate-limited"),
        }
    }
}

impl core::error::Error for FaucetError {}

/// Marker for not-yet-implemented subsystems.
#[derive(Debug)]
pub struct NotYetImplemented {
    subsystem: &'static str,
}

impl NotYetImplemented {
    #[must_use]
    pub const fn new(subsystem: &'static str) -> Self {
        Self { subsystem }
    }

    #[must_use]
    pub const fn subsystem(&self) -> &'static str {
        self.subsystem
    }
}

impl fmt::Display for NotYetImplemented {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} not yet implemented", self.subsystem)
    }
}

impl core::error::Error for NotYetImplemented {}
