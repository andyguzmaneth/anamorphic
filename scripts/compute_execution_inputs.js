// Compute Poseidon hash for execution proof circuit and output input.json.
// Usage: node scripts/compute_execution_inputs.js [output_path] [--wrong-recipient]
const { buildPoseidon } = require("circomlibjs");

async function main() {
    const poseidon = await buildPoseidon();
    const F = poseidon.F;

    const wrongRecipient = process.argv.includes("--wrong-recipient");

    // Instruction fields (same test values as instruction_commitment)
    const action_type = "1";                                      // swap
    const token_in = "1097077688018008265106216665536940668749033598146"; // 0xC02a...Cc2
    const token_out = "408903889015040462818475765061977746270004345344"; // 0x4770...0000
    const amount_in = "1000000000000000000";                      // 1e18
    const min_amount_out = "990000000000000000";                  // 0.99e18
    const recipient = "741333281676505741094108358262146866408682839647"; // 0x8218...5e5f
    const deadline = "1700000000";
    const nonce = "42";

    // Compute commitment hash
    const inputs = [action_type, token_in, token_out, amount_in, min_amount_out, recipient, deadline, nonce];
    const hash = poseidon(inputs.map(x => BigInt(x)));
    const commitment_hash = F.toObject(hash).toString();

    // Execution results (private)
    const execution_amount_out = "995000000000000000"; // 0.995e18 — above min_amount_out
    const execution_timestamp = "1699999000";          // before deadline

    // Public inputs
    const expected_recipient = wrongRecipient
        ? "999999999999999999999999999999999999999999999999" // wrong recipient
        : recipient;                                         // correct recipient
    const min_expected_amount = min_amount_out;
    const max_deadline = deadline;

    const inputJson = {
        // Private instruction fields
        action_type,
        token_in,
        token_out,
        amount_in,
        min_amount_out,
        recipient,
        deadline,
        nonce,
        // Private execution results
        execution_amount_out,
        execution_timestamp,
        // Public inputs
        commitment_hash,
        expected_recipient,
        min_expected_amount,
        max_deadline
    };

    const outputPath = process.argv[2] || "circuits/build/execution_input.json";
    require("fs").writeFileSync(outputPath, JSON.stringify(inputJson, null, 2) + "\n");
    console.log("commitment_hash: " + commitment_hash);
    console.log("wrong_recipient: " + wrongRecipient);
    console.log("Written to: " + outputPath);
}

main().catch(e => { console.error(e); process.exit(1); });
