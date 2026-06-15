use k256::ecdsa::SigningKey;
use tiny_keccak::{Hasher, Keccak};

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut h = Keccak::v256();
    h.update(data);
    h.finalize(&mut out);
    out
}

fn main() {
    let signing_key = SigningKey::random(&mut rand_core::OsRng);
    let private_key = signing_key.to_bytes();

    let verifying_key = signing_key.verifying_key();
    let point = verifying_key.to_encoded_point(false);
    let public_key_bytes = &point.as_bytes()[1..]; // strip 0x04 prefix

    let hash = keccak256(public_key_bytes);
    let address_bytes = &hash[12..];
    let address = format!("0x{}", hex::encode(address_bytes));
    let private_key_hex = format!("0x{}", hex::encode(private_key));

    println!("─────────────────────────────────────────────────────────");
    println!("  Arc x402 — New EOA Keypair");
    println!("─────────────────────────────────────────────────────────");
    println!("  Address:     {address}");
    println!("  Private Key: {private_key_hex}");
    println!("─────────────────────────────────────────────────────────");
    println!();
    println!("  ⚠  KEEP YOUR PRIVATE KEY SECRET. Never commit it to git.");
    println!();
    println!("  Next steps:");
    println!("  1. Fund with USDC: https://faucet.circle.com (select Arc Testnet)");
    println!("  2. Add to .env.local:");
    println!("       SELLER_ADDRESS={address}");
    println!("       SELLER_PRIVATE_KEY={private_key_hex}");
}
