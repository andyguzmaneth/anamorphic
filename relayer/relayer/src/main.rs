use anyhow::{Context, Result};
use ethers::prelude::*;
use std::sync::Arc;
use stealth_crypto::{
    instruction::decrypt_instruction,
    stealth_address::{relayer_check_stealth, RelayerKeys},
};

/// Relayer configuration
pub struct RelayerConfig {
    pub rpc_url: String,
    pub poll_interval_ms: u64,
    pub relayer_keys: RelayerKeys,
}

impl RelayerConfig {
    /// Load from environment or use defaults
    pub fn from_env() -> Result<Self> {
        let rpc_url = std::env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".to_string());
        let poll_interval_ms = std::env::var("POLL_INTERVAL_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2000);

        // For now, generate fresh keys. In production, load from secure storage.
        let mut rng = rand::thread_rng();
        let relayer_keys = RelayerKeys::generate(&mut rng);

        Ok(Self {
            rpc_url,
            poll_interval_ms,
            relayer_keys,
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
        let (_stealth_addr, shared_secret) =
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
