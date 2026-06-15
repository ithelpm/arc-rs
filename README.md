# arc-x402

Rust implementation of the [x402 HTTP payment protocol](https://x402.org) for the Arc testnet, using Circle USDC and Circle Gateway batch settlement. The core `arc-x402` library is framework-agnostic and can gate any HTTP resource behind nanopayments.

## Repository layout

```
arc-x402/                  — core library: EIP-712 signing, x402 protocol, Circle Gateway client
                             axum middleware included (feature = "axum-middleware", on by default)
crates/
  media-access/            — alloy 2.x bindings for MediaAccess.sol (on-chain access registry)
  access-gated-server/     — reference implementation: x402-gated content server
                             demonstrates both billing modes + soulbound access + SQLite fast-path
contracts/
  MediaAccess.sol          — Soulbound access registry deployed on Arc testnet
  script/
    Deploy.s.sol           — Forge deployment script
examples/                  — single-file CLI demos that call arc-x402 library functions
  seller/                  — demo axum server with four protected endpoints
  buyer/                   — CLI payment agent (pay / buy / stream subcommands)
  generate-keys/           — EOA keypair generator
```

## arc-x402 library

The library implements the full x402 flow and can be used independently of any specific backend:

```
Client                                    Server
  │                                         │
  │── GET /resource ───────────────────────►│
  │◄── 402  payment-required (base64) ──────│
  │                                         │
  │  (sign EIP-3009 TransferWithAuthorization via Circle Gateway)
  │                                         │
  │── GET /resource                         │
  │   payment-signature: <base64> ─────────►│
  │                                         │── Gateway.settle() ──►
  │◄── 200  payment-response (tx hash) ─────│◄─────────────────────
```

**Seller side** — server.rs:
- `build_requirements(price_usd, pay_to, endpoint)` — USD string → `PaymentRequirements`
- `build_buy_requirements(atomic, pay_to, url)` — atomic units, one-time purchase
- `build_chunk_requirements(atomic, pay_to, url)` — atomic units, per-chunk metered billing
- `handle_payment(sig_header, requirements, resource, gateway)` — validate + settle
- `payment_middleware` — axum Tower middleware; injects `PayerAddress` / `PaymentAmount` extensions

**Buyer side** — client.rs:
- `BuyerClient::pay(url, method, body)` — full 402 → sign → retry flow for any resource
- `BuyerClient::buy_access(base_url, item_id)` — buy-to-access with wallet fast-path
- `BuyerClient::sign_chunk(requirements, resource)` — sign a single streaming chunk
- `BuyerClient::create_stream_session(base_url, item_id)` — open a per-chunk billing session
- `BuyerClient::stream_chunk(base_url, item_id, session_id, req, res)` — pay + fetch chunk

## access-gated-server — reference implementation

A generic HTTP server that gates demo JSON content behind x402 payments. It has no dependencies on any particular upstream service — it demonstrates the full arc-x402 protocol flow with two billing modes and on-chain permanent access records.

```
┌─────────────┐   x402 protocol   ┌──────────────────────┐
│  buyer CLI  │ ────────────────► │  access-gated-server │
└─────────────┘                   └──────────────────────┘
                                          │  settle
                                  ┌───────▼───────┐
                                  │ Circle Gateway │
                                  └───────────────┘
                                          │  grantAccess (tokio::spawn)
                                  ┌───────▼───────────┐
                                  │  MediaAccess.sol  │
                                  │  (Arc testnet)    │
                                  └───────────────────┘
```

**Routes:**
- `POST /api/session` — create a metered billing session, returns `session_id` + `chunk_price_atomic`
- `GET /content/{item_id}` — protected content endpoint
  - `?wallet=…` → buy-to-access mode (one-time payment → permanent soulbound record)
  - `?session_id=…` → per-chunk metered mode (each request requires fresh payment-signature)

This pattern can be applied to any protected resource: video streams, API calls, AI inference, file downloads, etc.

## Prerequisites

- **Rust** toolchain (stable) — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Foundry** — `curl -L https://foundry.paradigm.xyz | bash && foundryup`
- **arc-canteen CLI** — `npm i -g @the-canteen-dev/arc-canteen`
- **Circle CLI** (optional, recommended) — `npm i -g @circle-fin/cli` — lets you pay with a managed Agent Wallet instead of a raw private key

## Quick Start

### 1 — Generate keypairs

```bash
# Seller keypair (deploys contract, receives payments)
cargo run --example generate-keys
# → copy SELLER_ADDRESS and SELLER_PRIVATE_KEY into .env

# Buyer keypair — two options:

# Option A: self-custodial EOA (raw private key, used by examples/buyer)
cargo run --example generate-keys
# → copy BUYER_ADDRESS and BUYER_PRIVATE_KEY into .env

# Option B: Circle Agent Wallet (auto-funded, compliance-aware, used with Circle CLI)
circle wallet login you@email.com --testnet
circle wallet list --chain ARC-TESTNET --type agent
# → copy the returned address into .env as BUYER_ADDRESS
```

### 2 — Fund wallets with testnet USDC

```bash
# Seller wallet — fund via Circle faucet (Gateway balance)
# Visit https://faucet.circle.com, select Arc Testnet, enter SELLER_ADDRESS

# Buyer wallet (Option A — EOA)
# Same: visit https://faucet.circle.com, enter BUYER_ADDRESS

# Buyer wallet (Option B — Circle Agent Wallet)
# Auto-funded with 20 USDC when you ran circle wallet login above.
# Confirm the Gateway balance:
circle gateway balance --address 0xYourAgentWallet --chain ARC-TESTNET
```

### 3 — Obtain an Arc testnet RPC URL

```bash
arc-canteen login            # GitHub device-flow → stores a session token
arc-canteen rpc eth_chainId  # should return 0x4cef52 (= 5042002)
```

The public endpoint `https://rpc.testnet.arc.network` also works without auth. Set `ARC_RPC_URL` in `.env`.

### 4 — Deploy MediaAccess.sol

```bash
cp .env.example .env   # fill in SELLER_ADDRESS, SELLER_PRIVATE_KEY, ARC_RPC_URL

forge script contracts/script/Deploy.s.sol \
  --rpc-url $ARC_RPC_URL \
  --private-key $SELLER_PRIVATE_KEY \
  --broadcast

# The script prints the deployed address — paste it into .env as MEDIA_ACCESS_CONTRACT
```

### 5 — Run the seller demo server

The `seller` example exposes four demo endpoints wrapped with the arc-x402 axum middleware:

```bash
source .env
cargo run --example seller
# Listening on http://localhost:3000

# Pay with Rust buyer CLI:
cargo run --example buyer -- pay --base-url http://localhost:3000 --limit 0.05

# Pay with Circle CLI (Agent Wallet):
circle services pay http://localhost:3000/hello \
  --address 0xYourAgentWallet --chain ARC-TESTNET
```

### 6 — Run access-gated-server and test the full closed-loop

```bash
source .env
cargo run --bin access-gated-server
# access-gated-server listening on 0.0.0.0:3001
```

**Option A — Rust buyer CLI:**

```bash
# Buy permanent access to a content item ($5.00 one-time)
cargo run --example buyer -- buy --item-id demo-episode-1 --base-url http://localhost:3001

# Stream 3 metered chunks ($0.001/chunk)
cargo run --example buyer -- stream --item-id demo-episode-1 --chunks 3 --base-url http://localhost:3001

# Second buy request — fast-path fires immediately, no payment charged
cargo run --example buyer -- buy --item-id demo-episode-1 --base-url http://localhost:3001
```

**Option B — Circle CLI (Agent Wallet):**

```bash
# Inspect the 402 payment requirements before paying
circle services inspect http://localhost:3001/content/demo-episode-1

# Buy-to-access: pay once, receive permanent soulbound access
circle services pay http://localhost:3001/content/demo-episode-1 \
  --address 0xYourAgentWallet \
  --chain ARC-TESTNET

# Metered streaming: create a session first, then pay per chunk
SESSION=$(curl -s -X POST http://localhost:3001/api/session \
  -H "Content-Type: application/json" \
  -d '{"wallet":"0xYourAgentWallet","item_id":"demo-episode-1"}' \
  | jq -r '.session_id')

circle services pay \
  "http://localhost:3001/content/demo-episode-1?session_id=$SESSION" \
  --address 0xYourAgentWallet \
  --chain ARC-TESTNET
```

The closed-loop demo shows:
1. **First buy** → 402 response → buyer pays → `grantAccess` submitted on Arc testnet in background → content returned
2. **Stream chunks** → each request requires a fresh payment-signature → metered billing settled per chunk
3. **Second buy** → SQLite cache / `hasAccess` check → content returned immediately with no payment

## Payment Modes

### Per-chunk metered billing

The buyer creates a session (`POST /api/session`) and receives a `session_id` plus the per-chunk price. Each subsequent `GET /content/{item_id}?session_id=…` must carry a signed `payment-signature` header for that chunk. No session state is tracked server-side beyond the payment requirements — each request is independently settled.

### Buy-to-access (one-time purchase)

A single x402 payment of `BUY_PRICE_ATOMIC` grants permanent access. The server writes a soulbound record to `MediaAccess.sol` via `grantAccess(wallet, contentId)` in a `tokio::spawn` background task — the HTTP response is never blocked by blockchain confirmation latency.

### Fast-path cache (returning buyers)

On every buy-mode request the server checks a local SQLite cache (1-hour TTL). On miss it falls back to `hasAccess(wallet, contentId)` via `eth_call`. If either returns `true`, content is delivered immediately with no payment.

## Paying with Circle CLI

`access-gated-server` speaks standard x402, so **any x402-compatible client works out of the box** — including Circle's official CLI (`@circle-fin/cli`). No code changes are needed on the server side.

### Setup (one-time)

```bash
npm install -g @circle-fin/cli

# Create an Agent Wallet on Arc testnet — auto-funded with 20 USDC from the Circle faucet
circle wallet login you@email.com --testnet
circle wallet list --chain ARC-TESTNET --type agent
# → note your agent wallet address (0x...)

# Confirm Gateway balance (the x402 protocol draws from this)
circle gateway balance --address 0xYourAgentWallet --chain ARC-TESTNET
```

### Inspect a protected endpoint before paying

```bash
# Shows the 402 PaymentRequirements: price, network, pay-to address, timeout
circle services inspect http://YOUR_SERVER/content/demo-episode-1
```

### Pay for buy-to-access content

```bash
circle services pay http://YOUR_SERVER/content/demo-episode-1 \
  --address 0xYourAgentWallet \
  --chain ARC-TESTNET \
  --max-amount 6.00
```

`circle services pay` handles the full x402 flow automatically: initial probe → 402 → EIP-3009 sign → retry with `payment-signature` header → parse `payment-response`.

### Pay for a metered streaming chunk

```bash
# 1. Open a billing session
SESSION=$(curl -s -X POST http://YOUR_SERVER/api/session \
  -H "Content-Type: application/json" \
  -d '{"wallet":"0xYourAgentWallet","item_id":"demo-episode-1"}' \
  | jq -r '.session_id')

# 2. Pay for each chunk (repeat as many times as needed)
circle services pay \
  "http://YOUR_SERVER/content/demo-episode-1?session_id=$SESSION" \
  --address 0xYourAgentWallet \
  --chain ARC-TESTNET \
  --max-amount 0.01
```

### Check Gateway balance and transaction history

```bash
circle gateway balance --address 0xYourAgentWallet --chain ARC-TESTNET
circle transaction list --address 0xYourAgentWallet --chain ARC-TESTNET
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `SELLER_ADDRESS` | — | Ethereum address that receives payments |
| `SELLER_PRIVATE_KEY` | — | Signs `grantAccess` transactions |
| `BUYER_PRIVATE_KEY` | — | Used by the buyer CLI |
| `ARC_RPC_URL` | `https://rpc.testnet.arc.network` | Arc testnet JSON-RPC |
| `MEDIA_ACCESS_CONTRACT` | — | Deployed `MediaAccess.sol` address |
| `PORT` | `3001` | Server listen port |
| `DATABASE_URL` | `sqlite://./data/access.db` | SQLite access cache |
| `CHUNK_PRICE_ATOMIC` | `1000` | Per-chunk price ($0.001 USDC) |
| `BUY_PRICE_ATOMIC` | `5000000` | One-time purchase price ($5.00 USDC) |

> **Atomic units**: 1 USDC = 1 000 000 atomic. Use `cargo run --example generate-keys` to create keypairs; fund at <https://faucet.circle.com>.
