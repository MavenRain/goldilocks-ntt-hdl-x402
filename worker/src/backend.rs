//! NTT compute backends.
//!
//! Three tiers, three `Io` constructors.  v0.1 ships `compute_wasm`
//! using a recursive Cooley-Tukey on Plonky3's
//! [`p3_goldilocks::Goldilocks`] field, with `[u8; 32]`
//! [`crate::types::ProofOfExecution`] computed as SHA-256 over a
//! tagged transcript.  The output is bit-exact equivalent to the
//! eventual Verilator-WASM blob compiled from `goldilocks-ntt-hdl`,
//! so the proof digest does not shift when v0.2 swaps backends.
//!
//! `compute_donor` and `compute_cloud` remain `NotYetImplemented`
//! stubs so callers can pattern-match exhaustively against
//! [`crate::types::Backend`] from day one without breaking when the
//! upgrades land.

use crate::error::{BackendError, Error, NotYetImplemented};
use crate::types::{
    Backend, CloudInstanceId, Direction, DonorId, Field, PolyDegreeLog2, ProofOfExecution,
};
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;
use comp_cat_rs::effect::io::Io;
use core::iter::successors;
use p3_field::{Field as P3Field, PrimeCharacteristicRing, PrimeField64, TwoAdicField};
use p3_goldilocks::Goldilocks;
use sha2::{Digest, Sha256};

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

/// Run on the in-Worker software backend.  v0.1 is a recursive
/// Cooley-Tukey software simulator over Plonky3's Goldilocks field.
/// v0.2 will swap the implementation for a Verilator-WASM blob
/// compiled from `goldilocks-ntt-hdl`; the proof-of-execution digest
/// is invariant under that swap because both backends produce
/// canonically-reduced field-element output.
#[must_use]
pub fn compute_wasm(
    field: Field,
    direction: Direction,
    degree: PolyDegreeLog2,
    coefficients: Vec<u64>,
) -> Io<Error, ComputeOutcome> {
    Io::suspend(move || compute_wasm_inner(field, direction, degree, &coefficients))
}

fn compute_wasm_inner(
    field: Field,
    direction: Direction,
    degree: PolyDegreeLog2,
    coefficients: &[u64],
) -> Result<ComputeOutcome, Error> {
    let n = 1usize << degree.get();
    let len = coefficients.len();
    if len == n {
        dispatch_field(field, direction, degree, coefficients)
            .map(|output| {
                let proof = compute_proof(field, direction, degree, &output);
                ComputeOutcome::new(output, proof, Backend::Wasm)
            })
            .map_err(Error::from)
    } else {
        Err(Error::Backend(BackendError::SimulatorFailed(format!(
            "expected {n} coefficients, got {len}"
        ))))
    }
}

fn dispatch_field(
    field: Field,
    direction: Direction,
    degree: PolyDegreeLog2,
    coefficients: &[u64],
) -> Result<Vec<u64>, BackendError> {
    match field {
        Field::Goldilocks => goldilocks_ntt(coefficients, direction, degree),
        Field::BabyBear => Err(BackendError::SimulatorFailed(
            "BabyBear backend lands in v0.2".to_string(),
        )),
    }
}

fn goldilocks_ntt(
    coefficients: &[u64],
    direction: Direction,
    degree: PolyDegreeLog2,
) -> Result<Vec<u64>, BackendError> {
    let log_n = usize::from(degree.get());
    if log_n > Goldilocks::TWO_ADICITY {
        Err(BackendError::SimulatorFailed(format!(
            "log2(n) = {log_n} exceeds Goldilocks two-adicity {}",
            Goldilocks::TWO_ADICITY
        )))
    } else {
        let omega_forward = Goldilocks::two_adic_generator(log_n);
        let omega = match direction {
            Direction::Forward => Ok(omega_forward),
            Direction::Inverse => omega_forward.try_inverse().ok_or_else(|| {
                BackendError::SimulatorFailed("primitive root has no inverse".to_string())
            }),
        }?;
        let field_in: Vec<Goldilocks> = coefficients
            .iter()
            .map(|&c| Goldilocks::from_u64(c))
            .collect();
        let result = ntt_recursive(&field_in, omega);
        let scaled = match direction {
            Direction::Forward => result,
            Direction::Inverse => {
                let n_field = Goldilocks::from_u64(1u64 << degree.get());
                let n_inv = n_field
                    .try_inverse()
                    .ok_or_else(|| BackendError::SimulatorFailed("n has no inverse".to_string()))?;
                result.into_iter().map(|x| x * n_inv).collect()
            }
        };
        Ok(scaled.into_iter().map(|x| x.as_canonical_u64()).collect())
    }
}

fn ntt_recursive(coeffs: &[Goldilocks], omega: Goldilocks) -> Vec<Goldilocks> {
    match coeffs.len() {
        0 | 1 => coeffs.to_vec(),
        n => {
            let half = n / 2;
            let evens: Vec<Goldilocks> = coeffs.iter().step_by(2).copied().collect();
            let odds: Vec<Goldilocks> = coeffs.iter().skip(1).step_by(2).copied().collect();
            let omega_sq = omega * omega;
            let e = ntt_recursive(&evens, omega_sq);
            let o = ntt_recursive(&odds, omega_sq);
            let twiddles: Vec<Goldilocks> = successors(Some(Goldilocks::ONE), |x| Some(*x * omega))
                .take(half)
                .collect();
            let lower = e
                .iter()
                .zip(o.iter())
                .zip(twiddles.iter())
                .map(|((&ek, &ok), &tk)| ek + tk * ok);
            let upper = e
                .iter()
                .zip(o.iter())
                .zip(twiddles.iter())
                .map(|((&ek, &ok), &tk)| ek - tk * ok);
            lower.chain(upper).collect()
        }
    }
}

fn compute_proof(
    field: Field,
    direction: Direction,
    degree: PolyDegreeLog2,
    output: &[u64],
) -> ProofOfExecution {
    let header = [
        u8::from(FieldTag::from(field)),
        u8::from(DirectionTag::from(direction)),
        degree.get(),
    ];
    let combined: Vec<u8> = header
        .into_iter()
        .chain(output.iter().flat_map(|&x| x.to_le_bytes()))
        .collect();
    let digest: [u8; 32] = Sha256::digest(combined).into();
    ProofOfExecution::new(digest)
}

#[derive(Debug, Clone, Copy)]
struct FieldTag(u8);

impl From<Field> for FieldTag {
    fn from(field: Field) -> Self {
        match field {
            Field::Goldilocks => Self(0),
            Field::BabyBear => Self(1),
        }
    }
}

impl From<FieldTag> for u8 {
    fn from(tag: FieldTag) -> Self {
        tag.0
    }
}

#[derive(Debug, Clone, Copy)]
struct DirectionTag(u8);

impl From<Direction> for DirectionTag {
    fn from(d: Direction) -> Self {
        match d {
            Direction::Forward => Self(0),
            Direction::Inverse => Self(1),
        }
    }
}

impl From<DirectionTag> for u8 {
    fn from(tag: DirectionTag) -> Self {
        tag.0
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;

    fn ok_or<T>(opt: Option<T>, msg: &str) -> Result<T, String> {
        opt.ok_or_else(|| msg.to_string())
    }

    #[test]
    fn forward_n2_matches_butterfly() -> Result<(), String> {
        let degree = PolyDegreeLog2::new(1).map_err(|e| format!("degree: {e}"))?;
        let coeffs = vec![3u64, 5u64];
        let outcome = compute_wasm(Field::Goldilocks, Direction::Forward, degree, coeffs)
            .run()
            .map_err(|e| format!("compute: {e}"))?;
        let out = outcome.output();
        ok_or(out.first().copied(), "missing slot 0").and_then(|a0| {
            ok_or(out.get(1).copied(), "missing slot 1").and_then(|a1| {
                let omega = Goldilocks::two_adic_generator(1);
                let f0 = Goldilocks::from_u64(3) + Goldilocks::from_u64(5);
                let f1 = Goldilocks::from_u64(3) + omega * Goldilocks::from_u64(5);
                let want0 = f0.as_canonical_u64();
                let want1 = f1.as_canonical_u64();
                (a0 == want0 && a1 == want1)
                    .then_some(())
                    .ok_or_else(|| format!("got [{a0},{a1}], want [{want0},{want1}]"))
            })
        })
    }

    #[test]
    fn round_trip_n4_recovers_input() -> Result<(), String> {
        let degree = PolyDegreeLog2::new(2).map_err(|e| format!("degree: {e}"))?;
        let coeffs: Vec<u64> = vec![7, 11, 13, 17];
        let forward = compute_wasm(
            Field::Goldilocks,
            Direction::Forward,
            degree,
            coeffs.clone(),
        )
        .run()
        .map_err(|e| format!("forward: {e}"))?;
        let back = compute_wasm(
            Field::Goldilocks,
            Direction::Inverse,
            degree,
            forward.output().to_vec(),
        )
        .run()
        .map_err(|e| format!("inverse: {e}"))?;
        (back.output() == coeffs.as_slice())
            .then_some(())
            .ok_or_else(|| {
                format!(
                    "round-trip mismatch: got {:?}, want {coeffs:?}",
                    back.output()
                )
            })
    }

    #[test]
    fn round_trip_n8_recovers_input() -> Result<(), String> {
        let degree = PolyDegreeLog2::new(3).map_err(|e| format!("degree: {e}"))?;
        let coeffs: Vec<u64> = (1..=8u64).collect();
        let forward = compute_wasm(
            Field::Goldilocks,
            Direction::Forward,
            degree,
            coeffs.clone(),
        )
        .run()
        .map_err(|e| format!("forward: {e}"))?;
        let back = compute_wasm(
            Field::Goldilocks,
            Direction::Inverse,
            degree,
            forward.output().to_vec(),
        )
        .run()
        .map_err(|e| format!("inverse: {e}"))?;
        (back.output() == coeffs.as_slice())
            .then_some(())
            .ok_or_else(|| format!("round-trip mismatch: got {:?}", back.output()))
    }

    #[test]
    fn length_mismatch_errors() -> Result<(), String> {
        let degree = PolyDegreeLog2::new(3).map_err(|e| format!("degree: {e}"))?;
        let coeffs = vec![1u64, 2u64, 3u64];
        compute_wasm(Field::Goldilocks, Direction::Forward, degree, coeffs)
            .run()
            .err()
            .filter(|e| matches!(e, Error::Backend(BackendError::SimulatorFailed(_))))
            .map(|_| ())
            .ok_or_else(|| "expected SimulatorFailed length-mismatch error".to_string())
    }

    #[test]
    fn babybear_returns_unsupported() -> Result<(), String> {
        let degree = PolyDegreeLog2::new(2).map_err(|e| format!("degree: {e}"))?;
        let coeffs = vec![1u64, 2, 3, 4];
        compute_wasm(Field::BabyBear, Direction::Forward, degree, coeffs)
            .run()
            .err()
            .filter(|e| format!("{e}").contains("BabyBear"))
            .map(|_| ())
            .ok_or_else(|| "expected BabyBear-not-supported error".to_string())
    }

    #[test]
    fn proof_is_deterministic() -> Result<(), String> {
        let degree = PolyDegreeLog2::new(2).map_err(|e| format!("degree: {e}"))?;
        let coeffs = vec![1u64, 2, 3, 4];
        let first = compute_wasm(
            Field::Goldilocks,
            Direction::Forward,
            degree,
            coeffs.clone(),
        )
        .run()
        .map_err(|e| format!("first: {e}"))?;
        let second = compute_wasm(Field::Goldilocks, Direction::Forward, degree, coeffs)
            .run()
            .map_err(|e| format!("second: {e}"))?;
        (first.proof().as_bytes() == second.proof().as_bytes())
            .then_some(())
            .ok_or_else(|| "proof of execution not deterministic".to_string())
    }

    #[test]
    fn proof_changes_with_direction() -> Result<(), String> {
        let degree = PolyDegreeLog2::new(2).map_err(|e| format!("degree: {e}"))?;
        let coeffs = vec![1u64, 2, 3, 4];
        let fwd = compute_wasm(
            Field::Goldilocks,
            Direction::Forward,
            degree,
            coeffs.clone(),
        )
        .run()
        .map_err(|e| format!("forward: {e}"))?;
        let inv = compute_wasm(Field::Goldilocks, Direction::Inverse, degree, coeffs)
            .run()
            .map_err(|e| format!("inverse: {e}"))?;
        (fwd.proof().as_bytes() != inv.proof().as_bytes())
            .then_some(())
            .ok_or_else(|| "proof should differ between Forward and Inverse".to_string())
    }
}
