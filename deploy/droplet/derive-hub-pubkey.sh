#!/bin/bash
set -euo pipefail
source /root/.cargo/env
mkdir -p /tmp/derive-pk
cd /tmp/derive-pk
cat > Cargo.toml <<'EOF'
[package]
name = "derive_pk"
version = "0.1.0"
edition = "2021"
[dependencies]
secp256k1 = { version = "0.29", features = ["std"] }
hex = "0.4"
EOF
mkdir -p src
cat > src/main.rs <<'EOF'
fn main() {
    let secret = hex::decode(std::env::args().nth(1).expect("hex sk")).unwrap();
    let sk = secp256k1::SecretKey::from_slice(&secret).unwrap();
    let secp = secp256k1::Secp256k1::new();
    let pk = secp256k1::PublicKey::from_secret_key(&secp, &sk);
    println!("{}", hex::encode(pk.serialize()));
}
EOF
cargo build --release -q
/tmp/derive-pk/target/release/derive_pk "$(grep '^FSP_AGENT_SECRET_KEY=' /etc/fspdevs/treasury.env | cut -d= -f2)"
