//! NTT compute backends.
//!
//! Three tiers, three `Io` constructors.  v0 wires `Wasm` only; the
//! `Donor` and `CloudFpga` constructors are present so callers can
//! pattern-match exhaustively against [`crate::types::Backend`] from
//! day one without breaking when the upgrades land.

use crate::error::{Error, NotYetImplemented};
use crate::types::{
    Backend, CloudInstanceId, Direction, DonorId, Field, PolyDegreeLog2, ProofOfExecution,
};
use alloc::vec::Vec;
use comp_cat_rs::effect::io::Io;

/// Outcome of a single NTT compute call.
#[derive(Debug, Clone)]
pub struct ComputeOutcome {
    output: Vec<u64>,
    proof: ProofOfExecution,
    backend: Backend,
}

impl ComputeOutcome {
    #[must_use]
    pub const fn new(output: Vec<u64>, proof: ProofOfExecution, backend: Backend) -> Self {
        Self {
            output,
            proof,
            backend,
        }
    }

    #[must_use]
    pub fn output(&self) -> &[u64] {
        &self.output
    }

    #[must_use]
    pub const fn proof(&self) -> ProofOfExecution {
        self.proof
    }

    #[must_use]
    pub const fn backend(&self) -> Backend {
        self.backend
    }
}

/// Run on the in-Worker Verilator-WASM simulator.
#[must_use]
pub fn compute_wasm(
    _field: Field,
    _direction: Direction,
    _degree: PolyDegreeLog2,
    _coefficients: Vec<u64>,
) -> Io<Error, ComputeOutcome> {
    Io::suspend(|| {
        Err(Error::NotYetImplemented(NotYetImplemented::new(
            "backend::compute_wasm (Verilator-compiled goldilocks-ntt-hdl WASM blob)",
        )))
    })
}

/// Delegate to a donor's home FPGA over Cloudflare Tunnel.  v0.2.
#[must_use]
pub fn compute_donor(
    _donor: DonorId,
    _field: Field,
    _direction: Direction,
    _degree: PolyDegreeLog2,
    _coefficients: Vec<u64>,
) -> Io<Error, ComputeOutcome> {
    Io::suspend(|| {
        Err(Error::NotYetImplemented(NotYetImplemented::new(
            "backend::compute_donor (donor mesh, v0.2)",
        )))
    })
}

/// Delegate to a cloud-FPGA instance.  v0.3, unlocks at ~$50/day USDC inflow.
#[must_use]
pub fn compute_cloud(
    _instance: CloudInstanceId,
    _field: Field,
    _direction: Direction,
    _degree: PolyDegreeLog2,
    _coefficients: Vec<u64>,
) -> Io<Error, ComputeOutcome> {
    Io::suspend(|| {
        Err(Error::NotYetImplemented(NotYetImplemented::new(
            "backend::compute_cloud (AWS f2.6xlarge, v0.3)",
        )))
    })
}
