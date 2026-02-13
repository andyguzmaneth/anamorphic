// Compute Poseidon hash for 8 instruction fields and output input.json for witness generation.
// Usage: node scripts/compute_poseidon.js [output_path]
const { buildPoseidon } = require("circomlibjs");

async function main() {
    const poseidon = await buildPoseidon();
    const F = poseidon.F;

    // Test instruction fields (as field elements):
    const action_type = "1";                                      // swap
    const token_in = "1097077688018008265106216665536940668749033598146"; // 0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2
    const token_out = "408903889015040462818475765061977746270004345344"; // 0x4770...0000 (fictional)
    const amount_in = "1000000000000000000";                      // 1e18
    const min_amount_out = "990000000000000000";                  // 0.99e18
    const recipient = "741333281676505741094108358262146866408682839647"; // 0x8218...5e5f (fictional)
    const deadline = "1700000000";
    const nonce = "42";

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
