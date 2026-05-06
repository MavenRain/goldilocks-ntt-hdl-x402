// Smoke-test buyer for ntt-x402.
//
// Signs a devnet USDC envelope with @x402/svm and hits POST /ntt on
// the Worker.  On success, prints the settlement signature from the
// X-Payment-Response header and the NTT result body.
//
// Usage:
//   X402_BUYER_KEYPAIR=~/.config/solana/x402-buyer-devnet.json \
//     X402_ENDPOINT=https://ntt-x402.isurvivable.workers.dev/ntt \
//     npm start
//
// The buyer keypair file must be the standard Solana JSON format
// (a 64-byte array: 32-byte secret + 32-byte public).  Fund the
// pubkey on devnet with:
//   solana airdrop 0.5 <buyer-pubkey> --url devnet      # gas
//   # then visit https://faucet.circle.com (Solana Devnet)
//   # to drip 20 testnet USDC into the same pubkey

import { x402Client, wrapFetchWithPayment } from "@x402/fetch";
import { registerExactSvmScheme } from "@x402/svm/exact/client";
import { createKeyPairSignerFromBytes } from "@solana/kit";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

const DEFAULT_ENDPOINT = "https://ntt-x402.isurvivable.workers.dev/ntt";
const DEFAULT_KEYPAIR = path.join(
  os.homedir(),
  ".config",
  "solana",
  "x402-buyer-devnet.json",
);

const ENDPOINT = process.env.X402_ENDPOINT ?? DEFAULT_ENDPOINT;
const KEYPAIR_PATH = process.env.X402_BUYER_KEYPAIR ?? DEFAULT_KEYPAIR;

const REQUEST_BODY = {
  field: "Goldilocks",
  direction: "Forward",
  degree_log2: 2,
  coefficients: [1, 2, 3, 4],
};

async function readSolanaKeypair(filePath: string): Promise<Uint8Array> {
  const raw = await fs.promises.readFile(filePath, "utf8");
  const bytes = Uint8Array.from(JSON.parse(raw));
  if (bytes.length !== 64) {
    throw new Error(
      `expected 64-byte Solana keypair at ${filePath}, got ${bytes.length} bytes`,
    );
  }
  return bytes;
}

async function main(): Promise<void> {
  console.log(`endpoint: ${ENDPOINT}`);
  console.log(`buyer keypair: ${KEYPAIR_PATH}`);

  const keypairBytes = await readSolanaKeypair(KEYPAIR_PATH);
  const signer = await createKeyPairSignerFromBytes(keypairBytes);
  console.log(`buyer pubkey: ${signer.address}`);

  const client = new x402Client();
  registerExactSvmScheme(client, { signer });

  const fetchWithPayment = wrapFetchWithPayment(fetch, client);

  console.log(`request: ${JSON.stringify(REQUEST_BODY)}`);
  const t0 = Date.now();
  const response = await fetchWithPayment(ENDPOINT, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(REQUEST_BODY),
  });
  const elapsed = Date.now() - t0;
  console.log(`status: ${response.status} (${elapsed} ms)`);

  const settlement = response.headers.get("X-Payment-Response");
  if (settlement !== null) {
    console.log(`X-Payment-Response: ${settlement}`);
  } else {
    console.log("no X-Payment-Response header on response");
  }

  const text = await response.text();
  try {
    const parsed = JSON.parse(text);
    console.log(`body:\n${JSON.stringify(parsed, null, 2)}`);
  } catch {
    console.log(`body (raw):\n${text}`);
  }

  if (!response.ok) {
    process.exit(1);
  }
}

main().catch((err) => {
  console.error("error:", err);
  process.exit(1);
});
