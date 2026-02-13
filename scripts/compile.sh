#!/usr/bin/env bash
# Compile the instruction_commitment circuit with circom.
# Outputs R1CS + WASM to circuits/build/
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
CIRCOM="${HOME}/.cargo/bin/circom"
CIRCUIT="${ROOT_DIR}/circuits/instruction_commitment.circom"
OUT_DIR="${ROOT_DIR}/circuits/build"

if [ ! -f "$CIRCOM" ]; then
    echo "ERROR: circom not found at $CIRCOM"
    echo "Install: git clone https://github.com/iden3/circom.git && cd circom && cargo build --release && cargo install --path circom"
    exit 1
fi

mkdir -p "$OUT_DIR"

echo "Compiling ${CIRCUIT}..."
"$CIRCOM" "$CIRCUIT" --r1cs --wasm --sym -o "$OUT_DIR"

echo ""
echo "=== Compilation successful ==="
echo "R1CS:   ${OUT_DIR}/instruction_commitment.r1cs"
echo "WASM:   ${OUT_DIR}/instruction_commitment_js/"
echo "Symbol: ${OUT_DIR}/instruction_commitment.sym"

# Log constraint count
echo ""
echo "=== Constraint count ==="
npx snarkjs r1cs info "${OUT_DIR}/instruction_commitment.r1cs"
