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

## Deploy

The worker compiles to `wasm32-unknown-unknown` via `worker-build`,
then deploys with `wrangler`.  The custom domain `ntt.isurvivable.cv`
is reached via a single CNAME record at Spaceship; the registrar does
not change.

### One-time setup

1.  **Install tooling.**

    ```bash
    cargo install worker-build
    npm install -g wrangler
    wrangler login
    ```

2.  **Set wrangler secrets** (not in `wrangler.toml`):

    ```bash
    wrangler secret put SOLANA_SIGNER_KEY    # base58 ed25519 keypair for receipt writes (v0.2)
    wrangler secret put SELLER_PUBKEY        # base58 mainnet pubkey that receives USDC
    ```

3.  **Spaceship CNAME.**  Log in to the Spaceship DNS panel for
    `isurvivable.cv` and add:

    | Type  | Host  | Value                       | TTL |
    | :---- | :---- | :-------------------------- | :-- |
    | CNAME | `ntt` | `ntt-x402.workers.dev`      | 300 |

    Cloudflare Workers will validate the CNAME on the next deploy and
    auto-issue the TLS certificate.  No nameserver transfer required.

### Deploy a new version

```bash
wrangler deploy
```

`wrangler.toml` already pins the route at `ntt.isurvivable.cv/*` and
ships against the CDP Solana-devnet facilitator
(`https://api.cdp.coinbase.com/x402/solana-devnet/v1`).  Flip
`FACILITATOR_URL` to the mainnet variant
(`https://api.cdp.coinbase.com/x402/solana/v1`) only after the devnet
end-to-end runs are clean.

### Local smoke test

```bash
wrangler dev
# in another shell
curl -i http://localhost:8787/                          # service descriptor
curl -i http://localhost:8787/.well-known/x402          # bazaar manifest
curl -i -X POST http://localhost:8787/ntt \              # 402 envelope
     -H 'Content-Type: application/json' \
     -d '{"field":"Goldilocks","direction":"Forward","degree_log2":2,"coefficients":[1,2,3,4]}'
```

The third call returns `402 Payment Required` with the x402 envelope.
A buyer with a wallet adds the `X-Payment` header (base64 of the
signed payment payload) and the worker awaits CDP `/verify`, runs
the NTT, and awaits CDP `/settle`.  The on-chain signature appears in
the response's `X-Payment-Response` header.

## License

Dual-licensed under either of:

- [MIT license](./LICENSE-MIT)
- [Apache License, Version 2.0](./LICENSE-APACHE)

at your option.
