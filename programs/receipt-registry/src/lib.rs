//! Receipt-registry program: emits one event per paid NTT call.
//!
//! State is intentionally event-only.  The event log is the receipt
//! registry; no per-receipt account is allocated, which keeps storage
//! rent at zero and lets the worker emit thousands of receipts per day
//! within a faucet-funded budget.

use anchor_lang::prelude::*;

declare_id!("Recpt11111111111111111111111111111111111111");

#[program]
pub mod receipt_registry {
    use super::{EmitReceipt, ReceiptEmitted};
    use anchor_lang::prelude::{Context, Result};

    /// Emit one receipt event for a paid NTT call.
    pub fn emit(
        ctx: Context<EmitReceipt>,
        job_id: [u8; 16],
        caller_addr_hash: [u8; 32],
        payment_tx: [u8; 64],
        proof_of_execution: [u8; 32],
        timestamp: u64,
        backend_tag: u8,
    ) -> Result<()> {
        emit!(ReceiptEmitted {
            emitter: ctx.accounts.emitter.key(),
            job_id,
            caller_addr_hash,
            payment_tx,
            proof_of_execution,
            timestamp,
            backend_tag,
        });
        Ok(())
    }
}

/// Accounts required to emit a receipt.  No state is mutated; only the
/// emitter's signature is checked so the registry can be filtered by
/// emitter when indexing events off-chain.
#[derive(Accounts)]
pub struct EmitReceipt<'info> {
    pub emitter: Signer<'info>,
}

/// Receipt event mirroring the worker-side `Receipt` newtype.
///
/// `backend_tag` matches `ntt_x402_worker::types::Backend`:
/// `0` Wasm, `1` Donor, `2` CloudFpga.
#[event]
pub struct ReceiptEmitted {
    pub emitter: Pubkey,
    pub job_id: [u8; 16],
    pub caller_addr_hash: [u8; 32],
    pub payment_tx: [u8; 64],
    pub proof_of_execution: [u8; 32],
    pub timestamp: u64,
    pub backend_tag: u8,
}
