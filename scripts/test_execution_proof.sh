#!/usr/bin/env bash
# Test the execution_proof circuit:
#  1. Generate witness with valid inputs and verify proof
#  2. Attempt witness generation with invalid inputs (wrong recipient) — must fail
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="${ROOT_DIR}/circuits/build"
WASM_DIR="${BUILD_DIR}/execution_proof_js"
R1CS="${BUILD_DIR}/execution_proof.r1cs"
ZKEY="${BUILD_DIR}/execution_proof_final.zkey"
VKEY="${BUILD_DIR}/verification_key.json"

# Check prerequisites
if [ ! -f "$R1CS" ] || [ ! -d "$WASM_DIR" ]; then
    echo "ERROR: Circuit not compiled. Run scripts/compile_execution_proof.sh first."
    exit 1
fi

if [ ! -f "$ZKEY" ] || [ ! -f "$VKEY" ]; then
    echo "ERROR: Groth16 setup not done. Run scripts/setup_groth16.sh first."
    exit 1
fi

# ===== TEST 1: Valid proof =====
echo "========================================="
echo "TEST 1: Valid inputs — proof generation and verification"
echo "========================================="

INPUT="${BUILD_DIR}/execution_input.json"
WITNESS="${BUILD_DIR}/execution_witness.wtns"
PROOF="${BUILD_DIR}/proof.json"
PUBLIC="${BUILD_DIR}/public.json"

# Generate valid input.json
echo "--- Generating valid input.json ---"
node "${SCRIPT_DIR}/compute_execution_inputs.js" "$INPUT"
echo ""

# Generate witness
echo "--- Generating witness ---"
node "${WASM_DIR}/generate_witness.js" "${WASM_DIR}/execution_proof.wasm" "$INPUT" "$WITNESS"
echo "Witness generated: ${WITNESS}"
echo ""

# Check witness against R1CS
echo "--- Checking witness against R1CS ---"
npx snarkjs wtns check "$R1CS" "$WITNESS"
echo ""

# Generate Groth16 proof
echo "--- Generating Groth16 proof ---"
npx snarkjs groth16 prove "$ZKEY" "$WITNESS" "$PROOF" "$PUBLIC"
echo "Proof generated: ${PROOF}"
echo "Public signals: ${PUBLIC}"
echo ""

# Verify proof
echo "--- Verifying proof ---"
npx snarkjs groth16 verify "$VKEY" "$PUBLIC" "$PROOF"
echo ""
echo "TEST 1 PASSED: Valid proof generated and verified."
echo ""

# ===== TEST 2: Invalid inputs (wrong recipient) =====
echo "========================================="
echo "TEST 2: Invalid inputs (wrong recipient) — witness must fail"
echo "========================================="

INVALID_INPUT="${BUILD_DIR}/execution_input_invalid.json"
INVALID_WITNESS="${BUILD_DIR}/execution_witness_invalid.wtns"

# Generate input.json with wrong recipient
echo "--- Generating invalid input.json (wrong recipient) ---"
node "${SCRIPT_DIR}/compute_execution_inputs.js" "$INVALID_INPUT" --wrong-recipient
echo ""

# Attempt witness generation — should fail due to constraint violation
echo "--- Attempting witness generation (should fail) ---"
if node "${WASM_DIR}/generate_witness.js" "${WASM_DIR}/execution_proof.wasm" "$INVALID_INPUT" "$INVALID_WITNESS" 2>&1; then
    echo ""
    echo "TEST 2 FAILED: Witness generation should have failed for wrong recipient!"
    exit 1
else
    echo ""
    echo "TEST 2 PASSED: Witness generation correctly rejected invalid inputs."
fi

echo ""
echo "========================================="
echo "ALL TESTS PASSED"
echo "========================================="
