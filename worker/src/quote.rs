//! Pricing-schedule logic.  Pure, no `Io`.
//!
//! The schedule is hard-coded for v0 and mirrors the on-chain
//! `pricing-schedule` Anchor program.  When the on-chain schedule is
//! later treated as the source of truth, this module becomes a cache
//! validator rather than a fallback.

use crate::error::OutOfTierError;
use crate::types::{MaxDegreeLog2, NttCallPriceMicrosUsdc, PolyDegreeLog2};

/// Compute the USDC price for a single NTT call at the requested
/// degree, gated against the active tier cap.
///
/// # Errors
///
/// Returns [`OutOfTierError`] when the requested `log2(n)` exceeds the
/// active tier's [`MaxDegreeLog2`] cap.
///
/// # Examples
///
/// ```
/// use ntt_x402_worker::quote::quote_for;
/// use ntt_x402_worker::types::{MaxDegreeLog2, PolyDegreeLog2};
///
/// let degree = PolyDegreeLog2::new(8).map_err(|_| ())?;
/// let cap = MaxDegreeLog2::new(12);
/// let price = quote_for(degree, cap).map_err(|_| ())?;
/// (price.get() == 1_000).then_some(()).ok_or(())?;
/// # Ok::<(), ()>(())
/// ```
pub fn quote_for(
    degree: PolyDegreeLog2,
    cap: MaxDegreeLog2,
) -> Result<NttCallPriceMicrosUsdc, OutOfTierError> {
    match () {
        () if degree.get() > cap.get() => Err(OutOfTierError::new(degree.get(), cap.get())),
        () => Ok(NttCallPriceMicrosUsdc::new(price_micros_for(degree.get()))),
    }
}

const fn price_micros_for(degree_log2: u8) -> u64 {
    match () {
        () if degree_log2 <= 8 => 1_000,
        () if degree_log2 <= 12 => 10_000,
        () if degree_log2 <= 16 => 100_000,
        () => 1_000_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::{String, ToString};

    #[test]
    fn small_degree_costs_one_milliusdc() -> Result<(), String> {
        let d = PolyDegreeLog2::new(8).map_err(|e| format!("degree: {e}"))?;
        let cap = MaxDegreeLog2::new(12);
        let price = quote_for(d, cap).map_err(|e| format!("quote: {e}"))?;
        (price.get() == 1_000)
            .then_some(())
            .ok_or_else(|| format!("expected 1000 micros, got {}", price.get()))
    }

    #[test]
    fn requesting_above_cap_rejects() -> Result<(), String> {
        let d = PolyDegreeLog2::new(20).map_err(|e| format!("degree: {e}"))?;
        let cap = MaxDegreeLog2::new(12);
        quote_for(d, cap)
            .err()
            .map(|_| ())
            .ok_or_else(|| "expected out-of-tier error".to_string())
    }
}
