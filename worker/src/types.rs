//! Newtypes and sum types for the NTT request/receipt domain.
//!
//! Every domain primitive is a newtype.  No raw `u8`, `u64`, or
//! `String` crosses a public boundary.

use crate::error::ParseError;
use alloc::string::String;
use alloc::vec::Vec;

/// Polynomial degree expressed as `log2(n)`.  Confined to `[1, 30]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PolyDegreeLog2(u8);

impl PolyDegreeLog2 {
    /// Construct, validating the inclusive range `[1, 30]`.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::DegreeOutOfRange`] when `value` is `0` or
    /// greater than `30`.
    pub fn new(value: u8) -> Result<Self, ParseError> {
        match () {
            () if !(1..=30).contains(&value) => Err(ParseError::DegreeOutOfRange(value)),
            () => Ok(Self(value)),
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// USDC price quoted in micro-units (1 USDC = `1_000_000` micros).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NttCallPriceMicrosUsdc(u64);

impl NttCallPriceMicrosUsdc {
    #[must_use]
    pub const fn new(micros: u64) -> Self {
        Self(micros)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Identifier minted by the worker for one NTT job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId([u8; 16]);

impl JobId {
    #[must_use]
    pub const fn new(raw: [u8; 16]) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// 32-byte challenge nonce supplied or echoed by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChallengeNonce([u8; 32]);

impl ChallengeNonce {
    #[must_use]
    pub const fn new(raw: [u8; 32]) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Domain-separated hash of the caller's pubkey, prevents on-chain
/// receipt log from being a clear caller-graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallerAddrHash([u8; 32]);

impl CallerAddrHash {
    #[must_use]
    pub const fn new(raw: [u8; 32]) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Bit-exact digest of the NTT output, attestable across backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProofOfExecution([u8; 32]);

impl ProofOfExecution {
    #[must_use]
    pub const fn new(raw: [u8; 32]) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Unix epoch seconds, encoded as `u64` to dodge year-2038 nonsense.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnixSeconds(u64);

impl UnixSeconds {
    #[must_use]
    pub const fn new(seconds: u64) -> Self {
        Self(seconds)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Field selector.  Goldilocks ships in v0; `BabyBear` lands once
/// `goldilocks-ntt-hdl` v0.10 publishes its `PrimeFieldHdl` migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Goldilocks,
    BabyBear,
}

/// Forward (DFT) or inverse (IDFT) transform direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Inverse,
}

/// Identifier for a registered donor in the on-chain donor registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DonorId(u32);

impl DonorId {
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Identifier for a provisioned cloud-FPGA instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CloudInstanceId(u32);

impl CloudInstanceId {
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Which backend served a given NTT call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Verilator-compiled WASM simulator running inside the Worker.
    Wasm,
    /// Donor's home FPGA reached over Cloudflare Tunnel.
    Donor(DonorId),
    /// Cloud FPGA instance (AWS f2.6xlarge or similar).
    CloudFpga(CloudInstanceId),
}

/// Inclusive cap on `log2(n)` enforced by the active tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxDegreeLog2(u8);

impl MaxDegreeLog2 {
    #[must_use]
    pub const fn new(cap: u8) -> Self {
        Self(cap)
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Privacy mode requested by the caller.  `Opaque` suppresses the
/// on-chain receipt event in exchange for forfeiting public proof of
/// execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyMode {
    Logged,
    Opaque,
}

/// Parsed NTT request body, post-validation.
#[derive(Debug, Clone)]
pub struct NttRequest {
    field: Field,
    direction: Direction,
    degree_log2: PolyDegreeLog2,
    coefficients: Vec<u64>,
    challenge: ChallengeNonce,
    privacy: PrivacyMode,
}

impl NttRequest {
    #[must_use]
    pub fn new(
        field: Field,
        direction: Direction,
        degree_log2: PolyDegreeLog2,
        coefficients: Vec<u64>,
        challenge: ChallengeNonce,
        privacy: PrivacyMode,
    ) -> Self {
        Self {
            field,
            direction,
            degree_log2,
            coefficients,
            challenge,
            privacy,
        }
    }

    #[must_use]
    pub const fn field(&self) -> Field {
        self.field
    }

    #[must_use]
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    #[must_use]
    pub const fn degree_log2(&self) -> PolyDegreeLog2 {
        self.degree_log2
    }

    #[must_use]
    pub fn coefficients(&self) -> &[u64] {
        &self.coefficients
    }

    #[must_use]
    pub const fn challenge(&self) -> ChallengeNonce {
        self.challenge
    }

    #[must_use]
    pub const fn privacy(&self) -> PrivacyMode {
        self.privacy
    }
}

/// On-chain receipt fields written to the Solana-devnet registry.
#[derive(Debug, Clone)]
pub struct Receipt {
    job: JobId,
    caller: CallerAddrHash,
    payment_tx: PaymentTxHash,
    proof: ProofOfExecution,
    timestamp: UnixSeconds,
    backend: Backend,
}

impl Receipt {
    #[must_use]
    pub const fn new(
        job: JobId,
        caller: CallerAddrHash,
        payment_tx: PaymentTxHash,
        proof: ProofOfExecution,
        timestamp: UnixSeconds,
        backend: Backend,
    ) -> Self {
        Self {
            job,
            caller,
            payment_tx,
            proof,
            timestamp,
            backend,
        }
    }

    #[must_use]
    pub const fn job(&self) -> JobId {
        self.job
    }

    #[must_use]
    pub const fn caller(&self) -> CallerAddrHash {
        self.caller
    }

    #[must_use]
    pub const fn payment_tx(&self) -> PaymentTxHash {
        self.payment_tx
    }

    #[must_use]
    pub const fn proof(&self) -> ProofOfExecution {
        self.proof
    }

    #[must_use]
    pub const fn timestamp(&self) -> UnixSeconds {
        self.timestamp
    }

    #[must_use]
    pub const fn backend(&self) -> Backend {
        self.backend
    }
}

/// Solana base58 pubkey of the seller wallet that receives USDC.
#[derive(Debug, Clone)]
pub struct SellerPubkey(String);

impl SellerPubkey {
    #[must_use]
    pub const fn new(base58: String) -> Self {
        Self(base58)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Solana transaction signature for the settlement transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaymentTxHash([u8; 64]);

impl PaymentTxHash {
    #[must_use]
    pub const fn new(raw: [u8; 64]) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

/// Bytes returned by [`crate::handler::serve`] on success.  Wrapper
/// over `Vec<u8>` so the boundary handler cannot accidentally mix it
/// with raw input bytes.
#[derive(Debug, Clone)]
pub struct ResponseBytes(Vec<u8>);

impl ResponseBytes {
    #[must_use]
    pub const fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

/// Bytes received from the boundary handler before parsing.
#[derive(Debug, Clone)]
pub struct RequestBytes(Vec<u8>);

impl RequestBytes {
    #[must_use]
    pub const fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poly_degree_log2_rejects_zero() -> Result<(), ParseError> {
        PolyDegreeLog2::new(0)
            .err()
            .map(|_| ())
            .ok_or(ParseError::DegreeOutOfRange(0))
    }

    #[test]
    fn poly_degree_log2_accepts_in_range() -> Result<(), ParseError> {
        PolyDegreeLog2::new(12).map(|_| ())
    }

    #[test]
    fn poly_degree_log2_rejects_above_thirty() -> Result<(), ParseError> {
        PolyDegreeLog2::new(31)
            .err()
            .map(|_| ())
            .ok_or(ParseError::DegreeOutOfRange(31))
    }
}
