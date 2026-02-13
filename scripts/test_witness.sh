#!/usr/bin/env bash
# Generate a witness for the instruction_commitment circuit with known test inputs
# and verify it satisfies the R1CS constraints.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="${ROOT_DIR}/circuits/build"
WASM_DIR="${BUILD_DIR}/instruction_commitment_js"
R1CS="${BUILD_DIR}/instruction_commitment.r1cs"
INPUT="${BUILD_DIR}/input.json"
WITNESS="${BUILD_DIR}/witness.wtns"

# Check that the circuit has been compiled
if [ ! -f "$R1CS" ] || [ ! -d "$WASM_DIR" ]; then
    echo "ERROR: Circuit not compiled. Run scripts/compile.sh first."
    exit 1
fi

# Step 1: Compute Poseidon hash and generate input.json
echo "=== Step 1: Computing Poseidon hash for test inputs ==="
node "${SCRIPT_DIR}/compute_poseidon.js" "$INPUT"
echo ""

# Step 2: Generate witness
echo "=== Step 2: Generating witness ==="
node "${WASM_DIR}/generate_witness.js" "${WASM_DIR}/instruction_commitment.wasm" "$INPUT" "$WITNESS"
echo "Witness generated: ${WITNESS}"
echo ""

# Step 3: Verify witness satisfies R1CS constraints
echo "=== Step 3: Verifying witness against R1CS ==="
npx snarkjs wtns check "$R1CS" "$WITNESS"
echo ""

echo "=== All checks passed ==="
