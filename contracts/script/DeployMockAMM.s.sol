// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "forge-std/Script.sol";
import {MockERC20} from "../src/MockERC20.sol";
import {MockAMM} from "../src/MockAMM.sol";

contract DeployMockAMM is Script {
    function run() external {
        vm.startBroadcast();

        MockERC20 tokenA = new MockERC20("Token A", "TKA");
        MockERC20 tokenB = new MockERC20("Token B", "TKB");
        MockAMM amm = new MockAMM(address(tokenA), address(tokenB));

        // Mint tokens to deployer
        uint256 mintAmount = 1_000_000 ether;
        tokenA.mint(msg.sender, mintAmount);
        tokenB.mint(msg.sender, mintAmount);

        // Add initial liquidity (100k of each)
        uint256 liquidityAmount = 100_000 ether;
        tokenA.approve(address(amm), liquidityAmount);
        tokenB.approve(address(amm), liquidityAmount);
        amm.addLiquidity(liquidityAmount, liquidityAmount);

        console.log("TokenA deployed at:", address(tokenA));
        console.log("TokenB deployed at:", address(tokenB));
        console.log("MockAMM deployed at:", address(amm));
        console.log("Liquidity added: %s of each token", liquidityAmount);

        vm.stopBroadcast();
    }
}
