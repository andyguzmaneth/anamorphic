pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";

// Proves that private instruction fields hash to a public commitment.
// Uses Poseidon hash with 8 inputs (one field element per instruction field).
//
// Private inputs (each as BN128 field element):
//   action_type, token_in, token_out, amount_in,
//   min_amount_out, recipient, deadline, nonce
//
// Public input:
//   commitment_hash
//
// Constraint:
//   Poseidon(action_type, token_in, token_out, amount_in,
//            min_amount_out, recipient, deadline, nonce) === commitment_hash

template InstructionCommitment() {
    // Private inputs
    signal input action_type;
    signal input token_in;
    signal input token_out;
    signal input amount_in;
    signal input min_amount_out;
    signal input recipient;
    signal input deadline;
    signal input nonce;

    // Public input
    signal input commitment_hash;

    // Compute Poseidon hash of all 8 instruction fields
    component hasher = Poseidon(8);
    hasher.inputs[0] <== action_type;
    hasher.inputs[1] <== token_in;
    hasher.inputs[2] <== token_out;
    hasher.inputs[3] <== amount_in;
    hasher.inputs[4] <== min_amount_out;
    hasher.inputs[5] <== recipient;
    hasher.inputs[6] <== deadline;
    hasher.inputs[7] <== nonce;

    // Constrain: computed hash must equal public commitment
    commitment_hash === hasher.out;
}

component main {public [commitment_hash]} = InstructionCommitment();
