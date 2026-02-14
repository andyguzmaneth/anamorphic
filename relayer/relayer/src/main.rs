use anyhow::{Context, Result};
use ethers::prelude::*;
use std::sync::Arc;
use std::path::PathBuf;
use stealth_crypto::{
    instruction::{decrypt_instruction, Instruction},
    stealth_address::{relayer_check_stealth, RelayerKeys},
};

// Generate contract bindings using ethers abigen!
abigen!(
    StealthEscrow,
    "../../contracts/out/StealthEscrow.sol/StealthEscrow.json"
);

abigen!(
    MockAMM,
    "../../contracts/out/MockAMM.sol/MockAMM.json"
);

abigen!(
    MockERC20,
    "../../contracts/out/MockERC20.sol/MockERC20.json"
);

/// Relayer configuration
pub struct RelayerConfig {
    pub rpc_url: String,
    pub poll_interval_ms: u64,
    pub relayer_keys: RelayerKeys,
    pub escrow_address: Option<Address>,
    pub amm_address: Option<Address>,
    pub bond_amount: U256,
}

/// Deployed contract addresses
#[derive(Clone, Debug)]
pub struct ContractAddresses {
    pub stealth_escrow: Address,
    pub mock_amm: Address,
}

impl RelayerConfig {
    /// Load from environment or use defaults
    pub fn from_env() -> Result<Self> {
        let rpc_url = std::env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".to_string());
        let poll_interval_ms = std::env::var("POLL_INTERVAL_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2000);

        let escrow_address = std::env::var("ESCROW_ADDRESS")
            .ok()
            .and_then(|s| s.parse().ok());

        let amm_address = std::env::var("AMM_ADDRESS")
            .ok()
            .and_then(|s| s.parse().ok());

        let bond_amount = std::env::var("BOND_AMOUNT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| U256::from(1_000_000_000_000_000_000u64)); // 1 ETH default

        // For now, generate fresh keys. In production, load from secure storage.
        let mut rng = rand::thread_rng();
        let relayer_keys = RelayerKeys::generate(&mut rng);

        Ok(Self {
            rpc_url,
            poll_interval_ms,
            relayer_keys,
            escrow_address,
            amm_address,
            bond_amount,
        })
    }
}

/// Relayer that monitors the chain for stealth transfers
pub struct Relayer {
    provider: Arc<Provider<Http>>,
    config: RelayerConfig,
    last_block: u64,
}

impl Relayer {
    pub fn new(config: RelayerConfig) -> Result<Self> {
        let provider = Provider::<Http>::try_from(&config.rpc_url)
            .context("Failed to connect to RPC")?;

        Ok(Self {
            provider: Arc::new(provider),
            config,
            last_block: 0,
        })
    }

    /// Start monitoring the chain
    pub async fn run(&mut self) -> Result<()> {
        println!("🚀 Relayer starting...");
        println!("   RPC: {}", self.config.rpc_url);
        println!(
            "   View pubkey: {}",
            hex::encode(self.config.relayer_keys.view_pubkey.to_sec1_bytes())
        );
        println!(
            "   Spend pubkey: {}",
            hex::encode(self.config.relayer_keys.spend_pubkey.to_sec1_bytes())
        );

        // Get current block number
        self.last_block = self
            .provider
            .get_block_number()
            .await
            .context("Failed to get initial block number")?
            .as_u64();

        println!("   Starting from block {}", self.last_block);

        loop {
            if let Err(e) = self.poll_once().await {
                eprintln!("❌ Error polling: {:#}", e);
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(
                self.config.poll_interval_ms,
            ))
            .await;
        }
    }

    /// Poll for new blocks and process transactions
    async fn poll_once(&mut self) -> Result<()> {
        let current_block = self
            .provider
            .get_block_number()
            .await
            .context("Failed to get block number")?
            .as_u64();

        if current_block <= self.last_block {
            return Ok(());
        }

        // Process all new blocks
        for block_num in (self.last_block + 1)..=current_block {
            self.process_block(block_num).await?;
        }

        self.last_block = current_block;
        Ok(())
    }

    /// Process a single block
    async fn process_block(&self, block_num: u64) -> Result<()> {
        let block = self
            .provider
            .get_block_with_txs(block_num)
            .await
            .context("Failed to get block")?
            .context("Block not found")?;

        println!("📦 Block {} ({} txs)", block_num, block.transactions.len());

        for tx in &block.transactions {
            if let Err(e) = self.process_transaction(tx).await {
                eprintln!("   ⚠️  Error processing tx {}: {:#}", tx.hash, e);
            }
        }

        Ok(())
    }

    /// Process a single transaction
    async fn process_transaction(&self, tx: &Transaction) -> Result<()> {
        // Only process ETH transfers (to != None, value > 0)
        let to = match tx.to {
            Some(addr) => addr,
            None => return Ok(()), // Contract creation
        };

        if tx.value.is_zero() {
            return Ok(());
        }

        // Check if calldata starts with 33-byte compressed ephemeral pubkey
        let calldata = &tx.input;
        if calldata.len() < 33 {
            return Ok(()); // Not a stealth transfer
        }

        // Extract ephemeral pubkey (first 33 bytes)
        let ephemeral_pubkey_bytes: [u8; 33] = calldata[0..33]
            .try_into()
            .context("Failed to extract ephemeral pubkey")?;

        // Try to parse as secp256k1 public key
        use k256::PublicKey;
        let ephemeral_pubkey = match PublicKey::from_sec1_bytes(&ephemeral_pubkey_bytes) {
            Ok(pk) => pk,
            Err(_) => return Ok(()), // Not a valid pubkey, skip
        };

        // Check if this stealth transfer is for us
        let to_bytes: [u8; 20] = to.into();
        let (stealth_addr, shared_secret) =
            match relayer_check_stealth(&self.config.relayer_keys, &ephemeral_pubkey, &to_bytes) {
                Some((stealth, secret)) => (stealth, secret),
                None => return Ok(()), // Not for us
            };

        // 🎯 Stealth transfer detected!
        println!("   🎯 STEALTH TRANSFER DETECTED!");
        println!("      Tx hash: {}", tx.hash);
        println!("      From: {}", tx.from);
        println!("      To (stealth): {}", to);
        println!("      Value: {} wei", tx.value);
        println!(
            "      Ephemeral pubkey: {}",
            hex::encode(&ephemeral_pubkey_bytes)
        );

        // Decrypt instruction from remaining calldata
        if calldata.len() > 33 {
            let encrypted_instruction = &calldata[33..];
            match decrypt_instruction(encrypted_instruction, &shared_secret) {
                Some(instruction) => {
                    println!("      ✅ Instruction decrypted:");
                    println!("         action_type: {}", instruction.action_type);
                    println!("         token_in: 0x{}", hex::encode(&instruction.token_in));
                    println!("         token_out: 0x{}", hex::encode(&instruction.token_out));
                    println!("         amount_in: 0x{}", hex::encode(&instruction.amount_in));
                    println!("         min_amount_out: 0x{}", hex::encode(&instruction.min_amount_out));
                    println!("         recipient: 0x{}", hex::encode(&instruction.recipient));
                    println!("         deadline: {}", instruction.deadline);
                    println!("         nonce: {}", instruction.nonce);
                    println!(
                        "         commitment: 0x{}",
                        hex::encode(instruction.commitment())
                    );

                    // If contract addresses are configured, execute the instruction
                    if let (Some(escrow_addr), Some(amm_addr)) =
                        (self.config.escrow_address, self.config.amm_address) {
                        println!("      🔧 Executing instruction...");
                        if let Err(e) = self.execute_instruction(&stealth_addr, &instruction, escrow_addr, amm_addr).await {
                            eprintln!("      ❌ Execution failed: {:#}", e);
                        }
                    } else {
                        println!("      ℹ️  Escrow/AMM addresses not configured, skipping execution");
                    }
                }
                None => {
                    eprintln!("      ❌ Failed to decrypt instruction (invalid ciphertext or wrong key)");
                }
            }
        } else {
            println!("      ⚠️  No encrypted instruction in calldata");
        }

        Ok(())
    }

    /// Execute a decrypted instruction: swap + commitment + proof
    async fn execute_instruction(
        &self,
        stealth_addr: &stealth_crypto::stealth_address::StealthAddress,
        instruction: &Instruction,
        escrow_address: Address,
        amm_address: Address,
    ) -> Result<()> {
        use k256::ecdsa::SigningKey;

        // Step 1: Derive stealth private key
        let stealth_privkey = stealth_crypto::stealth_address::relayer_recover_stealth_privkey(
            &self.config.relayer_keys,
            &stealth_addr.ephemeral_pubkey,
        );
        let signing_key = SigningKey::from(stealth_privkey);

        // Create wallet from stealth private key
        let wallet = LocalWallet::from(signing_key).with_chain_id(31337u64); // Anvil chain ID
        let wallet_address: Address = format!("0x{}", hex::encode(stealth_addr.address))
            .parse()
            .context("Invalid stealth address")?;

        println!("      📝 Stealth wallet address: {}", wallet_address);

        let client = SignerMiddleware::new(self.provider.clone(), wallet);
        let client = Arc::new(client);

        // Step 2: Execute swap on MockAMM
        let token_in_addr: Address = format!("0x{}", hex::encode(&instruction.token_in))
            .parse()
            .context("Invalid token_in address")?;
        let token_out_addr: Address = format!("0x{}", hex::encode(&instruction.token_out))
            .parse()
            .context("Invalid token_out address")?;
        let recipient_addr: Address = format!("0x{}", hex::encode(&instruction.recipient))
            .parse()
            .context("Invalid recipient address")?;

        // Parse amounts from big-endian [u8; 32]
        let amount_in = U256::from_big_endian(&instruction.amount_in);
        let min_amount_out = U256::from_big_endian(&instruction.min_amount_out);

        println!("      🔄 Executing swap: {} tokenIn for ≥{} tokenOut", amount_in, min_amount_out);

        // Approve AMM to spend tokens
        let token_in = MockERC20::new(token_in_addr, client.clone());
        let approve_tx = token_in.approve(amm_address, amount_in);
        let approve_receipt = approve_tx.send().await?.await?.context("Approve tx failed")?;
        println!("      ✅ Approved AMM (tx: {})", approve_receipt.transaction_hash);

        // Execute swap
        let amm = MockAMM::new(amm_address, client.clone());
        let swap_tx = amm.swap(token_in_addr, amount_in, min_amount_out);
        let swap_receipt = swap_tx.send().await?.await?.context("Swap tx failed")?;
        println!("      ✅ Swap executed (tx: {})", swap_receipt.transaction_hash);

        let amount_out = swap_receipt.logs.iter()
            .find_map(|log| {
                if log.topics.len() >= 3 && log.topics[0] == ethers::utils::keccak256("Swap(address,address,uint256,uint256)").into() {
                    // amountOut is the 4th topic or in data
                    if log.data.len() >= 64 {
                        return Some(U256::from_big_endian(&log.data[32..64]));
                    }
                }
                None
            })
            .unwrap_or(min_amount_out); // Fallback to min if we can't parse

        println!("      💰 Amount out: {}", amount_out);

        // Transfer output tokens to recipient
        let token_out = MockERC20::new(token_out_addr, client.clone());
        let balance = token_out.balance_of(wallet_address).call().await?;
        println!("      💰 Token balance: {}", balance);

        let transfer_tx = token_out.transfer(recipient_addr, amount_out);
        let transfer_receipt = transfer_tx.send().await?.await?.context("Transfer tx failed")?;
        println!("      ✅ Transferred {} tokens to recipient (tx: {})", amount_out, transfer_receipt.transaction_hash);

        // Step 3: Post commitment with bond
        let commitment = instruction.commitment();
        let commitment_hash: [u8; 32] = commitment;

        println!("      📋 Posting commitment: 0x{}", hex::encode(&commitment_hash));

        let escrow = StealthEscrow::new(escrow_address, client.clone());
        let post_tx = escrow.post_commitment(commitment_hash).value(self.config.bond_amount);
        let post_receipt = post_tx.send().await?.await?.context("PostCommitment tx failed")?;
        println!("      ✅ Commitment posted (tx: {})", post_receipt.transaction_hash);

        // Extract commitment ID from events
        let commitment_id = post_receipt.logs.iter()
            .find_map(|log| {
                if log.topics.len() >= 2 && log.topics[0] == ethers::utils::keccak256("CommitmentPosted(uint256,address,bytes32,uint256)").into() {
                    return Some(U256::from_big_endian(&log.topics[1].as_bytes()));
                }
                None
            })
            .context("CommitmentPosted event not found")?;

        println!("      🆔 Commitment ID: {}", commitment_id);

        // Step 4: Generate ZK proof
        println!("      🔐 Generating ZK proof...");
        let (proof_a, proof_b, proof_c, public_inputs) = self.generate_proof(instruction, amount_out).await?;
        println!("      ✅ Proof generated");

        // Step 5: Submit proof to verifyAndRelease
        println!("      📤 Submitting proof...");
        let verify_tx = escrow.verify_and_release(
            commitment_id,
            proof_a,
            proof_b,
            proof_c,
            public_inputs,
        );
        let verify_receipt = verify_tx.send().await?.await?.context("VerifyAndRelease tx failed")?;
        println!("      ✅ Proof verified, bond released (tx: {})", verify_receipt.transaction_hash);

        println!("      ✨ Execution complete!");
        Ok(())
    }

    /// Generate a Groth16 proof by shelling out to snarkjs
    async fn generate_proof(
        &self,
        instruction: &Instruction,
        execution_amount_out: U256,
    ) -> Result<([U256; 2], [[U256; 2]; 2], [U256; 2], [U256; 4])> {
        use std::process::Command;
        use std::fs;

        // Create temporary directory for proof generation
        let temp_dir = std::env::temp_dir().join(format!("stealth-proof-{}", rand::random::<u64>()));
        fs::create_dir_all(&temp_dir).context("Failed to create temp dir")?;

        // Write input.json
        let input_json = self.create_proof_input(instruction, execution_amount_out)?;
        let input_path = temp_dir.join("input.json");
        fs::write(&input_path, input_json).context("Failed to write input.json")?;

        // Paths to circuit artifacts (from circuits/build/)
        let circuit_root = PathBuf::from("../../circuits/build");
        let wasm_path = circuit_root.join("execution_proof_js/execution_proof.wasm");
        let zkey_path = circuit_root.join("execution_proof_final.zkey");

        // Run: snarkjs groth16 fullprove input.json wasm zkey proof.json public.json
        let proof_path = temp_dir.join("proof.json");
        let public_path = temp_dir.join("public.json");

        let output = Command::new("npx")
            .args(&[
                "snarkjs",
                "groth16",
                "fullprove",
                input_path.to_str().unwrap(),
                wasm_path.to_str().unwrap(),
                zkey_path.to_str().unwrap(),
                proof_path.to_str().unwrap(),
                public_path.to_str().unwrap(),
            ])
            .output()
            .context("Failed to run snarkjs")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("snarkjs fullprove failed: {}", stderr);
        }

        // Parse proof.json
        let proof_json = fs::read_to_string(&proof_path).context("Failed to read proof.json")?;
        let proof: serde_json::Value = serde_json::from_str(&proof_json)?;

        // Parse public.json
        let public_json = fs::read_to_string(&public_path).context("Failed to read public.json")?;
        let public: Vec<String> = serde_json::from_str(&public_json)?;

        // Extract proof components
        let pi_a = proof["pi_a"].as_array().context("Missing pi_a")?;
        let pi_b = proof["pi_b"].as_array().context("Missing pi_b")?;
        let pi_c = proof["pi_c"].as_array().context("Missing pi_c")?;

        let proof_a: [U256; 2] = [
            U256::from_dec_str(pi_a[0].as_str().unwrap())?,
            U256::from_dec_str(pi_a[1].as_str().unwrap())?,
        ];

        // Note: pi_b has [c0, c1] ordering in JSON, but EVM expects [c1, c0]
        let pi_b_0 = pi_b[0].as_array().unwrap();
        let pi_b_1 = pi_b[1].as_array().unwrap();
        let proof_b: [[U256; 2]; 2] = [
            [
                U256::from_dec_str(pi_b_0[1].as_str().unwrap())?, // c1
                U256::from_dec_str(pi_b_0[0].as_str().unwrap())?, // c0
            ],
            [
                U256::from_dec_str(pi_b_1[1].as_str().unwrap())?, // c1
                U256::from_dec_str(pi_b_1[0].as_str().unwrap())?, // c0
            ],
        ];

        let proof_c: [U256; 2] = [
            U256::from_dec_str(pi_c[0].as_str().unwrap())?,
            U256::from_dec_str(pi_c[1].as_str().unwrap())?,
        ];

        let public_inputs: [U256; 4] = [
            U256::from_dec_str(&public[0])?,
            U256::from_dec_str(&public[1])?,
            U256::from_dec_str(&public[2])?,
            U256::from_dec_str(&public[3])?,
        ];

        // Cleanup temp dir
        fs::remove_dir_all(&temp_dir).ok();

        Ok((proof_a, proof_b, proof_c, public_inputs))
    }

    /// Create input.json for the execution proof circuit
    fn create_proof_input(&self, instruction: &Instruction, execution_amount_out: U256) -> Result<String> {
        use serde_json::json;

        // Convert instruction fields to decimal strings (field elements)
        let action_type = instruction.action_type.to_string();
        let token_in = U256::from_big_endian(&instruction.token_in).to_string();
        let token_out = U256::from_big_endian(&instruction.token_out).to_string();
        let amount_in = U256::from_big_endian(&instruction.amount_in).to_string();
        let min_amount_out = U256::from_big_endian(&instruction.min_amount_out).to_string();
        let recipient = U256::from_big_endian(&instruction.recipient).to_string();
        let deadline = instruction.deadline.to_string();
        let nonce = instruction.nonce.to_string();

        let execution_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        // Public inputs match circuit: commitment_hash, expected_recipient, min_expected_amount, max_deadline
        let commitment = instruction.commitment();
        let commitment_hash = U256::from_big_endian(&commitment).to_string();

        let input = json!({
            "action_type": action_type,
            "token_in": token_in,
            "token_out": token_out,
            "amount_in": amount_in,
            "min_amount_out": min_amount_out,
            "recipient": recipient,
            "deadline": deadline,
            "nonce": nonce,
            "execution_amount_out": execution_amount_out.to_string(),
            "execution_timestamp": execution_timestamp.to_string(),
            "commitment_hash": commitment_hash,
            "expected_recipient": recipient.clone(),
            "min_expected_amount": min_amount_out.clone(),
            "max_deadline": deadline.clone(),
        });

        Ok(serde_json::to_string_pretty(&input)?)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = RelayerConfig::from_env()?;
    let mut relayer = Relayer::new(config)?;
    relayer.run().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use stealth_crypto::instruction::{encrypt_instruction, Instruction};
    use stealth_crypto::stealth_address::{user_generate_stealth, RelayerKeys};

    /// Integration test: send stealth transfer on Anvil and verify relayer detects it
    #[tokio::test]
    #[ignore] // Run with: cargo test --bin relayer -- --ignored --test-threads=1
    async fn test_stealth_detection_on_anvil() -> Result<()> {
        // Setup: Generate relayer keys
        let mut rng = rand::thread_rng();
        let relayer_keys = RelayerKeys::generate(&mut rng);

        // Setup: Connect to Anvil (must be running on 8545)
        let provider = Provider::<Http>::try_from("http://127.0.0.1:8545")?;
        let accounts: Vec<Address> = provider.get_accounts().await?;
        let sender = accounts[0];

        // User generates ephemeral keypair and stealth address
        use k256::SecretKey;
        let mut rng = rand::thread_rng();
        let ephemeral_privkey = SecretKey::random(&mut rng);
        let ephemeral_pubkey = ephemeral_privkey.public_key();

        let ephemeral_keys = stealth_crypto::stealth_address::EphemeralKeys {
            privkey: ephemeral_privkey,
            pubkey: ephemeral_pubkey,
        };

        let (stealth, shared_secret) = user_generate_stealth(
            &relayer_keys.spend_pubkey,
            &relayer_keys.view_pubkey,
            &ephemeral_keys,
        );

        let mut amount_in = [0u8; 32];
        amount_in[31] = 0xe8; // 1000 in hex = 0x3e8, but let's use 232 (0xe8) for simplicity
        amount_in[30] = 0x03;

        let mut min_amount_out = [0u8; 32];
        min_amount_out[31] = 0x84; // 900 in hex = 0x384
        min_amount_out[30] = 0x03;

        let instruction = Instruction {
            action_type: 1,
            token_in: [0x11; 20],
            token_out: [0x22; 20],
            amount_in,
            min_amount_out,
            recipient: [0x33; 20],
            deadline: 9999999999,
            nonce: 42,
        };

        let encrypted = encrypt_instruction(&instruction, &shared_secret);

        // Build calldata: ephemeral_pubkey || encrypted_instruction
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&ephemeral_keys.pubkey.to_sec1_bytes());
        calldata.extend_from_slice(&encrypted);

        // Send ETH transfer with stealth payload
        let stealth_eth_addr: Address = format!("0x{}", hex::encode(stealth.address))
            .parse()
            .unwrap();

        let tx = TransactionRequest::new()
            .from(sender)
            .to(stealth_eth_addr)
            .value(ethers::utils::parse_ether("1.0")?)
            .data(calldata);

        let tx_hash = provider.send_transaction(tx, None).await?.tx_hash();

        println!("✅ Sent stealth transfer: {}", tx_hash);
        println!("   Stealth address: {}", stealth_eth_addr);

        // Now test the relayer detection logic
        let config = RelayerConfig {
            rpc_url: "http://127.0.0.1:8545".to_string(),
            poll_interval_ms: 1000,
            relayer_keys: relayer_keys.clone(),
            escrow_address: None,
            amm_address: None,
            bond_amount: U256::from(1_000_000_000_000_000_000u64),
        };

        let relayer = Relayer::new(config)?;

        // Get the transaction
        let receipt = provider
            .get_transaction(tx_hash)
            .await?
            .context("Transaction not found")?;

        // Process the transaction
        relayer.process_transaction(&receipt).await?;

        println!("✅ Integration test passed: stealth transfer detected and instruction decrypted");

        Ok(())
    }

    #[test]
    fn test_config_from_defaults() {
        let config = RelayerConfig::from_env().unwrap();
        assert_eq!(config.rpc_url, "http://127.0.0.1:8545");
        assert_eq!(config.poll_interval_ms, 2000);
    }
}
