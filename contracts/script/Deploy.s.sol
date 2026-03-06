// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "forge-std/Script.sol";
import {Groth16Verifier} from "../src/Groth16Verifier.sol";
import {StealthEscrow} from "../src/StealthEscrow.sol";
import {MockERC20} from "../src/MockERC20.sol";
import {MockAMM} from "../src/MockAMM.sol";

/// @notice Full deployment: Groth16Verifier → StealthEscrow → MockERC20 × 2 → MockAMM, then mint and add liquidity.
/// Use for local Anvil or testnet (e.g. Sepolia). Set RPC and PRIVATE_KEY for testnet.
contract Deploy is Script {
    function run() external {
        vm.startBroadcast();

        Groth16Verifier verifier = new Groth16Verifier();
        console.log("Groth16Verifier deployed at:", address(verifier));

        StealthEscrow escrow = new StealthEscrow(address(verifier));
        console.log("StealthEscrow deployed at:", address(escrow));

        MockERC20 tokenA = new MockERC20("Token A", "TKA");
        MockERC20 tokenB = new MockERC20("Token B", "TKB");
        console.log("TokenA deployed at:", address(tokenA));
        console.log("TokenB deployed at:", address(tokenB));

        MockAMM amm = new MockAMM(address(tokenA), address(tokenB));
        console.log("MockAMM deployed at:", address(amm));

        uint256 mintAmount = 1_000_000 ether;
        tokenA.mint(msg.sender, mintAmount);
        tokenB.mint(msg.sender, mintAmount);

        uint256 liquidityAmount = 100_000 ether;
        tokenA.approve(address(amm), liquidityAmount);
        tokenB.approve(address(amm), liquidityAmount);
        amm.addLiquidity(liquidityAmount, liquidityAmount);
        console.log("Liquidity added: %s of each token", liquidityAmount);

        vm.stopBroadcast();

        console.log("--- Set these for the relayer ---");
        console.log("export ESCROW_ADDRESS=%s", address(escrow));
        console.log("export AMM_ADDRESS=%s", address(amm));
    }
}
