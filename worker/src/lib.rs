//! # ntt-x402-worker
//!
//! `Io`-pure core of the x402-gated NTT compute service.
//!
//! The crate is intentionally free of any Cloudflare Workers async
//! surface: all logic is expressed as `comp_cat_rs::effect::Io`
//! pipelines that defer side effects to a single boundary call.  The
//! eventual Workers entry point (added in v0.1) will be a thin
//! `#[event(fetch)]` shim that forwards request bytes into
//! [`handler::serve`] and runs the resulting `Io` once at the edge.
//!
//! ## Modules
//!
//! - [`error`]: the project-wide hand-rolled `Error` enum.
//! - [`types`]: newtypes and sum types for the NTT request/receipt domain.
//! - [`quote`]: pricing-schedule logic, pure.
//! - [`handler`]: `serve(request_bytes) -> Io<Error, ResponseBytes>` entry point.
//! - [`x402`]: Solana-mainnet x402 facilitator client (verify + settle).
//! - [`registry`]: Solana-devnet receipt + donor + pricing registry client.
//! - [`backend`]: NTT compute backends.  v0 ships the WASM Verilator stub;
//!   donor mesh (v0.2) and cloud FPGA (v0.3) follow.
//! - [`faucet`]: devnet airdrop puller.

#![cfg_attr(all(not(test), not(target_arch = "wasm32")), no_std)]
#![allow(clippy::module_name_repetitions)]

extern crate alloc;

pub mod backend;
pub mod error;
pub mod faucet;
pub mod handler;
pub mod quote;
pub mod registry;
pub mod types;
pub mod x402;

#[cfg(target_arch = "wasm32")]
pub mod edge;
