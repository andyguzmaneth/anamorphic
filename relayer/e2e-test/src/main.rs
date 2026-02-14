use anyhow::{Context, Result};
use ethers::prelude::*;
use std::sync::Arc;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use stealth_crypto::{
    instruction::{encrypt_instruction, Instruction},
    stealth_address::{user_generate_stealth, RelayerKeys, EphemeralKeys},
};

// Generate contract bindings
abigen!(
    StealthEscrow,
    "../../contracts/out/StealthEscrow.sol/StealthEscrow.json"
);

abigen!(
    Groth16Verifier,
    "../../contracts/out/Groth16Verifier.sol/Groth16Verifier.json"
);

abigen!(
    MockAMM,
    "../../contracts/out/MockAMM.sol/MockAMM.json"
);

abigen!(
    MockERC20,
    "../../contracts/out/MockERC20.sol/MockERC20.json"
);

/// Gas metrics for different operations
#[derive(Debug, Default)]
struct GasMetrics {
    stealth_transfer: u128,
    token_approve: u128,
    token_swap: u128,
    token_transfer: u128,
    post_commitment: u128,
    verify_and_release: u128,
    total_relayer: u128,
    baseline_swap: u128,
}

impl GasMetrics {
    fn total(&self) -> u128 {
        self.stealth_transfer + self.total_relayer
    }

    fn overhead_percentage(&self) -> f64 {
        if self.baseline_swap == 0 {
            return 0.0;
        }
        ((self.total() as f64 - self.baseline_swap as f64) / self.baseline_swap as f64) * 100.0
    }
}

/// End-to-end integration test orchestrator
struct E2ETest {
    anvil_process: Option<Child>,
    provider: Arc<Provider<Http>>,
    deployer: LocalWallet,
    user: LocalWallet,
}

impl E2ETest {
    /// Start Anvil and initialize provider
    async fn new() -> Result<Self> {
        println!("🚀 Starting Anvil...");

        // Start Anvil with deterministic accounts
        let anvil = Command::new("anvil")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to start anvil - make sure it's installed")?;

        // Wait for Anvil to start
        tokio::time::sleep(Duration::from_millis(2000)).await;

        // Connect to Anvil
        let provider = Provider::<Http>::try_from("http://127.0.0.1:8545")
            .context("Failed to connect to Anvil")?;

        // Use Anvil's first account as deployer
        let deployer = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse::<LocalWallet>()?
            .with_chain_id(31337u64);

        // Use Anvil's second account as user
        let user = "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
            .parse::<LocalWallet>()?
            .with_chain_id(31337u64);

        println!("✅ Anvil started");
        println!("   Deployer: {:?}", deployer.address());
        println!("   User: {:?}", user.address());

        Ok(Self {
            anvil_process: Some(anvil),
            provider: Arc::new(provider),
            deployer,
            user,
        })
    }

    /// Deploy all contracts
    async fn deploy_contracts(&self) -> Result<(Address, Address, Address, Address, Address)> {
        println!("\n📦 Deploying contracts...");

        let client = SignerMiddleware::new(self.provider.clone(), self.deployer.clone());
        let client = Arc::new(client);

        // Deploy Groth16Verifier
        let verifier_factory = Groth16Verifier::deploy(client.clone(), ())?;
        let verifier = verifier_factory.send().await?;
        let verifier_addr = verifier.address();
        println!("   ✅ Groth16Verifier: {}", verifier_addr);

        // Deploy StealthEscrow with verifier address
        let escrow_factory = StealthEscrow::deploy(client.clone(), verifier_addr)?;
        let escrow = escrow_factory.send().await?;
        let escrow_addr = escrow.address();
        println!("   ✅ StealthEscrow: {}", escrow_addr);

        // Deploy MockERC20 tokens
        let token_a_factory = MockERC20::deploy(client.clone(), ("Token A".to_string(), "TKA".to_string()))?;
        let token_a = token_a_factory.send().await?;
        let token_a_addr = token_a.address();
        println!("   ✅ TokenA: {}", token_a_addr);

        let token_b_factory = MockERC20::deploy(client.clone(), ("Token B".to_string(), "TKB".to_string()))?;
        let token_b = token_b_factory.send().await?;
        let token_b_addr = token_b.address();
        println!("   ✅ TokenB: {}", token_b_addr);

        // Deploy MockAMM
        let amm_factory = MockAMM::deploy(client.clone(), (token_a_addr, token_b_addr))?;
        let amm = amm_factory.send().await?;
        let amm_addr = amm.address();
        println!("   ✅ MockAMM: {}", amm_addr);

        // Mint tokens to deployer
        let mint_amount = U256::from_dec_str("1000000000000000000000000")?; // 1M tokens
        token_a.mint(client.address(), mint_amount).send().await?.await?;
        token_b.mint(client.address(), mint_amount).send().await?.await?;
        println!("   💰 Minted tokens to deployer");

        // Add liquidity (100k of each)
        let liquidity_amount = U256::from_dec_str("100000000000000000000000")?; // 100k tokens
        token_a.approve(amm_addr, liquidity_amount).send().await?.await?;
        token_b.approve(amm_addr, liquidity_amount).send().await?.await?;
        amm.add_liquidity(liquidity_amount, liquidity_amount).send().await?.await?;
        println!("   💧 Added liquidity to AMM");

        Ok((escrow_addr, amm_addr, token_a_addr, token_b_addr, verifier_addr))
    }

    /// Generate relayer and user keys
    fn generate_keys(&self) -> (RelayerKeys, EphemeralKeys) {
        let mut rng = rand::thread_rng();
        let relayer_keys = RelayerKeys::generate(&mut rng);

        let ephemeral_privkey = k256::SecretKey::random(&mut rng);
        let ephemeral_pubkey = ephemeral_privkey.public_key();
        let ephemeral_keys = EphemeralKeys {
            privkey: ephemeral_privkey,
            pubkey: ephemeral_pubkey,
        };

        println!("\n🔑 Generated keys:");
        println!("   Relayer view pubkey: {}", hex::encode(relayer_keys.view_pubkey.to_sec1_bytes()));
        println!("   Relayer spend pubkey: {}", hex::encode(relayer_keys.spend_pubkey.to_sec1_bytes()));
        println!("   Ephemeral pubkey: {}", hex::encode(ephemeral_keys.pubkey.to_sec1_bytes()));

        (relayer_keys, ephemeral_keys)
    }

    /// User: create and send stealth transfer
    async fn send_stealth_transfer(
        &self,
        relayer_keys: &RelayerKeys,
        ephemeral_keys: &EphemeralKeys,
        token_a_addr: Address,
        token_b_addr: Address,
    ) -> Result<(H256, [u8; 20], [u8; 32], Instruction, u128)> {
        println!("\n👤 User creating stealth transfer...");

        // Generate stealth address
        let (stealth, shared_secret) = user_generate_stealth(
            &relayer_keys.spend_pubkey,
            &relayer_keys.view_pubkey,
            ephemeral_keys,
        );

        let stealth_eth_addr: Address = format!("0x{}", hex::encode(stealth.address))
            .parse()
            .context("Invalid stealth address")?;

        println!("   🎯 Stealth address: {}", stealth_eth_addr);

        // Create instruction: swap 1000 TokenA for at least 900 TokenB
        let swap_amount = U256::from_dec_str("1000000000000000000000")?; // 1000 tokens
        let min_out = U256::from_dec_str("900000000000000000000")?; // 900 tokens

        let mut amount_in = [0u8; 32];
        swap_amount.to_big_endian(&mut amount_in);

        let mut min_amount_out = [0u8; 32];
        min_out.to_big_endian(&mut min_amount_out);

        let mut token_in = [0u8; 20];
        token_in.copy_from_slice(&token_a_addr.as_bytes()[..20]);

        let mut token_out = [0u8; 20];
        token_out.copy_from_slice(&token_b_addr.as_bytes()[..20]);

        let mut recipient = [0u8; 20];
        recipient.copy_from_slice(&self.user.address().as_bytes()[..20]);

        let instruction = Instruction {
            action_type: 1,
            token_in,
            token_out,
            amount_in,
            min_amount_out,
            recipient,
            deadline: 9999999999, // Far future
            nonce: 42,
        };

        println!("   📋 Instruction:");
        println!("      Swap: {} TokenA → ≥{} TokenB", swap_amount, min_out);
        println!("      Recipient: {:?}", self.user.address());
        println!("      Commitment: 0x{}", hex::encode(instruction.commitment()));

        // Encrypt instruction
        let encrypted = encrypt_instruction(&instruction, &shared_secret);

        // Build calldata: ephemeral_pubkey || encrypted_instruction
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&ephemeral_keys.pubkey.to_sec1_bytes());
        calldata.extend_from_slice(&encrypted);

        println!("   📦 Calldata size: {} bytes", calldata.len());

        // Send ETH transfer with stealth payload
        let client = SignerMiddleware::new(self.provider.clone(), self.user.clone());
        let tx = TransactionRequest::new()
            .from(self.user.address())
            .to(stealth_eth_addr)
            .value(ethers::utils::parse_ether("2.0")?) // Send 2 ETH for gas
            .data(calldata);

        let pending_tx = client.send_transaction(tx, None).await?;
        let tx_hash = pending_tx.tx_hash();
        let receipt = pending_tx.await?.context("Transaction failed")?;
        let gas_used = receipt.gas_used.context("No gas used")?.as_u128();

        println!("   ✅ Stealth transfer sent: {}", tx_hash);
        println!("   ⛽ Gas used: {}", gas_used);

        Ok((tx_hash, stealth.address, shared_secret, instruction, gas_used))
    }

    /// Relayer: execute the full flow
    async fn relayer_execute(
        &self,
        relayer_keys: &RelayerKeys,
        ephemeral_keys: &EphemeralKeys,
        stealth_addr: [u8; 20],
        instruction: &Instruction,
        escrow_addr: Address,
        amm_addr: Address,
        token_a_addr: Address,
        token_b_addr: Address,
    ) -> Result<GasMetrics> {
        println!("\n🤖 Relayer executing instruction...");

        let mut metrics = GasMetrics::default();

        // Recover stealth private key
        let stealth_privkey = stealth_crypto::stealth_address::relayer_recover_stealth_privkey(
            relayer_keys,
            &ephemeral_keys.pubkey,
        );
        let signing_key = k256::ecdsa::SigningKey::from(stealth_privkey);
        let wallet = LocalWallet::from(signing_key).with_chain_id(31337u64);

        let stealth_eth_addr: Address = format!("0x{}", hex::encode(stealth_addr))
            .parse()
            .context("Invalid stealth address")?;

        println!("   🔓 Recovered stealth wallet: {}", stealth_eth_addr);

        let client = SignerMiddleware::new(self.provider.clone(), wallet);
        let client = Arc::new(client);

        // Parse amounts
        let amount_in = U256::from_big_endian(&instruction.amount_in);
        let min_amount_out = U256::from_big_endian(&instruction.min_amount_out);
        let recipient: Address = format!("0x{}", hex::encode(&instruction.recipient))
            .parse()
            .context("Invalid recipient")?;

        // Get tokens from stealth address (we'll need to mint them first)
        let deployer_client = SignerMiddleware::new(self.provider.clone(), self.deployer.clone());
        let deployer_client = Arc::new(deployer_client);
        let token_a = MockERC20::new(token_a_addr, deployer_client.clone());
        token_a.mint(stealth_eth_addr, amount_in).send().await?.await?;
        println!("   💰 Minted {} TokenA to stealth address", amount_in);

        // Execute swap
        println!("   🔄 Executing swap...");

        let token_a_stealth = MockERC20::new(token_a_addr, client.clone());
        let approve_receipt = token_a_stealth.approve(amm_addr, amount_in).send().await?.await?.context("Approve failed")?;
        metrics.token_approve = approve_receipt.gas_used.context("No gas used")?.as_u128();
        println!("      ✅ Approved AMM (gas: {})", metrics.token_approve);

        let amm = MockAMM::new(amm_addr, client.clone());
        let swap_receipt = amm.swap(token_a_addr, amount_in, min_amount_out).send().await?.await?.context("Swap failed")?;
        metrics.token_swap = swap_receipt.gas_used.context("No gas used")?.as_u128();
        println!("      ✅ Swap executed (gas: {})", metrics.token_swap);

        // Extract amount_out from logs
        let amount_out = swap_receipt.logs.iter()
            .find_map(|log| {
                if log.data.len() >= 64 {
                    Some(U256::from_big_endian(&log.data[32..64]))
                } else {
                    None
                }
            })
            .unwrap_or(min_amount_out);

        println!("      💰 Received {} TokenB", amount_out);

        // Transfer to recipient
        let token_b = MockERC20::new(token_b_addr, client.clone());
        let transfer_receipt = token_b.transfer(recipient, amount_out).send().await?.await?.context("Transfer failed")?;
        metrics.token_transfer = transfer_receipt.gas_used.context("No gas used")?.as_u128();
        println!("      ✅ Transferred to recipient (gas: {})", metrics.token_transfer);

        // Post commitment with bond
        println!("   📋 Posting commitment...");
        let commitment = instruction.commitment();
        let bond_amount = ethers::utils::parse_ether("1.0")?;

        let escrow = StealthEscrow::new(escrow_addr, client.clone());
        let post_receipt = escrow.post_commitment(commitment).value(bond_amount).send().await?.await?.context("Post commitment failed")?;
        metrics.post_commitment = post_receipt.gas_used.context("No gas used")?.as_u128();
        println!("      ✅ Commitment posted (gas: {})", metrics.post_commitment);

        // Extract commitment ID
        let commitment_id = post_receipt.logs.iter()
            .find_map(|log| {
                if log.topics.len() >= 2 {
                    Some(U256::from_big_endian(&log.topics[1].as_bytes()))
                } else {
                    None
                }
            })
            .context("Commitment ID not found")?;

        println!("      🆔 Commitment ID: {}", commitment_id);

        // Generate ZK proof
        println!("   🔐 Generating ZK proof...");
        let proof_start = Instant::now();
        let (proof_a, proof_b, proof_c, public_inputs) = self.generate_proof(instruction, amount_out).await?;
        let proof_time = proof_start.elapsed();
        println!("      ✅ Proof generated in {:.2}s", proof_time.as_secs_f64());

        // Submit proof
        println!("   📤 Submitting proof...");
        let verify_receipt = escrow.verify_and_release(commitment_id, proof_a, proof_b, proof_c, public_inputs)
            .send().await?.await?.context("Verify and release failed")?;
        metrics.verify_and_release = verify_receipt.gas_used.context("No gas used")?.as_u128();
        println!("      ✅ Bond released (gas: {})", metrics.verify_and_release);

        metrics.total_relayer = metrics.token_approve + metrics.token_swap + metrics.token_transfer + metrics.post_commitment + metrics.verify_and_release;
        println!("   ✨ Execution complete!");

        Ok(metrics)
    }

    /// Measure baseline swap gas (without stealth)
    async fn measure_baseline_swap(
        &self,
        amm_addr: Address,
        token_a_addr: Address,
        _token_b_addr: Address,
    ) -> Result<u128> {
        println!("\n📊 Measuring baseline swap gas...");

        let client = SignerMiddleware::new(self.provider.clone(), self.deployer.clone());
        let client = Arc::new(client);

        let swap_amount = U256::from_dec_str("1000000000000000000000")?;
        let min_out = U256::from_dec_str("900000000000000000000")?;

        let token_a = MockERC20::new(token_a_addr, client.clone());
        token_a.approve(amm_addr, swap_amount).send().await?.await?;

        let amm = MockAMM::new(amm_addr, client.clone());
        let swap_receipt = amm.swap(token_a_addr, swap_amount, min_out).send().await?.await?.context("Baseline swap failed")?;
        let gas_used = swap_receipt.gas_used.context("No gas used")?.as_u128();

        println!("   ⛽ Baseline swap gas: {}", gas_used);

        Ok(gas_used)
    }

    /// Generate ZK proof by shelling out to snarkjs
    async fn generate_proof(
        &self,
        instruction: &Instruction,
        execution_amount_out: U256,
    ) -> Result<([U256; 2], [[U256; 2]; 2], [U256; 2], [U256; 4])> {
        use std::fs;
        use std::path::PathBuf;

        let temp_dir = std::env::temp_dir().join(format!("stealth-e2e-{}", rand::random::<u64>()));
        fs::create_dir_all(&temp_dir)?;

        // Create input.json
        let input_json = self.create_proof_input(instruction, execution_amount_out)?;
        let input_path = temp_dir.join("input.json");
        fs::write(&input_path, input_json)?;

        // Circuit artifacts paths - use absolute path from project root
        let project_root = std::env::current_dir()?.parent().unwrap().to_path_buf();
        let circuit_root = project_root.join("circuits/build");
        let wasm_path = circuit_root.join("execution_proof_js/execution_proof.wasm");
        let zkey_path = circuit_root.join("execution_proof_final.zkey");
        let proof_path = temp_dir.join("proof.json");
        let public_path = temp_dir.join("public.json");

        // Run snarkjs
        let output = Command::new("npx")
            .args(&[
                "snarkjs", "groth16", "fullprove",
                input_path.to_str().unwrap(),
                wasm_path.to_str().unwrap(),
                zkey_path.to_str().unwrap(),
                proof_path.to_str().unwrap(),
                public_path.to_str().unwrap(),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            anyhow::bail!("snarkjs failed:\nSTDOUT: {}\nSTDERR: {}", stdout, stderr);
        }

        // Parse proof
        let proof_json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&proof_path)?)?;
        let public: Vec<String> = serde_json::from_str(&fs::read_to_string(&public_path)?)?;

        let pi_a = proof_json["pi_a"].as_array().unwrap();
        let pi_b = proof_json["pi_b"].as_array().unwrap();
        let pi_c = proof_json["pi_c"].as_array().unwrap();

        let proof_a = [
            U256::from_dec_str(pi_a[0].as_str().unwrap())?,
            U256::from_dec_str(pi_a[1].as_str().unwrap())?,
        ];

        // Swap Fp2 ordering for EVM
        let pi_b_0 = pi_b[0].as_array().unwrap();
        let pi_b_1 = pi_b[1].as_array().unwrap();
        let proof_b = [
            [U256::from_dec_str(pi_b_0[1].as_str().unwrap())?, U256::from_dec_str(pi_b_0[0].as_str().unwrap())?],
            [U256::from_dec_str(pi_b_1[1].as_str().unwrap())?, U256::from_dec_str(pi_b_1[0].as_str().unwrap())?],
        ];

        let proof_c = [
            U256::from_dec_str(pi_c[0].as_str().unwrap())?,
            U256::from_dec_str(pi_c[1].as_str().unwrap())?,
        ];

        let public_inputs = [
            U256::from_dec_str(&public[0])?,
            U256::from_dec_str(&public[1])?,
            U256::from_dec_str(&public[2])?,
            U256::from_dec_str(&public[3])?,
        ];

        fs::remove_dir_all(&temp_dir).ok();

        Ok((proof_a, proof_b, proof_c, public_inputs))
    }

    fn create_proof_input(&self, instruction: &Instruction, execution_amount_out: U256) -> Result<String> {
        use serde_json::json;

        // Compute Poseidon hash for commitment using Node.js helper
        let fields_json = json!({
            "action_type": instruction.action_type.to_string(),
            "token_in": U256::from_big_endian(&instruction.token_in).to_string(),
            "token_out": U256::from_big_endian(&instruction.token_out).to_string(),
            "amount_in": U256::from_big_endian(&instruction.amount_in).to_string(),
            "min_amount_out": U256::from_big_endian(&instruction.min_amount_out).to_string(),
            "recipient": U256::from_big_endian(&instruction.recipient).to_string(),
            "deadline": instruction.deadline.to_string(),
            "nonce": instruction.nonce.to_string(),
        });

        let project_root = std::env::current_dir()?.parent().unwrap().to_path_buf();
        let poseidon_script = project_root.join("scripts/compute_poseidon.js");

        // Call Node.js script to compute Poseidon hash
        let output = Command::new("node")
            .arg(&poseidon_script)
            .arg(fields_json.to_string())
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to compute Poseidon hash: {}", String::from_utf8_lossy(&output.stderr));
        }

        let commitment_hash = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let input = json!({
            "action_type": instruction.action_type.to_string(),
            "token_in": U256::from_big_endian(&instruction.token_in).to_string(),
            "token_out": U256::from_big_endian(&instruction.token_out).to_string(),
            "amount_in": U256::from_big_endian(&instruction.amount_in).to_string(),
            "min_amount_out": U256::from_big_endian(&instruction.min_amount_out).to_string(),
            "recipient": U256::from_big_endian(&instruction.recipient).to_string(),
            "deadline": instruction.deadline.to_string(),
            "nonce": instruction.nonce.to_string(),
            "execution_amount_out": execution_amount_out.to_string(),
            "execution_timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs().to_string(),
            "commitment_hash": commitment_hash,
            "expected_recipient": U256::from_big_endian(&instruction.recipient).to_string(),
            "min_expected_amount": U256::from_big_endian(&instruction.min_amount_out).to_string(),
            "max_deadline": instruction.deadline.to_string(),
        });

        Ok(serde_json::to_string_pretty(&input)?)
    }

    /// Verify final state
    async fn verify_results(
        &self,
        recipient: Address,
        token_b_addr: Address,
        expected_min: U256,
    ) -> Result<()> {
        println!("\n✅ Verifying final state...");

        let token_b = MockERC20::new(token_b_addr, self.provider.clone());
        let balance = token_b.balance_of(recipient).call().await?;

        println!("   💰 Recipient TokenB balance: {}", balance);

        if balance >= expected_min {
            println!("   ✅ Balance check passed: {} ≥ {}", balance, expected_min);
        } else {
            anyhow::bail!("Balance check failed: {} < {}", balance, expected_min);
        }

        Ok(())
    }

    /// Print summary
    fn print_summary(&self, metrics: &GasMetrics) {
        let divider = "=".repeat(60);
        println!("\n{}", divider);
        println!("📊 E2E TEST SUMMARY");
        println!("{}", divider);
        println!("\n⛽ Gas Costs:");
        println!("   Stealth transfer:    {:>10} gas", metrics.stealth_transfer);
        println!("   Token approve:       {:>10} gas", metrics.token_approve);
        println!("   Token swap:          {:>10} gas", metrics.token_swap);
        println!("   Token transfer:      {:>10} gas", metrics.token_transfer);
        println!("   Post commitment:     {:>10} gas", metrics.post_commitment);
        println!("   Verify & release:    {:>10} gas", metrics.verify_and_release);
        println!("   {}", "-".repeat(40));
        println!("   Total (user + relayer): {:>10} gas", metrics.total());
        println!("   Baseline swap:       {:>10} gas", metrics.baseline_swap);
        println!("   Overhead:            {:>9.2}%", metrics.overhead_percentage());
        println!("\n✅ ALL ASSERTIONS PASSED");
        println!("{}\n", divider);
    }
}

impl Drop for E2ETest {
    fn drop(&mut self) {
        if let Some(mut child) = self.anvil_process.take() {
            let _ = child.kill();
            println!("🛑 Anvil stopped");
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("🎬 StealthDeFi End-to-End Integration Test");
    println!("{}\n", "=".repeat(60));

    // Step 1: Start Anvil
    let test = E2ETest::new().await?;

    // Step 2: Deploy contracts
    let (escrow_addr, amm_addr, token_a_addr, token_b_addr, _verifier_addr) =
        test.deploy_contracts().await?;

    // Step 3: Generate keys
    let (relayer_keys, ephemeral_keys) = test.generate_keys();

    // Step 4: User sends stealth transfer
    let (_tx_hash, stealth_addr, _shared_secret, instruction, stealth_gas) =
        test.send_stealth_transfer(&relayer_keys, &ephemeral_keys, token_a_addr, token_b_addr).await?;

    // Step 5: Relayer executes
    let mut metrics = test.relayer_execute(
        &relayer_keys,
        &ephemeral_keys,
        stealth_addr,
        &instruction,
        escrow_addr,
        amm_addr,
        token_a_addr,
        token_b_addr,
    ).await?;

    metrics.stealth_transfer = stealth_gas;

    // Step 6: Measure baseline
    metrics.baseline_swap = test.measure_baseline_swap(amm_addr, token_a_addr, token_b_addr).await?;

    // Step 7: Verify results
    let min_amount_out = U256::from_big_endian(&instruction.min_amount_out);
    test.verify_results(test.user.address(), token_b_addr, min_amount_out).await?;

    // Step 8: Print summary
    test.print_summary(&metrics);

    println!("🎉 End-to-end test completed successfully!");
    std::process::exit(0)
}
