// Compute Poseidon hash for 8 instruction fields
// Usage: node scripts/compute_poseidon.js '{"action_type":"1",...}'  (output just the hash)
// OR:    node scripts/compute_poseidon.js [output_path] (use built-in test values)
const { buildPoseidon } = require("circomlibjs");

async function main() {
    const poseidon = await buildPoseidon();
    const F = poseidon.F;

    // Check if JSON input provided as argument
    let action_type, token_in, token_out, amount_in, min_amount_out, recipient, deadline, nonce;

    if (process.argv[2] && process.argv[2].startsWith("{")) {
        // Parse JSON from command line
        const fields = JSON.parse(process.argv[2]);
        action_type = fields.action_type;
        token_in = fields.token_in;
        token_out = fields.token_out;
        amount_in = fields.amount_in;
        min_amount_out = fields.min_amount_out;
        recipient = fields.recipient;
        deadline = fields.deadline;
        nonce = fields.nonce;

        // Compute and output just the hash
        const inputs = [action_type, token_in, token_out, amount_in, min_amount_out, recipient, deadline, nonce];
        const hash = poseidon(inputs.map(x => BigInt(x)));
        const commitment_hash = F.toObject(hash).toString();
        console.log(commitment_hash);
        return;
    }

    // Otherwise, use built-in test values and write to file
    action_type = "1";
    token_in = "1097077688018008265106216665536940668749033598146";
    token_out = "408903889015040462818475765061977746270004345344";
    amount_in = "1000000000000000000";
    min_amount_out = "990000000000000000";
    recipient = "741333281676505741094108358262146866408682839647";
    deadline = "1700000000";
    nonce = "42";

    const inputs = [action_type, token_in, token_out, amount_in, min_amount_out, recipient, deadline, nonce];
    const hash = poseidon(inputs.map(x => BigInt(x)));
    const commitment_hash = F.toObject(hash).toString();

    const inputJson = {
        action_type,
        token_in,
        token_out,
        amount_in,
        min_amount_out,
        recipient,
        deadline,
        nonce,
        commitment_hash
    };

    const outputPath = process.argv[2] || "circuits/build/input.json";
    require("fs").writeFileSync(outputPath, JSON.stringify(inputJson, null, 2) + "\n");
    console.log("commitment_hash: " + commitment_hash);
    console.log("Written to: " + outputPath);
}

main().catch(e => { console.error(e); process.exit(1); });
