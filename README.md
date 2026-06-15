# arc-x402-media

x402 payment middleware for self-hosted media platforms — gate any Jellyfin video stream behind Circle USDC nanopayments on the Arc testnet, with on-chain permanent-access records written to a Soulbound smart contract.

## Architecture

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

**arc-x402** (library crate) — EIP-712 signing, EIP-3009 TransferWithAuthorization, Circle Gateway REST client, server middleware, and buyer client. Reused by both the proxy and the CLI.

**jellyfin-proxy** (binary crate) — axum reverse proxy that enforces x402 payments on `/Videos/{id}/stream`. Maintains a SQLite cache of granted access records to avoid re-charging returning buyers.

**MediaAccess.sol** — Soulbound Solidity contract deployed on Arc testnet. Records permanent, non-transferable access grants on-chain. The proxy writes grants fire-and-forget (non-blocking) via `tokio::spawn`.

## Prerequisites

- **Rust** toolchain (stable) — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Foundry** — `curl -L https://foundry.paradigm.xyz | bash && foundryup`
- **arc-canteen CLI** — `npm i -g @the-canteen-dev/arc-canteen` (provides authenticated Arc testnet RPC)

## Quick Start

### 1 — Generate keypairs

```bash
# Seller keypair (deploys contract, receives payments)
cargo run --example generate-keys
# → copy SELLER_ADDRESS and SELLER_PRIVATE_KEY into .env

# Buyer keypair (pays for streams)
cargo run --example generate-keys
# → copy BUYER_ADDRESS and BUYER_PRIVATE_KEY into .env
```

### 2 — Fund wallets with testnet USDC

Visit <https://faucet.circle.com>, select **Arc Testnet**, and request USDC for both addresses. The faucet deposits to the Circle Gateway balance (not the raw wallet) — this is the balance the x402 protocol draws from.

### 3 — Obtain an Arc testnet RPC URL

```bash
arc-canteen login          # GitHub device-flow → stores a session token
arc-canteen rpc eth_chainId  # should return 0x4cef52 (= 5042002)
```

The public endpoint `https://rpc.testnet.arc.network` also works and requires no auth. Set `ARC_RPC_URL` in `.env`.

### 4 — Deploy MediaAccess.sol

```bash
cp .env.example .env       # fill in SELLER_ADDRESS, SELLER_PRIVATE_KEY, ARC_RPC_URL

forge script contracts/script/Deploy.s.sol \
  --rpc-url $ARC_RPC_URL \
  --private-key $SELLER_PRIVATE_KEY \
  --broadcast

# The script prints the deployed address — paste it into .env as MEDIA_ACCESS_CONTRACT
```

### 5 — Start the seller demo server

The seller server exposes four demo endpoints protected by the arc-x402 axum middleware:

```bash
source .env
cargo run --example seller
# Listening on http://localhost:3000
```

### 6 — Start jellyfin-proxy

Requires a running Jellyfin instance. Set `JELLYFIN_URL` in `.env` to its base URL.

```bash
source .env
cargo run --bin jellyfin-proxy
# Listening on http://localhost:3001
# Upstream Jellyfin: http://localhost:8096
```

### 7 — Run the buyer CLI

```bash
source .env

# Loop over the seller's four demo endpoints until $0.05 is spent
cargo run --example buyer -- pay --base-url http://localhost:3000 --limit 0.05

# Buy permanent access to a Jellyfin item (one-time $5.00 payment)
cargo run --example buyer -- buy --item-id <JELLYFIN_ITEM_ID> --base-url http://localhost:3001

# Stream 3 chunks of a video using per-second billing ($0.001/chunk at default settings)
cargo run --example buyer -- stream --item-id <JELLYFIN_ITEM_ID> --chunks 3 --base-url http://localhost:3001
```

## Payment Modes

### Per-second streaming billing

The buyer opens a billing session (`POST /api/stream/session`) which pre-computes the `PaymentRequirements` for one chunk. Each subsequent `GET /Videos/{id}/stream?session_id=…` request must carry a fresh `payment-signature` header covering exactly one chunk price. The proxy time-limits the streamed response to `STREAM_CHUNK_SECS` seconds using a `take_until` deadline, so the buyer pays proportionally for what they watch.

**Chunk price** = `STREAM_RATE_PER_SEC_ATOMIC × STREAM_CHUNK_SECS` atomic USDC units.

### One-time purchase (buy-to-access)

A single x402 payment of `BUY_PRICE_ATOMIC` atomic USDC units grants the buyer's wallet permanent access. The proxy writes a Soulbound record to `MediaAccess.sol` via `grantAccess(wallet, contentId)` in a background task (`tokio::spawn`) so the HTTP response is never blocked by blockchain confirmation latency.

### Fast-path cache (returning buyers)

On every request the proxy checks a local SQLite cache (1-hour TTL) keyed by `(wallet, contentId)`. On cache miss it falls back to `hasAccess(wallet, contentId)` on-chain via `eth_call`. If either returns `true`, the video is proxied immediately with no payment required. The buyer should include their address as the `wallet` query parameter (the `buyer buy` subcommand does this automatically) so the fast-path fires correctly.

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `SELLER_ADDRESS` | — | Ethereum address that receives payments |
| `SELLER_PRIVATE_KEY` | — | Private key for signing `grantAccess` transactions |
| `BUYER_PRIVATE_KEY` | — | Private key used by the buyer CLI |
| `ARC_RPC_URL` | `https://rpc.testnet.arc.network` | Arc testnet JSON-RPC endpoint |
| `MEDIA_ACCESS_CONTRACT` | — | Deployed `MediaAccess.sol` address |
| `JELLYFIN_URL` | — | Upstream Jellyfin base URL (no trailing slash) |
| `PORT` | `3001` | Port the proxy listens on |
| `DATABASE_URL` | `sqlite://./data/access.db` | SQLite access cache path |
| `STREAM_RATE_PER_SEC_ATOMIC` | `100` | Billing rate in atomic USDC/sec (100 = $0.0001/sec) |
| `STREAM_CHUNK_SECS` | `10` | Seconds per billed video segment |
| `BUY_PRICE_ATOMIC` | `5000000` | One-time purchase price in atomic USDC ($5.00) |

> **Atomic units**: 1 USDC = 1 000 000 atomic units. Use `cargo run --example generate-keys` to create keypairs; fund them at <https://faucet.circle.com>.
