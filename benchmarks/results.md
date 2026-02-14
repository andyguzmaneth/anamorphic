# StealthDeFi Benchmark Results

Generated: 2026-02-14 02:47:00 UTC

## Gas Costs

| Operation | Gas Cost |
|-----------|----------|
| Stealth Transfer | 29,050 gas |
| Token Approve | 46,711 gas |
| Token Swap | 91,532 gas |
| Token Transfer | 47,665 gas |
| Post Commitment | 140,486 gas |
| Verify & Release | 287,885 gas |
| **Total (Stealth Protocol)** | **643,329 gas** |
| **Baseline (Direct Swap)** | **79,232 gas** |
| **Overhead** | **+711.96%** |

## Zero-Knowledge Proof Metrics

| Metric | Value |
|--------|-------|
| Circuit Constraints | 1,376 |
| Proof Generation Time (avg) | 1,970 ms (~2.0s) |
| Proof Generation Time (stddev) | 250 ms |
| Proof Size | 803 bytes (0.78 KB) |

## Analysis

### Gas Efficiency

The stealth protocol adds **711.96%** gas overhead compared to a direct AMM swap. This overhead is due to:

1. **ZK Proof Verification** (287,885 gas, 44.8%): bn128 pairing precompile
2. **Commitment Posting** (140,486 gas, 21.8%): storage writes for bond
3. **Token Operations** (185,908 gas, 28.9%): approve + swap + transfer
4. **Stealth Transfer** (29,050 gas, 4.5%): calldata cost

### Proof Generation Performance

Groth16 proof generation averages **1,970 ms** (~2 seconds) with a standard deviation of 250 ms over multiple runs. The circuit has **1,376 constraints**, which is relatively small for a ZK circuit. The proof itself is compact at **0.78 KB**, making it efficient for on-chain verification.

### Comparison with Existing Solutions

| Solution | Gas Overhead | Proof Time | Privacy Properties |
|----------|--------------|------------|-----------|
| **StealthDeFi** | **+712%** | **~2,000 ms** | Sender/recipient/instruction privacy |
| Tornado Cash | +150-200% | ~10,000 ms | Amount-based anonymity sets |
| Aztec Connect | +300-400% | ~15,000 ms | Full privacy, requires rollup |

**Note:** The above comparison is approximate. StealthDeFi achieves significantly faster proof generation (~5x faster) than Tornado Cash due to the use of Groth16 with a small circuit (1,376 constraints vs 20,000+ for Tornado Cash). However, StealthDeFi has higher gas overhead because it proves execution correctness, not just withdrawal validity.

## Key Findings

1. **Privacy-Performance Trade-off:** 712% gas overhead for strong privacy guarantees (sender anonymity, recipient privacy, instruction privacy)
2. **Fast Proof Generation:** ~2 seconds enables near-realtime execution, much faster than existing privacy protocols
3. **Compact Proofs:** 0.78 KB proofs minimize on-chain verification cost
4. **Efficient Circuit:** 1,376 constraints balance security and performance
5. **Production Viability:** While gas costs are high, they are acceptable for high-value privacy-preserving DeFi use cases where privacy is paramount

## Trade-offs

- **Higher Gas Cost:** The protocol prioritizes privacy over gas efficiency. Users pay ~8x more gas for privacy
- **Instant Execution:** Unlike Tornado Cash which requires time-delayed withdrawals for anonymity, StealthDeFi enables instant private execution
- **No Anonymity Set:** Stealth addresses provide cryptographic privacy without requiring large anonymity sets
- **Relayer Dependency:** The protocol requires a trusted relayer, whereas some alternatives are fully non-custodial
