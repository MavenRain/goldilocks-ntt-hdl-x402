//! `serve(request_bytes) -> Io<Error, ResponseBytes>` entry point.
//!
//! All logic is composed inside `Io` and never executed here.  The
//! eventual Cloudflare Workers `#[event(fetch)]` shim runs the
//! returned `Io` exactly once at the edge, satisfying the
//! delay-`run` rule.
//!
//! v0 wires the pure parse and quote stages and stubs the rest.  The
//! stubs return [`crate::error::Error::NotYetImplemented`] so an
//! integration smoke test can confirm the boundary handler reaches the
//! correct subsystem before the real Solana RPC and facilitator
//! clients are filled in.

use crate::error::{Error, NotYetImplemented};
use crate::types::{RequestBytes, ResponseBytes};
use comp_cat_rs::effect::io::Io;

/// Top-level entry point.  Pure construction of an `Io` pipeline; no
/// side effects until the boundary calls `.run()`.
#[must_use]
pub fn serve(_request: RequestBytes) -> Io<Error, ResponseBytes> {
    Io::suspend(|| {
        Err(Error::NotYetImplemented(NotYetImplemented::new(
            "handler::serve full pipeline (parse + quote + verify + compute + settle + receipt)",
        )))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn serve_returns_not_yet_implemented_for_v0() -> Result<(), &'static str> {
        let r = serve(RequestBytes::new(vec![])).run();
        r.err()
            .and_then(|e| match e {
                Error::NotYetImplemented(_) => Some(()),
                Error::Parse(_)
                | Error::OutOfTier(_)
                | Error::Settlement(_)
                | Error::Registry(_)
                | Error::Backend(_)
                | Error::Faucet(_) => None,
            })
            .ok_or("expected NotYetImplemented")
    }
}
