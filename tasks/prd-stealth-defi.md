# PRD: StealthDeFi — Privacy-Preserving DeFi Execution Prototype

## Introduction

Exploratory prototype for an MSc dissertation on privacy-preserving DeFi execution on public blockchains. The core idea: users execute DeFi operations (swaps, transfers) via transactions that are **indistinguishable from normal ETH transfers** on-chain. Uses stealth addresses, encrypted instructions, bonded relayers, and ZK proofs of correct execution.

This prototype implements **Approach A (Optimistic Execution)** — a single-relayer model where the relayer executes immediately and posts a bond + ZKP proof. Approach B (multi-hop onion routing) is out of scope for v1.

## Goals

- Prove feasibility of hiding DeFi intent inside normal-looking ETH transfers
- Implement stealth address derivation (ERC-5564 style) and instruction encryption (AES-256-GCM)
- Build Solidity contracts for commitment/bond mechanism with challenge period
- Build Circom/Groth16 circuits proving correct execution without revealing instruction content
- Build a Rust relayer that detects stealth transfers, decrypts, executes, and proves
- Run full end-to-end flow on local Anvil and measure gas costs, proving times, and privacy properties

## User Stories

### US-001: Project scaffold — Foundry + Rust + Circom
**Description:** As a developer, I need a monorepo with Foundry (Solidity), Rust (cargo workspace), and Circom directories so all components can be built from one place.

**Acceptance Criteria:**
- [ ] `contracts/` directory with Foundry project (`forge init` style — foundry.toml, src/, test/, script/)
- [ ] `relayer/` directory with Rust cargo workspace containing two crates: `stealth-crypto` (library) and `relayer` (binary)
- [ ] `circuits/` directory with a Makefile or shell script that compiles a trivial test circuit with circom
- [ ] `package.json` at root with snarkjs as devDependency (for witness generation and proof)
- [ ] `forge build` passes with zero errors
- [ ] `cargo build` passes with zero errors in the workspace
- [ ] Circom test circuit compiles to R1CS + WASM

### US-002: Stealth address library (Rust)
**Description:** As a developer, I need a Rust library that generates stealth addresses using ECDH on secp256k1, so users can send funds to unlinkable one-time addresses that only the relayer can spend.

**Acceptance Criteria:**
- [ ] `stealth-crypto` crate with `StealthAddress` module
- [ ] Key generation: relayer has spend keypair + view keypair (meta-address)
- [ ] User generates ephemeral keypair, computes ECDH shared secret with relayer's view pubkey
- [ ] Stealth address derived: `hash(shared_secret || "spend") * G + relayer_spend_pubkey`
- [ ] Relayer can compute stealth private key from shared secret + spend privkey
- [ ] Ephemeral pubkey included in transaction for relayer to detect
- [ ] 5+ unit tests: key generation, shared secret agreement, stealth derivation, private key recovery, deterministic output for fixed inputs
- [ ] `cargo test` passes

### US-003: Instruction encryption (Rust)
**Description:** As a developer, I need to encrypt DeFi instructions with the ECDH shared secret so only the intended relayer can read them.

**Acceptance Criteria:**
- [ ] `Instruction` struct in `stealth-crypto`: action_type (u8), token_in (address), token_out (address), amount_in (u256), min_amount_out (u256), recipient (address), deadline (u64), nonce (u64)
- [ ] Serialize instruction to bytes (deterministic, big-endian)
- [ ] Encrypt with AES-256-GCM using key derived from shared secret (HKDF-SHA256)
- [ ] Decrypt function recovers original instruction
- [ ] Commitment: `keccak256(serialized_instruction)` computed before encryption
- [ ] 5+ unit tests: round-trip encrypt/decrypt, commitment matches, wrong key fails, tampered ciphertext fails, different nonces produce different ciphertexts
- [ ] `cargo test` passes

### US-004: Commitment and bond contract (Solidity)
**Description:** As a relayer, I need a smart contract where I post a bond alongside my execution commitment, so users have economic guarantees of correct execution.

**Acceptance Criteria:**
- [ ] `StealthEscrow.sol` contract with:
  - `postCommitment(bytes32 commitment, bytes32 stealthAddrHash)` — relayer sends ETH bond with this call
  - `Commitment` struct: commitment hash, bond amount, relayer address, challenge deadline (block.timestamp + 7 days), zkpVerified (bool), released (bool)
  - `challenge(uint256 commitmentId, bytes calldata proof)` — anyone can challenge with evidence of misbehavior
  - `releaseBond(uint256 commitmentId)` — relayer claims bond after challenge period (or after ZKP verification)
  - `recoverFunds(uint256 commitmentId, bytes32 ephemeralPubKey, bytes calldata proof)` — user recovers if relayer disappears (14-day timeout)
- [ ] Bond must be >= 1 ETH (configurable minimum)
- [ ] Events emitted: CommitmentPosted, BondReleased, ChallengeSubmitted, FundsRecovered
- [ ] Foundry tests: post commitment, release after period, challenge flow, recovery flow, reject early release
- [ ] `forge test` passes

### US-005: Mock AMM contract (Solidity)
**Description:** As a developer, I need a simple AMM pool for testing swaps locally, since we can't fork mainnet Uniswap for a prototype.

**Acceptance Criteria:**
- [ ] `MockERC20.sol` — minimal ERC20 with public `mint(address, uint256)`
- [ ] `MockAMM.sol` — constant-product AMM (x * y = k)
  - `addLiquidity(uint256 amountA, uint256 amountB)` — deposits both tokens
  - `swap(address tokenIn, uint256 amountIn, uint256 minAmountOut)` — swaps with slippage protection
  - Two token addresses set at construction
- [ ] Deploy script creates TokenA, TokenB, and AMM with initial liquidity
- [ ] Foundry tests: add liquidity, swap A→B, swap B→A, slippage revert, price impact
- [ ] `forge test` passes

### US-006: Circom circuit — instruction commitment proof
**Description:** As a developer, I need a ZK circuit that proves an instruction matches a public commitment hash without revealing the instruction content.

**Acceptance Criteria:**
- [ ] `circuits/instruction_commitment.circom` circuit
- [ ] Private inputs: action_type, token_in, token_out, amount_in, min_amount_out, recipient, deadline, nonce
- [ ] Public inputs: commitment_hash (Poseidon hash of all private inputs)
- [ ] Circuit constraint: Poseidon(private_inputs) === commitment_hash
- [ ] Uses circomlib's Poseidon hasher
- [ ] Circuit compiles, R1CS generated
- [ ] Witness generation test with known inputs passes (snarkjs)
- [ ] Constraint count logged

### US-007: Circom circuit — execution correctness proof
**Description:** As a developer, I need a ZK circuit that proves the relayer executed the instruction correctly (right recipient, sufficient output amount, within deadline) without revealing instruction details.

**Acceptance Criteria:**
- [ ] `circuits/execution_proof.circom` — extends instruction commitment circuit
- [ ] Additional private inputs: shared_secret, stealth_private_key, execution_amount_out, execution_timestamp
- [ ] Additional public inputs: stealth_address, final_recipient, amount_out, execution_tx_hash
- [ ] Constraints:
  - Instruction commitment matches (reuses US-006 logic)
  - `final_recipient == instruction.recipient`
  - `amount_out >= instruction.min_amount_out`
  - `execution_timestamp <= instruction.deadline`
- [ ] Groth16 trusted setup: powers of tau ceremony (BN128, at least 2^16 constraints)
- [ ] Verification key + Solidity verifier contract auto-generated via snarkjs
- [ ] Proof generation test with valid inputs succeeds
- [ ] Proof verification test (snarkjs verify) passes

### US-008: On-chain ZKP verifier integration (Solidity)
**Description:** As a relayer, I want to submit a ZK proof to skip the 7-day challenge period and get my bond released immediately.

**Acceptance Criteria:**
- [ ] `Groth16Verifier.sol` — auto-generated from snarkjs export
- [ ] `StealthEscrow.sol` updated: `verifyAndRelease(uint256 commitmentId, uint256[2] a, uint256[2][2] b, uint256[2] c, uint256[] publicInputs)`
- [ ] If proof verifies → sets `zkpVerified = true` and releases bond immediately
- [ ] If proof fails → reverts (bond stays locked)
- [ ] Foundry test: submit valid proof → bond released, submit invalid proof → revert
- [ ] `forge test` passes

### US-009: Rust relayer — Anvil monitoring and stealth detection
**Description:** As a relayer, I need to watch the local chain for incoming stealth transfers, detect them using my view key, and decrypt the embedded instructions.

**Acceptance Criteria:**
- [ ] `relayer` binary connects to local Anvil node via JSON-RPC (ethers-rs or alloy)
- [ ] Monitors new blocks for ETH transfers to addresses derivable from relayer's meta-address
- [ ] For each candidate tx: extracts ephemeral pubkey from calldata, computes ECDH shared secret, derives expected stealth address, checks if it matches tx recipient
- [ ] On match: decrypts instruction from remaining calldata, logs decoded instruction
- [ ] Handles: no matching txs (skip), malformed data (log warning, skip), RPC errors (retry with backoff)
- [ ] Integration test: send a stealth transfer on Anvil, verify relayer detects and decrypts it
- [ ] `cargo test` passes

### US-010: Rust relayer — execute swap and post proof
**Description:** As a relayer, after detecting and decrypting a stealth instruction, I need to execute the swap on the mock AMM and post a ZK proof to the escrow contract.

**Acceptance Criteria:**
- [ ] Relayer uses stealth private key to claim ETH from stealth address
- [ ] Executes `swap()` on MockAMM with decoded instruction parameters
- [ ] Sends swapped tokens to `instruction.recipient`
- [ ] Computes instruction commitment hash
- [ ] Posts commitment + bond to StealthEscrow
- [ ] Generates Groth16 proof (shells out to snarkjs or uses ark-circom)
- [ ] Submits proof to StealthEscrow for immediate bond release
- [ ] Full execution logged with tx hashes for each step
- [ ] `cargo test` with Anvil passes

### US-011: End-to-end integration test
**Description:** As a developer, I need a single script that runs the complete flow on local Anvil to prove the system works end-to-end.

**Acceptance Criteria:**
- [ ] Shell script or Rust test binary that:
  1. Starts Anvil (or connects to running instance)
  2. Deploys: MockERC20 x2, MockAMM (with liquidity), StealthEscrow, Groth16Verifier
  3. Generates relayer keypairs (spend + view) and user ephemeral keypair
  4. User: derives stealth address, encrypts swap instruction, sends ETH + encrypted data to stealth address
  5. Relayer: detects transfer, decrypts instruction, executes swap on MockAMM
  6. Relayer: posts commitment + bond, generates ZKP, submits proof
  7. Asserts: bond released, recipient received swapped tokens, all events emitted
- [ ] Script prints summary: gas costs per step, total gas, proof generation time
- [ ] All assertions pass
- [ ] Exit code 0 on success

### US-012: Privacy analysis
**Description:** As a researcher, I need to analyze the on-chain footprint of the protocol to evaluate whether stealth transfers are distinguishable from normal ETH transfers.

**Acceptance Criteria:**
- [ ] Python or Rust script that reads Anvil traces from the integration test
- [ ] Checks:
  - **Linkability:** Can sender address be linked to final recipient from on-chain data alone?
  - **Distinguishability:** Does the stealth transfer's gas, calldata size, or value pattern differ from a normal ETH transfer?
  - **Relayer footprint:** Are the relayer's execution txs linkable to the original deposit?
- [ ] Compares gas cost: stealth swap vs. direct Uniswap-style swap
- [ ] Output: markdown report (`analysis/privacy-report.md`) with metrics table
- [ ] Script runs without errors

### US-013: Benchmark suite
**Description:** As a researcher, I need quantitative benchmarks for the dissertation's evaluation chapter.

**Acceptance Criteria:**
- [ ] Measures and reports:
  - Gas cost per operation: stealth transfer, commitment posting, proof verification, bond release, challenge, recovery
  - ZKP circuit size (constraint count)
  - Proof generation time (avg of 10 runs)
  - Proof size (bytes)
  - End-to-end latency (deposit → bond release)
  - Comparison: direct swap gas vs. stealth swap total gas (overhead %)
- [ ] Output: `benchmarks/results.csv` + `benchmarks/results.md` (formatted table)
- [ ] All measurements automated (no manual steps)
- [ ] Script runs without errors

## Functional Requirements

- FR-1: Stealth address derivation using ECDH on secp256k1 (ERC-5564 compatible)
- FR-2: AES-256-GCM instruction encryption with HKDF-derived key from shared secret
- FR-3: Instruction commitment via Poseidon hash (ZK-friendly) for circuit, keccak256 for on-chain
- FR-4: Solidity escrow contract with bond posting, challenge period, ZKP fast-release, and timeout recovery
- FR-5: Constant-product AMM for local swap testing
- FR-6: Circom circuits proving instruction commitment and execution correctness
- FR-7: Groth16 proof system with auto-generated Solidity verifier
- FR-8: Rust relayer monitoring Anvil for stealth transfers via JSON-RPC
- FR-9: Relayer executes decoded instructions and submits ZK proofs on-chain
- FR-10: End-to-end test script covering full protocol flow

## Non-Goals (Out of Scope for v1)

- **No multi-hop onion routing** (Approach B). Single relayer only.
- **No mainnet or testnet deployment.** Local Anvil only.
- **No relayer reputation system.** Single trusted relayer for prototype.
- **No cross-chain support.**
- **No frontend/UI.** CLI and scripts only.
- **No production key management.** Hardcoded test keys are fine.
- **No MEV protection beyond the protocol itself.** No Flashbots integration.
- **No formal verification of contracts.** Foundry tests are sufficient for a prototype.

## Technical Considerations

### Architecture

```
stealth-defi/
├── contracts/          # Foundry — Solidity contracts
│   ├── src/            # StealthEscrow, MockAMM, MockERC20, Groth16Verifier
│   ├── test/           # Foundry tests
│   └── script/         # Deploy scripts
├── relayer/            # Cargo workspace
│   ├── stealth-crypto/ # Stealth addresses, encryption, key derivation
│   └── relayer/        # Binary — monitors chain, executes, proves
├── circuits/           # Circom circuits
│   ├── instruction_commitment.circom
│   ├── execution_proof.circom
│   └── scripts/        # Compile, setup, prove, verify scripts
├── analysis/           # Privacy analysis scripts + reports
├── benchmarks/         # Benchmark scripts + results
└── tasks/              # Ralph PRD files
```

### Key Dependencies

**Rust:** ethers-rs (or alloy), k256 (secp256k1), aes-gcm, hkdf, sha2, tiny-keccak
**Solidity:** forge-std, OpenZeppelin (ERC20)
**Circom:** circomlib (Poseidon, comparators), snarkjs (proving + verification)
**Node:** snarkjs (proof generation + Solidity verifier export)

### Cryptographic Details

- **Stealth addresses:** ECDH on secp256k1 → HKDF-SHA256 → derived spend key
- **Encryption:** AES-256-GCM with 12-byte random nonce, key from HKDF(shared_secret, "encryption")
- **ZK hash:** Poseidon (circuit-friendly) for commitment inside circuits; keccak256 for on-chain matching
- **Proof system:** Groth16 on BN128 (snarkjs compatible, ~200 byte proofs, ~250k gas verification)

### Risks

- **Circom complexity:** Execution correctness circuit may be large (>100k constraints). Could hit snarkjs memory limits. Mitigation: start with instruction commitment only, add execution constraints incrementally.
- **Poseidon vs keccak mismatch:** Circuit uses Poseidon, Solidity uses keccak. Need a bridging strategy (either implement keccak in circom — expensive — or use Poseidon on-chain via precompile/library).
- **Proof generation time:** Groth16 proving on consumer hardware could be slow for large circuits. Benchmark early.
- **ethers-rs churn:** Library is in maintenance mode; alloy is the successor. Either works for a prototype.

## Success Metrics

- End-to-end flow completes on local Anvil with all assertions passing
- Stealth transfer is not linkable to final recipient from on-chain data alone
- Gas overhead < 5x compared to a direct swap (for single-relayer model)
- ZKP proof generation < 60 seconds on M1/M2 MacBook
- All Foundry tests, cargo tests, and circom witness tests pass

## Open Questions

1. Should Poseidon be used on-chain too (via a Solidity library) to avoid the keccak/Poseidon mismatch? Or accept dual hashing with a mapping?
2. What's the minimum bond-to-transaction-value ratio that makes theft unprofitable? (Literature suggests 2-10x)
3. Can ark-circom generate Groth16 proofs in Rust directly, or do we need to shell out to snarkjs? (Affects relayer architecture)
4. Should the prototype support ERC-20 stealth transfers or ETH-only? (ETH-only is simpler, ERC-20 is more realistic)
