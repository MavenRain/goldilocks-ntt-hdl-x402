//! Donor-registry program: PDA-per-donor with revenue share in basis
//! points.  Splits are computed on-chain so donors trust the math and
//! the worker cannot silently change them.

use anchor_lang::prelude::*;

declare_id!("Dnr11111111111111111111111111111111111111111");

const DONOR_SEED: &[u8] = b"donor";
const MAX_BASIS_POINTS: u16 = 10_000;

#[program]
pub mod donor_registry {
    use super::{
        DonorAccount, DonorRegistryError, RegisterDonor, SetActive, SplitOwed, UpdateShare,
        DONOR_SEED, MAX_BASIS_POINTS,
    };
    use anchor_lang::prelude::{Context, Result};

    /// Register a new donor.  Allocates the PDA at
    /// `[DONOR_SEED, payout_pubkey]`.
    pub fn register(
        ctx: Context<RegisterDonor>,
        share_basis_points: u16,
        donor_id: u32,
    ) -> Result<()> {
        match () {
            () if share_basis_points > MAX_BASIS_POINTS => {
                Err(DonorRegistryError::ShareExceedsMaxBasisPoints.into())
            }
            () => {
                let donor = &mut ctx.accounts.donor;
                donor.payout = ctx.accounts.payout.key();
                donor.donor_id = donor_id;
                donor.share_basis_points = share_basis_points;
                donor.active = true;
                donor.bump = ctx.bumps.donor;
                Ok(())
            }
        }
    }

    /// Update the share for an existing donor.
    pub fn update_share(ctx: Context<UpdateShare>, share_basis_points: u16) -> Result<()> {
        match () {
            () if share_basis_points > MAX_BASIS_POINTS => {
                Err(DonorRegistryError::ShareExceedsMaxBasisPoints.into())
            }
            () => {
                let donor = &mut ctx.accounts.donor;
                donor.share_basis_points = share_basis_points;
                Ok(())
            }
        }
    }

    /// Toggle the donor's active flag.
    pub fn set_active(ctx: Context<SetActive>, active: bool) -> Result<()> {
        let donor = &mut ctx.accounts.donor;
        donor.active = active;
        Ok(())
    }

    /// Emit a `SplitOwed` event so the worker can settle the share to
    /// the donor's payout address from a separate USDC transfer.
    pub fn emit_split(
        ctx: Context<SetActive>,
        gross_micros_usdc: u64,
        job_id: [u8; 16],
    ) -> Result<()> {
        let donor = &ctx.accounts.donor;
        let donor_share = u128::from(gross_micros_usdc)
            .checked_mul(u128::from(donor.share_basis_points))
            .and_then(|n| n.checked_div(u128::from(MAX_BASIS_POINTS)))
            .ok_or(DonorRegistryError::ShareArithmeticOverflow)?;
        let donor_share_u64 =
            u64::try_from(donor_share).map_err(|_| DonorRegistryError::ShareArithmeticOverflow)?;
        emit!(SplitOwed {
            donor_id: donor.donor_id,
            payout: donor.payout,
            owed_micros_usdc: donor_share_u64,
            gross_micros_usdc,
            job_id,
        });
        Ok(())
    }
}

#[account]
pub struct DonorAccount {
    pub payout: Pubkey,
    pub donor_id: u32,
    pub share_basis_points: u16,
    pub active: bool,
    pub bump: u8,
}

impl DonorAccount {
    pub const SIZE: usize = 8 + 32 + 4 + 2 + 1 + 1;
}

#[derive(Accounts)]
#[instruction(share_basis_points: u16, donor_id: u32)]
pub struct RegisterDonor<'info> {
    #[account(
        init,
        payer = registrar,
        space = DonorAccount::SIZE,
        seeds = [DONOR_SEED, payout.key().as_ref()],
        bump,
    )]
    pub donor: Account<'info, DonorAccount>,
    /// CHECK: the payout pubkey is recorded as bytes only; no claim is
    /// made about the account's ownership program.
    pub payout: AccountInfo<'info>,
    #[account(mut)]
    pub registrar: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateShare<'info> {
    #[account(
        mut,
        seeds = [DONOR_SEED, donor.payout.as_ref()],
        bump = donor.bump,
    )]
    pub donor: Account<'info, DonorAccount>,
    pub registrar: Signer<'info>,
}

#[derive(Accounts)]
pub struct SetActive<'info> {
    #[account(
        mut,
        seeds = [DONOR_SEED, donor.payout.as_ref()],
        bump = donor.bump,
    )]
    pub donor: Account<'info, DonorAccount>,
    pub registrar: Signer<'info>,
}

#[event]
pub struct SplitOwed {
    pub donor_id: u32,
    pub payout: Pubkey,
    pub owed_micros_usdc: u64,
    pub gross_micros_usdc: u64,
    pub job_id: [u8; 16],
}

#[error_code]
pub enum DonorRegistryError {
    #[msg("share basis points exceeds 10000")]
    ShareExceedsMaxBasisPoints,
    #[msg("share arithmetic overflow")]
    ShareArithmeticOverflow,
}
