pragma circom 2.0.0;

// Trivial test circuit: proves knowledge of a and b such that a * b = c
template Multiplier() {
    signal input a;
    signal input b;
    signal output c;

    c <== a * b;
}

component main = Multiplier();
