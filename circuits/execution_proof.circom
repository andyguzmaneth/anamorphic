pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";
include "../node_modules/circomlib/circuits/comparators.circom";

// Proves that a relayer executed correctly: right recipient, sufficient output,
// within deadline — without revealing the full instruction.
//
// Includes instruction commitment verification as a sub-circuit.
//
// Private inputs (instruction fields — each as BN128 field element):
//   action_type, token_in, token_out, amount_in,
//   min_amount_out, recipient, deadline, nonce
//
// Additional private inputs (execution results):
//   execution_amount_out — actual amount the relayer delivered
//   execution_timestamp  — block.timestamp when execution occurred
//
// Public inputs:
//   commitment_hash      — Poseidon hash of all 8 instruction fields
//   expected_recipient   — recipient the verifier expects
//   min_expected_amount  — minimum output amount the verifier expects
//   max_deadline         — maximum deadline the verifier expects
//
// Constraints:
//   1. Poseidon(instruction_fields) === commitment_hash
//   2. recipient === expected_recipient
//   3. execution_amount_out >= min_amount_out (from instruction)
//   4. execution_timestamp <= deadline (from instruction)

template ExecutionProof() {
    // Private instruction fields
    signal input action_type;
    signal input token_in;
    signal input token_out;
    signal input amount_in;
    signal input min_amount_out;
    signal input recipient;
    signal input deadline;
    signal input nonce;

    // Private execution results
    signal input execution_amount_out;
    signal input execution_timestamp;

    // Public inputs
    signal input commitment_hash;
    signal input expected_recipient;
    signal input min_expected_amount;
    signal input max_deadline;

    // --- Constraint 1: Instruction commitment ---
    // Poseidon(8 instruction fields) must equal the public commitment_hash
    component hasher = Poseidon(8);
    hasher.inputs[0] <== action_type;
    hasher.inputs[1] <== token_in;
    hasher.inputs[2] <== token_out;
    hasher.inputs[3] <== amount_in;
    hasher.inputs[4] <== min_amount_out;
    hasher.inputs[5] <== recipient;
    hasher.inputs[6] <== deadline;
    hasher.inputs[7] <== nonce;
    commitment_hash === hasher.out;

    // --- Constraint 2: Correct recipient ---
    // The instruction's recipient must match the publicly expected recipient
    component recipientCheck = IsEqual();
    recipientCheck.in[0] <== recipient;
    recipientCheck.in[1] <== expected_recipient;
    recipientCheck.out === 1;

    // --- Constraint 3: Sufficient output ---
    // execution_amount_out >= min_amount_out (from the instruction)
    // Using 128 bits — sufficient for token amounts (max ~3.4e38)
    component amountCheck = GreaterEqThan(128);
    amountCheck.in[0] <== execution_amount_out;
    amountCheck.in[1] <== min_amount_out;
    amountCheck.out === 1;

    // --- Constraint 4: Within deadline ---
    // execution_timestamp <= deadline (from the instruction)
    // Using 64 bits — sufficient for Unix timestamps
    component deadlineCheck = LessEqThan(64);
    deadlineCheck.in[0] <== execution_timestamp;
    deadlineCheck.in[1] <== deadline;
    deadlineCheck.out === 1;
}

component main {public [commitment_hash, expected_recipient, min_expected_amount, max_deadline]} = ExecutionProof();
