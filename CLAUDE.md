# AGENTS.md

# Agent context for Arc + Circle

This repo bundles developer documentation and sample codebases for
working on the **Arc** Layer-1 blockchain and the **Circle** developer
platform (USDC, EURC, CCTP, wallets, gateway, smart-contract platform,
etc.). It is consumed by AI agents (Claude Code, Cursor, Cody,
Continue, …) and synced into `~/.arc-canteen/context/` by
`arc-canteen context sync`.

## Layout

```
docs/
  docs.arc.network/         # Arc chain + App Kit docs (mirrored, .md per page)
    llms.txt                # the upstream index this mirror was built from
    arc-chain.md
    arc/concepts/*.md
    arc/references/*.md
    arc/tutorials/*.md
    arc/tools/*.md
    app-kit/**/*.md
    ai/mcp.md
    build/*.md
    integrate/*.md
  developers.circle.com/    # Circle developer platform docs
    llms.txt
    api-reference.md
    agent-stack/**/*.md     # Agent Stack — Circle CLI, Agent Wallets, x402 payments
    ai/mcp.md               # Circle MCP server setup
    ai/skills.md            # Circle Skills overview (the SKILL.md files live in circlefin-skills/)
    cctp/**/*.md
    contracts/**/*.md
    cpn/**/*.md
    gateway/**/*.md
    paymaster/**/*.md
    stablecoins/**/*.md
    stablefx/**/*.md
    wallets/**/*.md
    xreserve/**/*.md
    openapi/*.yaml          # OpenAPI specs for the Circle APIs
  circlefin-skills/         # SKILL.md files extracted from circlefin/skills
    use-arc.md
    use-usdc.md
    use-circle-wallets.md
    use-developer-controlled-wallets.md
    use-user-controlled-wallets.md
    use-modular-wallets.md
    use-gateway.md
    use-smart-contract-platform.md
    bridge-stablecoin.md
samples/                    # full sample codebases as git submodules
  arc-commerce/             # github.com/circlefin/arc-commerce
  arc-multichain-wallet/    # github.com/circlefin/arc-multichain-wallet
  arc-escrow/               # github.com/circlefin/arc-escrow
  arc-fintech/              # github.com/circlefin/arc-fintech
  arc-p2p-payments/         # github.com/circlefin/arc-p2p-payments
  arc-nanopayments/         # github.com/circlefin/arc-nanopayments
  arc-prediction-markets/   # github.com/circlefin/arc-prediction-markets
  arc-stablecoin-fx/        # github.com/circlefin/arc-stablecoin-fx
```

## Where to start

For most "build something on Arc" questions, the `circlefin-skills/`
files are the LLM-optimized entry points — they cover architecture
decisions, correct flows, and common pitfalls. Read the relevant
skill before diving into the per-page docs.

| Task                                            | Start with                                |
| ----------------------------------------------- | ----------------------------------------- |
| Anything Arc-specific (chain config, deploy)    | `circlefin-skills/use-arc.md`             |
| USDC transfers / balances / approvals           | `circlefin-skills/use-usdc.md`            |
| Crosschain USDC (CCTP, Bridge Kit)              | `circlefin-skills/bridge-stablecoin.md`   |
| Choosing a wallet type                          | `circlefin-skills/use-circle-wallets.md`  |
| Custodial / dev-controlled wallets              | `circlefin-skills/use-developer-controlled-wallets.md` |
| Embedded / user-controlled wallets              | `circlefin-skills/use-user-controlled-wallets.md` |
| Smart-contract wallets / 4337 / passkeys        | `circlefin-skills/use-modular-wallets.md` |
| Unified balance / nanopayments                  | `circlefin-skills/use-gateway.md`         |
| Contract templates, deploy, monitor             | `circlefin-skills/use-smart-contract-platform.md` |
| Building an AI agent that holds & spends USDC itself | `docs/developers.circle.com/agent-stack.md` (Circle CLI + Agent Wallets) |
| Onchain agent identity / job settlement (ERC-8004/8183) | `docs/docs.arc.network/build/agentic-economy.md` |
| Reference live SDK signatures / contract addrs  | docs/developers.circle.com/api-reference.md and openapi/*.yaml |

The Arc and Circle sides of "agents" are complements, not duplicates:
**Agent Stack** (`developers.circle.com/agent-stack.md`) is the *tooling* an
agent runs — Circle CLI (`@circle-fin/cli`), Agent Wallets, Gateway
nanopayments, the x402 service marketplace, and Circle Skills. The Arc
**Agentic Economy** docs cover the *onchain standards* agents settle with —
ERC-8004 identity/reputation and ERC-8183 job contracts — plus the `arc-escrow`
sample.

## Using arc-canteen against this context

The `arc-canteen` CLI uses the same Postgres session token to talk to:

- `arc-cli-server.thecanteenapp.com` for event logging (telemetry from
  `arc-canteen login`, `update-traction`, `update-product`, `events`)
- `rpc.testnet.arc-node.thecanteenapp.com` for JSON-RPC against the
  Arc testnet (`arc-canteen rpc <method> [params]`)

A typical interaction:

```bash
arc-canteen login                                # GitHub device flow → swrm_ token
arc-canteen rpc eth_chainId                      # → 0x4cef52
arc-canteen rpc eth_getBalance '["0xabc...", "latest"]'
```

The proxy enforces a method allowlist (read-mostly Eth RPC plus
`eth_sendRawTransaction`). The full list lives in the proxy code; if
you hit `method '<x>' not allowed by the proxy`, that method isn't
exposed by design.

## Refreshing the context

Run `arc-canteen context sync` to git-pull this repo (with submodules)
into `~/.arc-canteen/context/`. The upstream is
`https://github.com/the-canteen-dev/context-arc`.

## License and provenance

- `docs/docs.arc.network/`, `docs/developers.circle.com/`, and
  `docs/circlefin-skills/` are mirrors of upstream Circle content.
  Source URLs follow the mirrored path (e.g. `arc/concepts/foo.md` →
  `https://docs.arc.network/arc/concepts/foo.md`).
- `samples/` are git submodules pointing at the original `circlefin/*`
  repositories. Their licenses apply to each sample.


# Files available in /home/kyle/.arc-canteen/context

AGENTS.md
README.md
docs/circlefin-skills/bridge-stablecoin.md
docs/circlefin-skills/use-arc.md
docs/circlefin-skills/use-circle-wallets.md
docs/circlefin-skills/use-developer-controlled-wallets.md
docs/circlefin-skills/use-gateway.md
docs/circlefin-skills/use-modular-wallets.md
docs/circlefin-skills/use-smart-contract-platform.md
docs/circlefin-skills/use-usdc.md
docs/circlefin-skills/use-user-controlled-wallets.md
docs/developers.circle.com/agent-stack/agent-wallets/fees.md
docs/developers.circle.com/agent-stack/agent-wallets/quickstart.md
docs/developers.circle.com/agent-stack/agent-wallets/supported-blockchains.md
docs/developers.circle.com/agent-stack/agent-wallets/wallet-operations/authenticate.md
docs/developers.circle.com/agent-stack/agent-wallets/wallet-operations/bridge.md
docs/developers.circle.com/agent-stack/agent-wallets/wallet-operations/custom-policies.md
docs/developers.circle.com/agent-stack/agent-wallets/wallet-operations/execute-contract.md
docs/developers.circle.com/agent-stack/agent-wallets/wallet-operations/fund.md
docs/developers.circle.com/agent-stack/agent-wallets/wallet-operations/nanopay.md
docs/developers.circle.com/agent-stack/agent-wallets/wallet-operations/pay-for-service.md
docs/developers.circle.com/agent-stack/agent-wallets/wallet-operations/sign.md
docs/developers.circle.com/agent-stack/agent-wallets/wallet-operations/swap.md
docs/developers.circle.com/agent-stack/agent-wallets/wallet-operations/transfer.md
docs/developers.circle.com/agent-stack/agent-wallets/wallet-operations.md
docs/developers.circle.com/agent-stack/agent-wallets.md
docs/developers.circle.com/agent-stack/circle-cli/command-reference.md
docs/developers.circle.com/agent-stack/circle-cli.md
docs/developers.circle.com/agent-stack.md
docs/developers.circle.com/ai/mcp.md
docs/developers.circle.com/ai/skills.md
docs/developers.circle.com/api-reference/keys.md
docs/developers.circle.com/api-reference.md
docs/developers.circle.com/bridge-kit.md
docs/developers.circle.com/cctp/concepts/fast-transfer-allowance.md
docs/developers.circle.com/cctp/concepts/fees.md
docs/developers.circle.com/cctp/concepts/supported-chains-and-domains.md
docs/developers.circle.com/cctp/migration-from-v1-to-v2.md
docs/developers.circle.com/cctp/quickstarts/transfer-usdc-ethereum-to-arc.md
docs/developers.circle.com/cctp/quickstarts/transfer-usdc-solana-to-arc.md
docs/developers.circle.com/cctp/references/contract-addresses.md
docs/developers.circle.com/cctp/references/technical-guide.md
docs/developers.circle.com/cctp.md
docs/developers.circle.com/circle-mint/crypto-payments-quickstart.md
docs/developers.circle.com/circle-mint/getting-started-with-the-circle-apis.md
docs/developers.circle.com/circle-mint/introducing-circle-mint.md
docs/developers.circle.com/circle-mint/quickstart-withdraw-to-bank.md
docs/developers.circle.com/circle-mint/supported-chains-and-currencies.md
docs/developers.circle.com/contracts/deploy-smart-contract-template.md
docs/developers.circle.com/contracts/scp-deploy-smart-contract.md
docs/developers.circle.com/contracts/scp-event-monitoring.md
docs/developers.circle.com/contracts/scp-interact-smart-contract.md
docs/developers.circle.com/contracts/scp-templates-overview.md
docs/developers.circle.com/contracts/supported-blockchains.md
docs/developers.circle.com/contracts.md
docs/developers.circle.com/cpn/concepts/payments/payments.md
docs/developers.circle.com/cpn/concepts/quotes.md
docs/developers.circle.com/cpn/quickstarts/integrate-with-cpn-ofi.md
docs/developers.circle.com/cpn/references/blockchains/supported-blockchains.md
docs/developers.circle.com/cpn/references/compliance/supported-countries.md
docs/developers.circle.com/cpn.md
docs/developers.circle.com/gateway/nanopayments/concepts/x402.md
docs/developers.circle.com/gateway/nanopayments/quickstarts/buyer.md
docs/developers.circle.com/gateway/nanopayments/quickstarts/seller.md
docs/developers.circle.com/gateway/nanopayments.md
docs/developers.circle.com/gateway/quickstarts/unified-balance-evm.md
docs/developers.circle.com/gateway/quickstarts/unified-balance-solana.md
docs/developers.circle.com/gateway/references/fees.md
docs/developers.circle.com/gateway/references/supported-blockchains.md
docs/developers.circle.com/gateway.md
docs/developers.circle.com/llms.txt
docs/developers.circle.com/openapi/cctp.yaml
docs/developers.circle.com/openapi/compliance.yaml
docs/developers.circle.com/openapi/cpn-ofi.yaml
docs/developers.circle.com/openapi/developer-controlled-wallets.yaml
docs/developers.circle.com/openapi/gateway.yaml
docs/developers.circle.com/openapi/smart-contract-platform.yaml
docs/developers.circle.com/openapi/stablefx.yaml
docs/developers.circle.com/openapi/user-controlled-wallets.yaml
docs/developers.circle.com/openapi/xreserve.yaml
docs/developers.circle.com/paymaster/addresses-and-events.md
docs/developers.circle.com/paymaster/pay-gas-fees-usdc.md
docs/developers.circle.com/paymaster.md
docs/developers.circle.com/sample-projects.md
docs/developers.circle.com/sdks.md
docs/developers.circle.com/stablecoins/eurc-contract-addresses.md
docs/developers.circle.com/stablecoins/quickstart-transfer-10-usdc-on-solana.md
docs/developers.circle.com/stablecoins/quickstarts/transfer-eurc-evm.md
docs/developers.circle.com/stablecoins/quickstarts/transfer-usdc-evm.md
docs/developers.circle.com/stablecoins/usdc-contract-addresses.md
docs/developers.circle.com/stablecoins/what-is-eurc.md
docs/developers.circle.com/stablecoins/what-is-usdc.md
docs/developers.circle.com/stablefx/quickstarts/fx-trade-maker.md
docs/developers.circle.com/stablefx/quickstarts/fx-trade-taker.md
docs/developers.circle.com/stablefx/references/supported-currencies.md
docs/developers.circle.com/stablefx.md
docs/developers.circle.com/wallets/account-types.md
docs/developers.circle.com/wallets/create-api-key.md
docs/developers.circle.com/wallets/dev-controlled/create-your-first-wallet.md
docs/developers.circle.com/wallets/dev-controlled.md
docs/developers.circle.com/wallets/gas-station/send-a-gasless-transaction.md
docs/developers.circle.com/wallets/gas-station.md
docs/developers.circle.com/wallets/infrastructure-models.md
docs/developers.circle.com/wallets/modular/create-a-wallet-and-send-gasless-txn.md
docs/developers.circle.com/wallets/modular.md
docs/developers.circle.com/wallets/supported-blockchains.md
docs/developers.circle.com/wallets/user-controlled.md
docs/developers.circle.com/wallets.md
docs/developers.circle.com/xreserve/concepts/how-xreserve-works.md
docs/developers.circle.com/xreserve/references/supported-blockchains-and-domains.md
docs/developers.circle.com/xreserve/tutorials/deposit-usdc-into-xreserve.md
docs/developers.circle.com/xreserve.md
docs/docs.arc.network/ai/mcp.md
docs/docs.arc.network/app-kit/bridge.md
docs/docs.arc.network/app-kit/concepts/bridge-fees.md
docs/docs.arc.network/app-kit/concepts/swap-fees.md
docs/docs.arc.network/app-kit/concepts/unified-balance-fees.md
docs/docs.arc.network/app-kit/quickstarts/bridge-tokens-across-blockchains.md
docs/docs.arc.network/app-kit/quickstarts/send-tokens-same-chain.md
docs/docs.arc.network/app-kit/quickstarts/swap-tokens-crosschain.md
docs/docs.arc.network/app-kit/quickstarts/swap-tokens-same-chain.md
docs/docs.arc.network/app-kit/quickstarts/unified-balance-deposit-and-spend.md
docs/docs.arc.network/app-kit/references/bridge-error-recovery.md
docs/docs.arc.network/app-kit/references/sdk-reference.md
docs/docs.arc.network/app-kit/references/supported-blockchains.md
docs/docs.arc.network/app-kit/send.md
docs/docs.arc.network/app-kit/swap.md
docs/docs.arc.network/app-kit/tutorials/adapter-setups.md
docs/docs.arc.network/app-kit/tutorials/installation.md
docs/docs.arc.network/app-kit/unified-balance.md
docs/docs.arc.network/app-kit.md
docs/docs.arc.network/arc/concepts/deterministic-finality.md
docs/docs.arc.network/arc/concepts/opt-in-privacy.md
docs/docs.arc.network/arc/concepts/post-quantum-security.md
docs/docs.arc.network/arc/concepts/running-a-node.md
docs/docs.arc.network/arc/concepts/stable-fee-design.md
docs/docs.arc.network/arc/concepts/system-overview.md
docs/docs.arc.network/arc/references/connect-to-arc.md
docs/docs.arc.network/arc/references/contract-addresses.md
docs/docs.arc.network/arc/references/evm-compatibility.md
docs/docs.arc.network/arc/references/gas-and-fees.md
docs/docs.arc.network/arc/references/sample-applications.md
docs/docs.arc.network/arc/tools/account-abstraction.md
docs/docs.arc.network/arc/tools/compliance-vendors.md
docs/docs.arc.network/arc/tools/data-indexers.md
docs/docs.arc.network/arc/tools/node-providers.md
docs/docs.arc.network/arc/tools/oracles.md
docs/docs.arc.network/arc/tutorials/create-your-first-erc-8183-job.md
docs/docs.arc.network/arc/tutorials/deploy-contracts.md
docs/docs.arc.network/arc/tutorials/deploy-on-arc.md
docs/docs.arc.network/arc/tutorials/interact-with-contracts.md
docs/docs.arc.network/arc/tutorials/monitor-contract-events.md
docs/docs.arc.network/arc/tutorials/register-your-first-ai-agent.md
docs/docs.arc.network/arc/tutorials/run-an-arc-node.md
docs/docs.arc.network/arc-chain.md
docs/docs.arc.network/build/agentic-economy.md
docs/docs.arc.network/build.md
docs/docs.arc.network/integrate/connect-to-arc.md
docs/docs.arc.network/integrate/deploy-on-arc.md
docs/docs.arc.network/integrate.md
docs/docs.arc.network/llms.txt
samples/arc-commerce/
samples/arc-escrow/
samples/arc-fintech/
samples/arc-multichain-wallet/
samples/arc-nanopayments/
samples/arc-p2p-payments/
samples/arc-prediction-markets/
samples/arc-stablecoin-fx/
