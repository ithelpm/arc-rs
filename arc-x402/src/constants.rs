pub const ARC_TESTNET_CHAIN_ID: u64 = 5042002;
pub const ARC_TESTNET_CAIP2: &str = "eip155:5042002";
pub const ARC_TESTNET_USDC: &str = "0x3600000000000000000000000000000000000000";
pub const ARC_TESTNET_GATEWAY_WALLET: &str = "0x0077777d7EBA4688BDeF3E311b846F25870A19B9";
pub const ARC_TESTNET_DOMAIN_ID: u32 = 26;
pub const GATEWAY_API_TESTNET: &str = "https://gateway-api-testnet.circle.com";
pub const ARC_TESTNET_RPC: &str = "https://rpc.testnet.arc.network";

/// validBefore must be at least 7 days from now (Circle Gateway requirement).
/// We add a 10-minute buffer to avoid edge cases.
pub const MIN_VALID_DURATION_SECS: u64 = 7 * 24 * 3600 + 600;

/// Maximum accepted payment price in USDC atomic units ($1000 = 1_000_000_000 units).
/// Prevents accidental overpayment from misconfigured price strings.
pub const MAX_PRICE_ATOMIC: u64 = 1_000_000_000;

pub const X402_VERSION: u32 = 2;
pub const DEFAULT_MAX_TIMEOUT_SECONDS: u64 = 604900;
pub const GATEWAY_DOMAIN_NAME: &str = "GatewayWalletBatched";
pub const GATEWAY_DOMAIN_VERSION: &str = "1";
