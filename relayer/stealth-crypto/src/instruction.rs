use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use hkdf::Hkdf;
use sha2::Sha256;

use crate::keccak256;

/// DeFi instruction to be encrypted and sent to the relayer.
///
/// Fixed-width fields, serialized in big-endian for deterministic byte layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Action type (0 = swap, 1 = transfer, etc.)
    pub action_type: u8,
    /// Input token address
    pub token_in: [u8; 20],
    /// Output token address
    pub token_out: [u8; 20],
    /// Amount of input token (256-bit big-endian)
    pub amount_in: [u8; 32],
    /// Minimum acceptable output amount (256-bit big-endian)
    pub min_amount_out: [u8; 32],
    /// Recipient address for the output
    pub recipient: [u8; 20],
    /// Deadline timestamp (unix seconds)
    pub deadline: u64,
    /// Unique nonce to prevent replay
    pub nonce: u64,
}

/// Serialized instruction size:
/// 1 + 20 + 20 + 32 + 32 + 20 + 8 + 8 = 141 bytes
const SERIALIZED_LEN: usize = 141;

impl Instruction {
    /// Deterministic serialization to bytes (big-endian, fixed-width).
    pub fn to_bytes(&self) -> [u8; SERIALIZED_LEN] {
        let mut buf = [0u8; SERIALIZED_LEN];
        let mut offset = 0;

        buf[offset] = self.action_type;
        offset += 1;

        buf[offset..offset + 20].copy_from_slice(&self.token_in);
        offset += 20;

        buf[offset..offset + 20].copy_from_slice(&self.token_out);
        offset += 20;

        buf[offset..offset + 32].copy_from_slice(&self.amount_in);
        offset += 32;

        buf[offset..offset + 32].copy_from_slice(&self.min_amount_out);
        offset += 32;

        buf[offset..offset + 20].copy_from_slice(&self.recipient);
        offset += 20;

        buf[offset..offset + 8].copy_from_slice(&self.deadline.to_be_bytes());
        offset += 8;

        buf[offset..offset + 8].copy_from_slice(&self.nonce.to_be_bytes());

        buf
    }

    /// Deserialize from bytes.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() != SERIALIZED_LEN {
            return None;
        }

        let mut offset = 0;

        let action_type = data[offset];
        offset += 1;

        let mut token_in = [0u8; 20];
        token_in.copy_from_slice(&data[offset..offset + 20]);
        offset += 20;

        let mut token_out = [0u8; 20];
        token_out.copy_from_slice(&data[offset..offset + 20]);
        offset += 20;

        let mut amount_in = [0u8; 32];
        amount_in.copy_from_slice(&data[offset..offset + 32]);
        offset += 32;

        let mut min_amount_out = [0u8; 32];
        min_amount_out.copy_from_slice(&data[offset..offset + 32]);
        offset += 32;

        let mut recipient = [0u8; 20];
        recipient.copy_from_slice(&data[offset..offset + 20]);
        offset += 20;

        let deadline = u64::from_be_bytes(data[offset..offset + 8].try_into().ok()?);
        offset += 8;

        let nonce = u64::from_be_bytes(data[offset..offset + 8].try_into().ok()?);

        Some(Self {
            action_type,
            token_in,
            token_out,
            amount_in,
            min_amount_out,
            recipient,
            deadline,
            nonce,
        })
    }

    /// Compute the keccak256 commitment of the serialized instruction.
    pub fn commitment(&self) -> [u8; 32] {
        keccak256(&self.to_bytes())
    }
}

/// Derive a 256-bit AES encryption key from the ECDH shared secret via HKDF-SHA256.
pub fn derive_encryption_key(shared_secret: &[u8; 32]) -> [u8; 32] {
    let hkdf = Hkdf::<Sha256>::new(None, shared_secret);
    let mut key = [0u8; 32];
    hkdf.expand(b"stealth-defi-encryption", &mut key)
        .expect("32 bytes is a valid HKDF output length");
    key
}

/// Encrypt an instruction using AES-256-GCM.
/// Returns: nonce (12 bytes) || ciphertext || tag (16 bytes).
pub fn encrypt_instruction(
    instruction: &Instruction,
    shared_secret: &[u8; 32],
) -> Vec<u8> {
    let key = derive_encryption_key(shared_secret);
    let cipher = Aes256Gcm::new_from_slice(&key).expect("valid key length");

    // Generate random 12-byte nonce
    let nonce_bytes: [u8; 12] = {
        use rand::RngCore;
        let mut buf = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut buf);
        buf
    };
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = instruction.to_bytes();
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .expect("encryption should not fail with valid inputs");

    // nonce || ciphertext || tag (tag is appended by aes-gcm internally)
    let mut result = Vec::with_capacity(12 + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    result
}

/// Decrypt an encrypted instruction.
/// Input format: nonce (12 bytes) || ciphertext || tag.
/// Returns None if decryption fails (wrong key, tampered data, etc.)
pub fn decrypt_instruction(
    encrypted: &[u8],
    shared_secret: &[u8; 32],
) -> Option<Instruction> {
    if encrypted.len() < 12 {
        return None;
    }

    let key = derive_encryption_key(shared_secret);
    let cipher = Aes256Gcm::new_from_slice(&key).expect("valid key length");

    let nonce = Nonce::from_slice(&encrypted[..12]);
    let ciphertext = &encrypted[12..];

    let plaintext = cipher.decrypt(nonce, ciphertext).ok()?;
    Instruction::from_bytes(&plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_instruction() -> Instruction {
        Instruction {
            action_type: 0, // swap
            token_in: [0xAA; 20],
            token_out: [0xBB; 20],
            amount_in: {
                let mut buf = [0u8; 32];
                buf[31] = 100; // 100 wei
                buf
            },
            min_amount_out: {
                let mut buf = [0u8; 32];
                buf[31] = 90; // 90 wei
                buf
            },
            recipient: [0xCC; 20],
            deadline: 1_700_000_000,
            nonce: 42,
        }
    }

    fn test_shared_secret() -> [u8; 32] {
        [0x11; 32]
    }

    #[test]
    fn test_serialization_round_trip() {
        let instr = test_instruction();
        let bytes = instr.to_bytes();
        let recovered = Instruction::from_bytes(&bytes).expect("deserialization should succeed");
        assert_eq!(instr, recovered);
    }

    #[test]
    fn test_encryption_round_trip() {
        let instr = test_instruction();
        let secret = test_shared_secret();

        let encrypted = encrypt_instruction(&instr, &secret);
        let decrypted = decrypt_instruction(&encrypted, &secret)
            .expect("decryption should succeed with correct key");

        assert_eq!(instr, decrypted);
    }

    #[test]
    fn test_wrong_key_fails() {
        let instr = test_instruction();
        let secret = test_shared_secret();
        let wrong_secret = [0x22; 32];

        let encrypted = encrypt_instruction(&instr, &secret);
        let result = decrypt_instruction(&encrypted, &wrong_secret);

        assert!(result.is_none(), "Decryption with wrong key should fail");
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let instr = test_instruction();
        let secret = test_shared_secret();

        let mut encrypted = encrypt_instruction(&instr, &secret);
        // Tamper with a byte in the ciphertext (after the 12-byte nonce)
        if encrypted.len() > 20 {
            encrypted[20] ^= 0xFF;
        }

        let result = decrypt_instruction(&encrypted, &secret);
        assert!(
            result.is_none(),
            "Decryption of tampered ciphertext should fail (GCM auth)"
        );
    }

    #[test]
    fn test_commitment_deterministic() {
        let instr = test_instruction();
        let c1 = instr.commitment();
        let c2 = instr.commitment();
        assert_eq!(c1, c2, "Commitment should be deterministic");

        // Different instruction => different commitment
        let mut instr2 = test_instruction();
        instr2.nonce = 99;
        let c3 = instr2.commitment();
        assert_ne!(c1, c3, "Different instructions should produce different commitments");
    }

    #[test]
    fn test_different_nonces_produce_different_ciphertexts() {
        let instr = test_instruction();
        let secret = test_shared_secret();

        let encrypted1 = encrypt_instruction(&instr, &secret);
        let encrypted2 = encrypt_instruction(&instr, &secret);

        // The random nonce ensures different ciphertexts even for the same plaintext + key
        assert_ne!(
            encrypted1, encrypted2,
            "Different random nonces should produce different ciphertexts"
        );

        // Both should decrypt correctly
        let d1 = decrypt_instruction(&encrypted1, &secret).unwrap();
        let d2 = decrypt_instruction(&encrypted2, &secret).unwrap();
        assert_eq!(d1, d2);
    }

    #[test]
    fn test_hkdf_key_derivation_deterministic() {
        let secret = test_shared_secret();
        let k1 = derive_encryption_key(&secret);
        let k2 = derive_encryption_key(&secret);
        assert_eq!(k1, k2, "HKDF derivation should be deterministic");

        let different_secret = [0x22; 32];
        let k3 = derive_encryption_key(&different_secret);
        assert_ne!(k1, k3, "Different secrets should produce different keys");
    }

    #[test]
    fn test_deserialization_wrong_length() {
        let result = Instruction::from_bytes(&[0u8; 10]);
        assert!(result.is_none(), "Wrong-length data should fail to deserialize");
    }
}
