// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {MockERC20} from "../src/MockERC20.sol";
import {MockAMM} from "../src/MockAMM.sol";

contract MockAMMTest is Test {
    MockERC20 tokenA;
    MockERC20 tokenB;
    MockAMM amm;
    address alice = address(0x1);
    address bob = address(0x2);

    uint256 constant INITIAL_LIQUIDITY = 100_000 ether;

    function setUp() public {
        tokenA = new MockERC20("Token A", "TKA");
        tokenB = new MockERC20("Token B", "TKB");
        amm = new MockAMM(address(tokenA), address(tokenB));

        // Mint and add initial liquidity as deployer (this contract)
        tokenA.mint(address(this), INITIAL_LIQUIDITY);
        tokenB.mint(address(this), INITIAL_LIQUIDITY);
        tokenA.approve(address(amm), INITIAL_LIQUIDITY);
        tokenB.approve(address(amm), INITIAL_LIQUIDITY);
        amm.addLiquidity(INITIAL_LIQUIDITY, INITIAL_LIQUIDITY);
    }

    function test_addLiquidity() public {
        assertEq(amm.reserveA(), INITIAL_LIQUIDITY);
        assertEq(amm.reserveB(), INITIAL_LIQUIDITY);
        assertEq(amm.totalLP(), INITIAL_LIQUIDITY);
        assertEq(amm.lpBalance(address(this)), INITIAL_LIQUIDITY);
    }

    function test_addLiquidity_secondProvider() public {
        uint256 amountA = 50_000 ether;
        uint256 amountB = 50_000 ether;

        tokenA.mint(alice, amountA);
        tokenB.mint(alice, amountB);

        vm.startPrank(alice);
        tokenA.approve(address(amm), amountA);
        tokenB.approve(address(amm), amountB);
        amm.addLiquidity(amountA, amountB);
        vm.stopPrank();

        assertEq(amm.reserveA(), INITIAL_LIQUIDITY + amountA);
        assertEq(amm.reserveB(), INITIAL_LIQUIDITY + amountB);
        assertGt(amm.lpBalance(alice), 0);
    }

    function test_swapAtoB() public {
        uint256 swapAmount = 1_000 ether;
        tokenA.mint(alice, swapAmount);

        uint256 expectedOut = amm.getAmountOut(address(tokenA), swapAmount);
        assertGt(expectedOut, 0);

        vm.startPrank(alice);
        tokenA.approve(address(amm), swapAmount);
        uint256 amountOut = amm.swap(address(tokenA), swapAmount, 0);
        vm.stopPrank();

        assertEq(amountOut, expectedOut);
        assertEq(tokenA.balanceOf(alice), 0);
        assertEq(tokenB.balanceOf(alice), amountOut);

        // Reserves updated
        assertEq(amm.reserveA(), INITIAL_LIQUIDITY + swapAmount);
        assertEq(amm.reserveB(), INITIAL_LIQUIDITY - amountOut);
    }

    function test_swapBtoA() public {
        uint256 swapAmount = 500 ether;
        tokenB.mint(bob, swapAmount);

        uint256 expectedOut = amm.getAmountOut(address(tokenB), swapAmount);
        assertGt(expectedOut, 0);

        vm.startPrank(bob);
        tokenB.approve(address(amm), swapAmount);
        uint256 amountOut = amm.swap(address(tokenB), swapAmount, 0);
        vm.stopPrank();

        assertEq(amountOut, expectedOut);
        assertEq(tokenB.balanceOf(bob), 0);
        assertEq(tokenA.balanceOf(bob), amountOut);
    }

    function test_swapSlippageRevert() public {
        uint256 swapAmount = 1_000 ether;
        tokenA.mint(alice, swapAmount);

        uint256 expectedOut = amm.getAmountOut(address(tokenA), swapAmount);

        vm.startPrank(alice);
        tokenA.approve(address(amm), swapAmount);
        // Request more than possible output
        vm.expectRevert("Slippage exceeded");
        amm.swap(address(tokenA), swapAmount, expectedOut + 1);
        vm.stopPrank();
    }

    function test_swapZeroAmountRevert() public {
        vm.startPrank(alice);
        vm.expectRevert("Amount must be > 0");
        amm.swap(address(tokenA), 0, 0);
        vm.stopPrank();
    }

    function test_getAmountOut_constantProduct() public view {
        // With equal reserves of 100k each, swapping 1000 should give:
        // amountOut = (1000 * 100000) / (100000 + 1000) = 100000000 / 101000 ≈ 990.099
        uint256 amountOut = amm.getAmountOut(address(tokenA), 1_000 ether);
        // Should be less than input due to price impact
        assertLt(amountOut, 1_000 ether);
        assertGt(amountOut, 900 ether); // sanity check: not too much slippage on 1% of pool
    }

    function test_swapInvalidToken() public {
        vm.expectRevert("Invalid token");
        amm.swap(address(0xdead), 100, 0);
    }

    function test_swapEmitsEvent() public {
        uint256 swapAmount = 100 ether;
        tokenA.mint(alice, swapAmount);

        uint256 expectedOut = amm.getAmountOut(address(tokenA), swapAmount);

        vm.startPrank(alice);
        tokenA.approve(address(amm), swapAmount);

        vm.expectEmit(true, true, false, true);
        emit MockAMM.Swap(alice, address(tokenA), swapAmount, expectedOut);

        amm.swap(address(tokenA), swapAmount, 0);
        vm.stopPrank();
    }

    function test_addLiquidity_zeroAmountReverts() public {
        vm.expectRevert("Amounts must be > 0");
        amm.addLiquidity(0, 100);
    }

    function test_constantProductInvariant() public {
        uint256 kBefore = amm.reserveA() * amm.reserveB();

        uint256 swapAmount = 5_000 ether;
        tokenA.mint(alice, swapAmount);

        vm.startPrank(alice);
        tokenA.approve(address(amm), swapAmount);
        amm.swap(address(tokenA), swapAmount, 0);
        vm.stopPrank();

        uint256 kAfter = amm.reserveA() * amm.reserveB();
        // k should increase (or stay equal) due to rounding in favor of the pool
        assertGe(kAfter, kBefore);
    }
}
