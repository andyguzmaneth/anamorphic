// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {MockERC20} from "./MockERC20.sol";

contract MockAMM {
    MockERC20 public tokenA;
    MockERC20 public tokenB;

    uint256 public reserveA;
    uint256 public reserveB;

    // Simple LP tracking per address (not an ERC20)
    mapping(address => uint256) public lpBalance;
    uint256 public totalLP;

    event LiquidityAdded(address indexed provider, uint256 amountA, uint256 amountB, uint256 lpMinted);
    event Swap(address indexed sender, address indexed tokenIn, uint256 amountIn, uint256 amountOut);

    constructor(address _tokenA, address _tokenB) {
        tokenA = MockERC20(_tokenA);
        tokenB = MockERC20(_tokenB);
    }

    function addLiquidity(uint256 amountA, uint256 amountB) external {
        require(amountA > 0 && amountB > 0, "Amounts must be > 0");

        tokenA.transferFrom(msg.sender, address(this), amountA);
        tokenB.transferFrom(msg.sender, address(this), amountB);

        uint256 lpMinted;
        if (totalLP == 0) {
            lpMinted = amountA; // first deposit: LP = amountA as baseline
        } else {
            lpMinted = (amountA * totalLP) / reserveA;
        }

        reserveA += amountA;
        reserveB += amountB;
        lpBalance[msg.sender] += lpMinted;
        totalLP += lpMinted;

        emit LiquidityAdded(msg.sender, amountA, amountB, lpMinted);
    }

    function getAmountOut(address tokenIn, uint256 amountIn) public view returns (uint256) {
        require(amountIn > 0, "Amount must be > 0");

        (uint256 reserveIn, uint256 reserveOut) = _getReserves(tokenIn);
        require(reserveIn > 0 && reserveOut > 0, "No liquidity");

        // Constant product: (reserveIn + amountIn) * (reserveOut - amountOut) = reserveIn * reserveOut
        // amountOut = reserveOut - (reserveIn * reserveOut) / (reserveIn + amountIn)
        // Simplified: amountOut = (amountIn * reserveOut) / (reserveIn + amountIn)
        uint256 amountOut = (amountIn * reserveOut) / (reserveIn + amountIn);
        return amountOut;
    }

    function swap(address tokenIn, uint256 amountIn, uint256 minAmountOut) external returns (uint256 amountOut) {
        require(amountIn > 0, "Amount must be > 0");

        amountOut = getAmountOut(tokenIn, amountIn);
        require(amountOut >= minAmountOut, "Slippage exceeded");
        require(amountOut > 0, "Insufficient output");

        (MockERC20 inToken, MockERC20 outToken) = _getTokens(tokenIn);

        inToken.transferFrom(msg.sender, address(this), amountIn);
        outToken.transfer(msg.sender, amountOut);

        // Update reserves from actual balances
        reserveA = tokenA.balanceOf(address(this));
        reserveB = tokenB.balanceOf(address(this));

        emit Swap(msg.sender, tokenIn, amountIn, amountOut);
    }

    function _getReserves(address tokenIn) internal view returns (uint256 reserveIn, uint256 reserveOut) {
        if (tokenIn == address(tokenA)) {
            return (reserveA, reserveB);
        } else if (tokenIn == address(tokenB)) {
            return (reserveB, reserveA);
        } else {
            revert("Invalid token");
        }
    }

    function _getTokens(address tokenIn) internal view returns (MockERC20 inToken, MockERC20 outToken) {
        if (tokenIn == address(tokenA)) {
            return (tokenA, tokenB);
        } else if (tokenIn == address(tokenB)) {
            return (tokenB, tokenA);
        } else {
            revert("Invalid token");
        }
    }
}
