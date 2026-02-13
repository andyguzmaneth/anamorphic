// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {StealthEscrow} from "../src/StealthEscrow.sol";

contract StealthEscrowTest is Test {
    StealthEscrow public escrow;
    address relayer = address(0x1);
    address user = address(0x2);
    address challenger = address(0x3);
    bytes32 commitment = keccak256("test-commitment");

    function setUp() public {
        escrow = new StealthEscrow();
        vm.deal(relayer, 100 ether);
        vm.deal(user, 100 ether);
    }

    function test_PostCommitment() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 1 ether}(commitment);

        assertEq(id, 0);
        (
            bytes32 hash,
            uint256 bondAmount,
            address storedRelayer,
            uint256 challengeDeadline,
            bool zkpVerified,
            bool released
        ) = escrow.commitments(id);

        assertEq(hash, commitment);
        assertEq(bondAmount, 1 ether);
        assertEq(storedRelayer, relayer);
        assertEq(challengeDeadline, block.timestamp + 7 days);
        assertFalse(zkpVerified);
        assertFalse(released);
    }

    function test_PostCommitmentEmitsEvent() public {
        vm.prank(relayer);
        vm.expectEmit(true, true, false, true);
        emit StealthEscrow.CommitmentPosted(0, relayer, commitment, 1 ether);
        escrow.postCommitment{value: 1 ether}(commitment);
    }

    function test_RejectLowBond() public {
        vm.prank(relayer);
        vm.expectRevert("Bond below minimum");
        escrow.postCommitment{value: 0.5 ether}(commitment);
    }

    function test_ReleaseBondAfterDeadline() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 2 ether}(commitment);

        uint256 balanceBefore = relayer.balance;

        // Warp past challenge period
        vm.warp(block.timestamp + 7 days);

        vm.prank(relayer);
        escrow.releaseBond(id);

        assertEq(relayer.balance, balanceBefore + 2 ether);

        (, , , , , bool released) = escrow.commitments(id);
        assertTrue(released);
    }

    function test_RejectEarlyRelease() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 1 ether}(commitment);

        // Try to release before challenge deadline
        vm.warp(block.timestamp + 6 days);
        vm.prank(relayer);
        vm.expectRevert("Challenge period not over");
        escrow.releaseBond(id);
    }

    function test_RejectReleaseByNonRelayer() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 1 ether}(commitment);

        vm.warp(block.timestamp + 7 days);
        vm.prank(user);
        vm.expectRevert("Not the relayer");
        escrow.releaseBond(id);
    }

    function test_RejectDoubleRelease() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 1 ether}(commitment);

        vm.warp(block.timestamp + 7 days);
        vm.prank(relayer);
        escrow.releaseBond(id);

        vm.prank(relayer);
        vm.expectRevert("Already released");
        escrow.releaseBond(id);
    }

    function test_Challenge() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 1 ether}(commitment);

        vm.prank(challenger);
        vm.expectEmit(true, true, false, true);
        emit StealthEscrow.ChallengeSubmitted(id, challenger);
        escrow.challenge(id);
    }

    function test_RejectChallengeAfterDeadline() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 1 ether}(commitment);

        vm.warp(block.timestamp + 7 days);
        vm.prank(challenger);
        vm.expectRevert("Challenge period over");
        escrow.challenge(id);
    }

    function test_RecoverFundsAfterTimeout() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 3 ether}(commitment);

        uint256 userBalanceBefore = user.balance;

        // Warp past challenge deadline + recovery timeout (7 + 14 = 21 days)
        vm.warp(block.timestamp + 21 days);

        vm.prank(user);
        escrow.recoverFunds(id);

        assertEq(user.balance, userBalanceBefore + 3 ether);

        (, , , , , bool released) = escrow.commitments(id);
        assertTrue(released);
    }

    function test_RejectEarlyRecovery() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 1 ether}(commitment);

        // Warp to 20 days (before 21-day recovery timeout)
        vm.warp(block.timestamp + 20 days);

        vm.prank(user);
        vm.expectRevert("Recovery timeout not reached");
        escrow.recoverFunds(id);
    }

    function test_MultipleCommitments() public {
        bytes32 commitment2 = keccak256("test-commitment-2");

        vm.prank(relayer);
        uint256 id1 = escrow.postCommitment{value: 1 ether}(commitment);

        vm.prank(relayer);
        uint256 id2 = escrow.postCommitment{value: 2 ether}(commitment2);

        assertEq(id1, 0);
        assertEq(id2, 1);

        (bytes32 hash1, uint256 bond1, , , , ) = escrow.commitments(id1);
        (bytes32 hash2, uint256 bond2, , , , ) = escrow.commitments(id2);

        assertEq(hash1, commitment);
        assertEq(bond1, 1 ether);
        assertEq(hash2, commitment2);
        assertEq(bond2, 2 ether);
    }
}
