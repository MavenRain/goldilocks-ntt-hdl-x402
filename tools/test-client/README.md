# ntt-x402-test-client

Smoke-test buyer that signs a devnet USDC envelope with `@x402/svm`
and calls `POST /ntt` on the live Worker.  Verifies the full
verify + compute + settle path against the CDP Solana-devnet
facilitator.

## Setup

```bash
cd tools/test-client
npm install
```

Create a dedicated buyer keypair (separate from the seller wallet):

```bash
solana-keygen new --no-bip39-passphrase \
  -o ~/.config/solana/x402-buyer-devnet.json
solana-keygen pubkey ~/.config/solana/x402-buyer-devnet.json
```

Fund it.  Both calls below take the printed pubkey:

```bash
# Devnet SOL for SPL transfer gas (0.5 SOL is overkill but fine)
solana airdrop 0.5 <buyer-pubkey> --url devnet
```

Then visit [faucet.circle.com](https://faucet.circle.com), pick
`Solana Devnet`, paste the same pubkey, and request 20 USDC.

## Run

```bash
npm start
```

Or with explicit overrides:

```bash
X402_ENDPOINT=https://ntt-x402.isurvivable.workers.dev/ntt \
  X402_BUYER_KEYPAIR=~/.config/solana/x402-buyer-devnet.json \
  npm start
```

The script prints the buyer pubkey, the request body, the response
status with elapsed milliseconds, the `X-Payment-Response` settlement
signature, and the parsed NTT response body.  Exit code is 0 on
`response.ok`, 1 otherwise.

## Expected output (happy path)

```text
endpoint: https://ntt-x402.isurvivable.workers.dev/ntt
buyer keypair: /Users/.../x402-buyer-devnet.json
buyer pubkey: <base58>
request: {"field":"Goldilocks","direction":"Forward","degree_log2":2,"coefficients":[1,2,3,4]}
status: 200 (1240 ms)
X-Payment-Response: {"network":"solana","transaction":"<base58 sig>"}
body:
{
  "output": ["10", "18446744069414584319", ...],
  "proof_of_execution": "<64 hex chars>",
  "backend": "Wasm",
  "field": "Goldilocks",
  "direction": "Forward",
  "degree_log2": 2
}
```

`output` entries are JSON strings (not bare numbers) because Goldilocks
values can exceed JavaScript's safe-integer range (2^53 - 1).  Parse them
with `BigInt(s)` when you need to do arithmetic; treat them as opaque
labels otherwise.  The `proof_of_execution` digest is invariant under
this encoding because it's computed over the canonical u64 little-endian
bytes, not the JSON serialization.

The first call is slower (~1-2 s) because Cloudflare Workers cold-starts
the WASM module; subsequent calls drop to sub-200 ms.

## Failure modes worth knowing

| Symptom                                             | Cause                                                                                                  |
| :-------------------------------------------------- | :----------------------------------------------------------------------------------------------------- |
| `expected 64-byte Solana keypair`                   | Keypair file is not the standard `solana-keygen` JSON format.                                          |
| `status: 402` and no settlement header              | Buyer wallet is empty; fund USDC and SOL.                                                              |
| `status: 402` with `verify rejected`                | CDP facilitator rejected the envelope.  Check `FACILITATOR_URL` matches the network of the buyer wallet. |
| `status: 502` with `facilitator transport error`    | CDP facilitator unreachable; transient, retry.                                                         |
| `status: 500` with `backend error`                  | Bug in the Worker; check `wrangler tail` for the panic.                                                |
