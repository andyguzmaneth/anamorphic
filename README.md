# Anamorphic

**Privacy-preserving DeFi execution on public blockchains.**

DeFi transactions that look like normal ETH transfers. An anamorphic image appears as one thing to most observers but reveals its true form to those who know where to look. Anamorphic does the same for on-chain transactions: users execute swaps, transfers, and other DeFi operations through transactions that are indistinguishable from ordinary ETH transfers to outside observers.

Built with stealth addresses (ERC-5564), encrypted instructions (AES-256-GCM), bonded relayers, and zero-knowledge proofs (Groth16) of correct execution.

## How It Works

```
User                          On-Chain                        Relayer
 |                               |                               |
 |  1. Derive stealth address    |                               |
 |     via ECDH shared secret    |                               |
 |                               |                               |
 |  2. Encrypt swap instruction  |                               |
 |     (AES-256-GCM)             |                               |
 |                               |                               |
 |  3. Send ETH to stealth addr  |                               |
 |     with ephemeral pubkey     |                               |
 |     + encrypted payload       |                               |
 |  ─────────────────────────>   |                               |
 |                               |   4. Detect stealth transfer  |
 |                               |      via view key + ECDH      |
 |                               |   <───────────────────────────|
 |                               |                               |
 |                               |   5. Decrypt instruction      |
 |                               |   6. Execute swap on AMM      |
 |                               |   7. Send tokens to recipient |
 |                               |   <───────────────────────────|
 |                               |                               |
 |                               |   8. Post bond + commitment   |
 |                               |   9. Generate Groth16 proof   |
 |                               |  10. Submit proof → bond back |
 |                               |   <───────────────────────────|
```

An observer sees: ETH sent to a random address with some calldata. They cannot determine the sender's identity, the recipient, the swap parameters, or that a DeFi operation occurred at all.

## Architecture

```
anamorphic/
├── contracts/          Solidity — StealthEscrow, Groth16Verifier, MockAMM
├── relayer/            Rust — stealth-crypto lib, relayer binary, E2E test
├── circuits/           Circom — instruction commitment + execution proof
├── scripts/            Build, setup, prove, benchmark
├── analysis/           Privacy analysis reports
└── benchmarks/         Gas costs, proof times, overhead metrics
```

## Performance

| Metric | Value |
|--------|-------|
| Total gas (full protocol) | 643,329 |
| Baseline direct swap | 79,232 |
| Overhead | +712% |
| Proof generation | ~2s |
| Proof size | 803 bytes |
| Circuit constraints | 1,376 |

### Gas Breakdown

| Operation | Gas | Share |
|-----------|-----|-------|
| ZK proof verification | 287,885 | 44.8% |
| Token ops (approve + swap + transfer) | 185,908 | 28.9% |
| Commitment + bond posting | 140,486 | 21.8% |
| Stealth transfer | 29,050 | 4.5% |

## Privacy Properties

| Property | Strength | Notes |
|----------|----------|-------|
| Sender anonymity | Strong | No on-chain link from user to stealth address |
| Recipient privacy | Strong | Hidden until swap execution |
| Instruction privacy | Strong | Encrypted, only relayer can decrypt |
| Amount privacy | Partial | ETH amount visible; token amounts hidden until swap |
| Timing privacy | Weak | Immediate execution creates timing correlation |
| Transaction type | Weak | Distinguishable by calldata, but blends with general contract interactions |

## Quick Start

### Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation) (forge, anvil)
- [Rust](https://rustup.rs/) (1.75+)
- [Circom](https://docs.circom.io/getting-started/installation/) (0.5+)
- [Node.js](https://nodejs.org/) (18+)

### Build

```bash
npm install                           # snarkjs + circomlib
cd contracts && forge build && cd ..  # Solidity
cd relayer && cargo build && cd ..    # Rust
```

### Test

```bash
# Solidity (31 tests)
cd contracts && forge test

# Rust (17 tests)
cd relayer && cargo test

# Circom witness generation
./scripts/test_witness.sh
./scripts/test_execution_proof.sh
```

### Run End-to-End

```bash
# Runs full protocol on local Anvil: deploy, stealth transfer,
# decrypt, swap, prove, verify — all automated
cargo run --bin e2e-test --manifest-path relayer/Cargo.toml
```

### Privacy Analysis & Benchmarks

```bash
# Generate privacy report → analysis/privacy-report.md
cargo run --bin privacy-analyzer --manifest-path relayer/Cargo.toml

# Run benchmarks → benchmarks/results.csv + benchmarks/results.md
./scripts/quick_benchmarks.sh
```

## Cryptography

| Component | Scheme | Details |
|-----------|--------|---------|
| Stealth addresses | ECDH on secp256k1 | ERC-5564 compatible, one-time unlinkable addresses |
| Key derivation | HKDF-SHA256 | Encryption key derived from ECDH shared secret |
| Instruction encryption | AES-256-GCM | 12-byte random nonce, authenticated encryption |
| ZK commitment | Poseidon hash | Circuit-friendly, 8-input hash of instruction fields |
| On-chain commitment | keccak256 | Standard Ethereum hash for contract-side matching |
| Proof system | Groth16 on BN128 | ~800 byte proofs, ~288k gas verification |

## Stack

- **Rust**: stealth address derivation (k256), instruction encryption (aes-gcm), relayer (ethers-rs)
- **Solidity**: escrow contract, Groth16 verifier, mock AMM (Foundry)
- **Circom**: instruction commitment proof, execution correctness proof (snarkjs)

## Limitations

This is a research prototype for an MSc dissertation. Not production-ready.

- Single relayer (no multi-hop routing)
- Local Anvil only (no testnet/mainnet)
- No relayer reputation system
- No MEV protection beyond the protocol itself
- Poseidon/keccak hash mismatch between circuits and contracts (bridged, not unified)
- Timing correlation is the primary privacy leak

## License

MIT
