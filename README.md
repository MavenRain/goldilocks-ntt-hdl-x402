# goldilocks-ntt-hdl-x402

A free-to-deploy, free-to-run x402 wrapper that exposes Goldilocks (and later
BabyBear) NTT compute as paid HTTP endpoints settled in USDC.

## Tiers

| Tier            | Backend                                       | Max `log2(n)` | Status |
| :-------------- | :-------------------------------------------- | :-----------: | :----- |
| Free            | Verilator-compiled WASM of `goldilocks-ntt-hdl` |     12      | v0     |
| Donor (mesh)    | Donor's home FPGA over Cloudflare Tunnel        |     20      | v0.2   |
| Cloud (premium) | AWS EC2 f2.6xlarge spot (Virtex VU47P)          |     22+     | v0.3 (unlocks at ~$50/day USDC inflow) |

## Architecture

```text
[Agent] --HTTP--> [Cloudflare Worker @ ntt.isurvivable.cv]
                           |
                           +--(simulate RTL)--> [Verilator-WASM blob]
                           |
                           +--(read pricing, write receipt)
                           |       |
                           |       v
                           |   [Solana devnet: receipt-registry,
                           |                    donor-registry,
                           |                    pricing-schedule]
                           |
                           +--(verify + settle)
                                   |
                                   v
                              [Solana mainnet x402 facilitator]
                                   |
                                   v
                              [Seller wallet receives USDC]
```

State on Solana devnet (free), payment on Solana mainnet (Coinbase CDP
facilitator eats gas), compute on Cloudflare Workers free tier.

## Layout

```text
/Cargo.toml              workspace root (worker member only)
/wrangler.toml           Cloudflare Workers deployment
/Anchor.toml             Solana devnet program registry
/worker/                 Rust crate compiled to wasm32 for CF Workers
/programs/               Anchor programs deployed to Solana devnet
  receipt-registry/      per-call receipts as on-chain events
  donor-registry/        donor URLs + revenue-share PDAs
  pricing-schedule/      immutable pricing tier config
/scripts/                Verilator-WASM build pipeline (placeholder)
```

## Conventions

This repo follows the `comp-cat-rs` foundation conventions documented in
[`CLAUDE.md`](./CLAUDE.md): newtypes for all domain primitives, hand-rolled
`Error` enum, no `unwrap`/`expect`/`panic`/`assert`, no `for`/`while`/`return`,
no naked `as`, no public fields, no interior mutability, no `dyn` outside the
`comp-cat-rs` carve-out, exhaustive match (no `_` wildcards on enums), and
combinators over pattern matching on `Option`/`Result`.

## License

Dual-licensed under either of:

- [MIT license](./LICENSE-MIT)
- [Apache License, Version 2.0](./LICENSE-APACHE)

at your option.
