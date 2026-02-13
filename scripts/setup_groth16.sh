#!/usr/bin/env bash
# Groth16 trusted setup for the execution_proof circuit.
# 1. Powers of tau ceremony (BN128, 2^16 constraints)
# 2. Circuit-specific phase 2
# 3. Export proving key, verification key, and Solidity verifier
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="${ROOT_DIR}/circuits/build"
R1CS="${BUILD_DIR}/execution_proof.r1cs"
CONTRACTS_DIR="${ROOT_DIR}/contracts/src"

if [ ! -f "$R1CS" ]; then
    echo "ERROR: Circuit not compiled. Run scripts/compile_execution_proof.sh first."
    exit 1
fi

echo "=== Phase 1: Powers of Tau ==="
npx snarkjs powersoftau new bn128 16 "${BUILD_DIR}/pot16_0000.ptau" -v
npx snarkjs powersoftau contribute "${BUILD_DIR}/pot16_0000.ptau" "${BUILD_DIR}/pot16_0001.ptau" --name="First contribution" -v -e="random entropy for stealth-defi"
npx snarkjs powersoftau prepare phase2 "${BUILD_DIR}/pot16_0001.ptau" "${BUILD_DIR}/pot16_final.ptau" -v

echo ""
echo "=== Phase 2: Circuit-specific setup ==="
npx snarkjs groth16 setup "$R1CS" "${BUILD_DIR}/pot16_final.ptau" "${BUILD_DIR}/execution_proof_0000.zkey"
npx snarkjs zkey contribute "${BUILD_DIR}/execution_proof_0000.zkey" "${BUILD_DIR}/execution_proof_final.zkey" --name="First contribution" -v -e="random entropy for circuit setup"

echo ""
echo "=== Export verification key ==="
npx snarkjs zkey export verificationkey "${BUILD_DIR}/execution_proof_final.zkey" "${BUILD_DIR}/verification_key.json"

echo ""
echo "=== Export Solidity verifier ==="
npx snarkjs zkey export solidityverifier "${BUILD_DIR}/execution_proof_final.zkey" "${CONTRACTS_DIR}/Groth16Verifier.sol"
echo "Solidity verifier written to: ${CONTRACTS_DIR}/Groth16Verifier.sol"

echo ""
echo "=== Setup complete ==="
echo "Proving key:       ${BUILD_DIR}/execution_proof_final.zkey"
echo "Verification key:  ${BUILD_DIR}/verification_key.json"
echo "Solidity verifier: ${CONTRACTS_DIR}/Groth16Verifier.sol"
