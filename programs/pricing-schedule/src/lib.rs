//! Pricing-schedule program: a single PDA holding the immutable tier
//! table.  `initialize` is callable once; the worker reads the table
//! and the off-chain `quote::quote_for` mirror is verified against it
//! on each deployment.

use anchor_lang::prelude::*;

declare_id!("Prc11111111111111111111111111111111111111111");

const SCHEDULE_SEED: &[u8] = b"schedule";

#[program]
pub mod pricing_schedule {
    use super::{Initialize, ScheduleAccount, SCHEDULE_SEED};
    use anchor_lang::prelude::{Context, Result};

    /// Initialize the immutable schedule.  Tiers are
    /// `(max_log2, micros_usdc)` pairs in ascending order of
    /// `max_log2`.
    pub fn initialize(ctx: Context<Initialize>, tiers: [Tier; 4]) -> Result<()> {
        let schedule = &mut ctx.accounts.schedule;
        schedule.tiers = tiers;
        schedule.bump = ctx.bumps.schedule;
        Ok(())
    }
}

#[account]
pub struct ScheduleAccount {
    pub tiers: [Tier; 4],
    pub bump: u8,
}

impl ScheduleAccount {
    pub const SIZE: usize = 8 + (4 * Tier::SIZE) + 1;
}

#[derive(AnchorSerialize, AnchorDeserialize, Copy, Clone, Default)]
pub struct Tier {
    pub max_log2: u8,
    pub micros_usdc: u64,
}

impl Tier {
    pub const SIZE: usize = 1 + 8;
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = deployer,
        space = ScheduleAccount::SIZE,
        seeds = [SCHEDULE_SEED],
        bump,
    )]
    pub schedule: Account<'info, ScheduleAccount>,
    #[account(mut)]
    pub deployer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
