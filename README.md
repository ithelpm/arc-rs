# arc-x402

Rust implementation of the [x402 HTTP payment protocol](https://x402.org) for the Arc testnet, using Circle USDC and Circle Gateway batch settlement. The core `arc-x402` library is framework-agnostic and can gate any HTTP resource behind nanopayments — `jellyfin-proxy` is one example use case that applies it to self-hosted media streaming.

## Repository layout

```
arc-x402/          — core library: EIP-712 signing, x402 protocol, Circle Gateway client
                     axum middleware included (feature = "axum-middleware", on by default)
crates/
  media-access/    — alloy 2.x bindings for MediaAccess.sol (on-chain access registry)
  jellyfin-proxy/  — full application: x402-gated Jellyfin reverse proxy
                     (multi-module: config, access, streaming, proxy, main)
contracts/
  MediaAccess.sol  — Soulbound access registry deployed on Arc testnet
  script/
    Deploy.s.sol   — Forge deployment script
examples/          — single-file CLI demos that call arc-x402 library functions
  seller/          — demo axum server with four protected endpoints
  buyer/           — CLI payment agent (pay / buy / stream subcommands)
  generate-keys/   — EOA keypair generator
```

## arc-x402 library

The library implements the full x402 flow and can be used independently of Jellyfin or any specific resource type:

```
Client                                    Server
  │                                         │
  │── GET /resource ───────────────────────►│
  │◄── 402  PAYMENT-REQUIRED (base64) ──────│
  │                                         │
  │  (sign EIP-3009 TransferWithAuthorization via Circle Gateway)
  │                                         │
  │── GET /resource                         │
  │   payment-signature: <base64> ─────────►│
  │                                         │── Gateway.settle() ──►
  │◄── 200  PAYMENT-RESPONSE (tx hash) ─────│◄─────────────────────
```

**Seller side** — server.rs:
- `build_requirements(price_usd, pay_to, endpoint)` — USD string → `PaymentRequirements`
- `build_buy_requirements(atomic, pay_to, url)` — atomic units, for media buy-to-access
- `build_chunk_requirements(atomic, pay_to, url)` — atomic units, for per-segment billing
- `handle_payment(sig_header, requirements, resource, gateway)` — validate + settle
- `payment_middleware` — axum Tower middleware; injects `PayerAddress` / `PaymentAmount` extensions

**Buyer side** — client.rs:
- `BuyerClient::pay(url, method, body)` — full 402 → sign → retry flow for any resource
- `BuyerClient::buy_access(base_url, item_id)` — buy-to-access with wallet fast-path
- `BuyerClient::sign_chunk(requirements, resource)` — sign a single streaming chunk
- `BuyerClient::create_stream_session(base_url, item_id)` — open a per-second billing session
- `BuyerClient::stream_chunk(base_url, item_id, session_id, req, res)` — pay + fetch chunk

## jellyfin-proxy use case

One concrete application of `arc-x402`: a reverse proxy that gates Jellyfin video streams behind USDC nanopayments, with two billing models and an on-chain permanent-access record.

```
┌─────────────┐   x402 protocol   ┌──────────────────┐   proxy   ┌──────────────┐
│  buyer CLI  │ ────────────────► │  jellyfin-proxy  │ ────────► │   Jellyfin   │
└─────────────┘                   └──────────────────┘           └──────────────┘
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

## Prerequisites

- **Rust** toolchain (stable) — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Foundry** — `curl -L https://foundry.paradigm.xyz | bash && foundryup`
- **arc-canteen CLI** — `npm i -g @the-canteen-dev/arc-canteen`

## Quick Start

### 1 — Generate keypairs

```bash
# Seller keypair (deploys contract, receives payments)
cargo run --example generate-keys
# → copy SELLER_ADDRESS and SELLER_PRIVATE_KEY into .env

# Buyer keypair (pays for resources)
cargo run --example generate-keys
# → copy BUYER_ADDRESS and BUYER_PRIVATE_KEY into .env
```

### 2 — Fund wallets with testnet USDC

Visit <https://faucet.circle.com>, select **Arc Testnet**, and request USDC for both addresses. The faucet deposits to the Circle Gateway balance — this is the balance the x402 protocol draws from.

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

### 5 — Try the seller demo server

The `seller` example exposes four demo endpoints wrapped with the arc-x402 axum middleware:

```bash
source .env
cargo run --example seller
# Listening on http://localhost:3000

# In another terminal:
cargo run --example buyer -- pay --base-url http://localhost:3000 --limit 0.05
```

### 6 — Run jellyfin-proxy

Requires a running Jellyfin instance. Set `JELLYFIN_URL` in `.env`.

```bash
source .env
cargo run --bin jellyfin-proxy
# Listening on http://localhost:3001

# Buy permanent access to a Jellyfin item ($5.00 one-time)
cargo run --example buyer -- buy --item-id <JELLYFIN_ITEM_ID> --base-url http://localhost:3001

# Stream 3 chunks using per-second billing ($0.001/chunk at default settings)
cargo run --example buyer -- stream --item-id <JELLYFIN_ITEM_ID> --chunks 3 --base-url http://localhost:3001
```

## Payment Modes (jellyfin-proxy)

### Per-second streaming billing

The buyer opens a session (`POST /api/stream/session`). Each `GET /Videos/{id}/stream?session_id=…` must carry a signed `payment-signature` for one chunk. The proxy time-limits the streamed response to `STREAM_CHUNK_SECS` seconds so the buyer pays proportionally for what they watch.

**Chunk price** = `STREAM_RATE_PER_SEC_ATOMIC × STREAM_CHUNK_SECS` atomic USDC.

### One-time purchase (buy-to-access)

A single x402 payment of `BUY_PRICE_ATOMIC` grants the buyer's wallet permanent access. The proxy writes a Soulbound record to `MediaAccess.sol` via `grantAccess(wallet, contentId)` in a `tokio::spawn` background task — the HTTP response is never blocked by blockchain confirmation latency.

### Fast-path cache (returning buyers)

On every request the proxy checks a local SQLite cache (1-hour TTL). On miss it falls back to `hasAccess(wallet, contentId)` via `eth_call`. If either returns `true`, the video is proxied immediately with no payment.

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `SELLER_ADDRESS` | — | Ethereum address that receives payments |
| `SELLER_PRIVATE_KEY` | — | Signs `grantAccess` transactions |
| `BUYER_PRIVATE_KEY` | — | Used by the buyer CLI |
| `ARC_RPC_URL` | `https://rpc.testnet.arc.network` | Arc testnet JSON-RPC |
| `MEDIA_ACCESS_CONTRACT` | — | Deployed `MediaAccess.sol` address |
| `JELLYFIN_URL` | — | Upstream Jellyfin base URL |
| `PORT` | `3001` | Proxy listen port |
| `DATABASE_URL` | `sqlite://./data/access.db` | SQLite access cache |
| `STREAM_RATE_PER_SEC_ATOMIC` | `100` | Billing rate (100 = $0.0001/sec) |
| `STREAM_CHUNK_SECS` | `10` | Seconds per billed segment |
| `BUY_PRICE_ATOMIC` | `5000000` | One-time purchase price ($5.00) |

> **Atomic units**: 1 USDC = 1 000 000 atomic. Use `cargo run --example generate-keys` to create keypairs; fund at <https://faucet.circle.com>.
