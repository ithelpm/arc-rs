use alloy::{
    network::EthereumWallet,
    primitives::{keccak256, Address, B256, TxHash},
    providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
    sol,
};
use anyhow::Context;

sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    interface IMediaAccess {
        event AccessGranted(address indexed wallet, bytes32 indexed contentId);
        function grantAccess(address wallet, bytes32 contentId) external;
        function hasAccess(address wallet, bytes32 contentId) external view returns (bool);
    }
}

/// Converts a string item ID (e.g. a Jellyfin UUID) to a bytes32 content ID
/// using keccak256 for deterministic, collision-resistant mapping.
pub fn item_id_to_content_id(item_id: &str) -> B256 {
    keccak256(item_id.as_bytes())
}

/// Client for interacting with a deployed MediaAccess contract.
///
/// Read-only operations (`has_access`) work without a signer.
/// Write operations (`grant_access`) require `.with_signer()`.
pub struct MediaAccessClient {
    rpc_url: String,
    private_key: Option<String>,
    pub contract_address: Address,
}

impl MediaAccessClient {
    /// Creates a read-only client.
    pub fn new(rpc_url: impl Into<String>, contract_address: Address) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            private_key: None,
            contract_address,
        }
    }

    /// Attaches a private key for write operations.
    pub fn with_signer(mut self, private_key: impl Into<String>) -> Self {
        self.private_key = Some(private_key.into());
        self
    }

    /// Checks on-chain whether `wallet` has access to `content_id`.
    /// Read-only — no gas, no signing.
    pub async fn has_access(&self, wallet: Address, content_id: B256) -> anyhow::Result<bool> {
        let url: url::Url = self.rpc_url.parse().context("invalid RPC URL")?;
        let provider = ProviderBuilder::new().connect_http(url);
        let contract = IMediaAccess::new(self.contract_address, provider);
        // alloy 2.x: single-return call() resolves to the return value directly
        let has = contract.hasAccess(wallet, content_id).call().await?;
        Ok(has)
    }

    /// Grants soulbound access on-chain. Requires a signer.
    /// Returns the settled transaction hash.
    pub async fn grant_access(&self, wallet: Address, content_id: B256) -> anyhow::Result<TxHash> {
        let key = self
            .private_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("private key required for grantAccess; use .with_signer()"))?;

        let signer: PrivateKeySigner = key.parse()?;
        let eth_wallet = EthereumWallet::from(signer);
        let url: url::Url = self.rpc_url.parse().context("invalid RPC URL")?;

        // alloy 2.x: recommended fillers (gas, nonce, chain ID) are enabled by default
        let provider = ProviderBuilder::new()
            .wallet(eth_wallet)
            .connect_http(url);

        let contract = IMediaAccess::new(self.contract_address, provider);
        let pending = contract.grantAccess(wallet, content_id).send().await?;
        let receipt = pending.get_receipt().await?;
        Ok(receipt.transaction_hash)
    }
}
