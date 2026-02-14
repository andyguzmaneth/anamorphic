// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {StealthEscrow} from "../src/StealthEscrow.sol";
import {Groth16Verifier} from "../src/Groth16Verifier.sol";

contract StealthEscrowTest is Test {
    StealthEscrow public escrow;
    Groth16Verifier public verifier;
    address relayer = address(0x1);
    address user = address(0x2);
    address challenger = address(0x3);
    bytes32 commitment = keccak256("test-commitment");

    function setUp() public {
        verifier = new Groth16Verifier();
        escrow = new StealthEscrow(address(verifier));
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

    // --- ZKP Verification Tests (US-008) ---

    // Helper: returns a valid Groth16 proof generated from the execution_proof circuit
    function _validProof() internal pure returns (
        uint256[2] memory a,
        uint256[2][2] memory b,
        uint256[2] memory c,
        uint256[4] memory pubSignals
    ) {
        // From circuits/build/proof.json (generated by snarkjs groth16 fullprove)
        a[0] = 5891667544514626888805183524213092777807054446765997974741398256416250209378;
        a[1] = 18197115894362439973471699934098941184190218664457517022055689133292809503511;

        // Note: B point inner pairs are swapped vs proof.json (Fp2 field extension ordering)
        b[0][0] = 20881138442845696973020676562932453820222382007326858078289643403651000539909;
        b[0][1] = 742241725793983168175977038263930361784346555442275399408646619440534541371;
        b[1][0] = 18842376418410783466224840279900472026977539522218089048393225279914993896365;
        b[1][1] = 14532144777020327299004920917719526995243847356908751716377658380904484241806;

        c[0] = 12031848300788114233029640220877326121543298286658190194809876605606815019113;
        c[1] = 8768071520988020161248037629961434953167467214382063552311449366904030507316;

        // From circuits/build/public.json
        // [commitment_hash, expected_recipient, min_expected_amount, max_deadline]
        pubSignals[0] = 15344805099227907589368610494303412963199918916201036799308050966100038413701;
        pubSignals[1] = 741333281676505741094108358262146866408682839647;
        pubSignals[2] = 990000000000000000;
        pubSignals[3] = 1700000000;
    }

    function test_VerifyAndReleaseBond() public {
        // Post a commitment with the relayer
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 2 ether}(commitment);

        uint256 balanceBefore = relayer.balance;

        // Get valid proof
        (
            uint256[2] memory a,
            uint256[2][2] memory b,
            uint256[2] memory c,
            uint256[4] memory pubSignals
        ) = _validProof();

        // Submit proof — should release bond immediately
        vm.prank(relayer);
        escrow.verifyAndRelease(id, a, b, c, pubSignals);

        // Verify bond was released
        assertEq(relayer.balance, balanceBefore + 2 ether);

        // Verify state updated
        (, , , , bool zkpVerified, bool released) = escrow.commitments(id);
        assertTrue(zkpVerified);
        assertTrue(released);
    }

    function test_VerifyAndReleaseEmitsEvents() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 1 ether}(commitment);

        (
            uint256[2] memory a,
            uint256[2][2] memory b,
            uint256[2] memory c,
            uint256[4] memory pubSignals
        ) = _validProof();

        vm.prank(relayer);
        vm.expectEmit(true, true, false, true);
        emit StealthEscrow.ZkpVerified(id, relayer);
        vm.expectEmit(true, true, false, true);
        emit StealthEscrow.BondReleased(id, relayer, 1 ether);
        escrow.verifyAndRelease(id, a, b, c, pubSignals);
    }

    function test_RevertInvalidProof() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 1 ether}(commitment);

        // Garbage proof values
        uint256[2] memory a = [uint256(1), uint256(2)];
        uint256[2][2] memory b = [[uint256(1), uint256(2)], [uint256(3), uint256(4)]];
        uint256[2] memory c = [uint256(1), uint256(2)];
        uint256[4] memory pubSignals = [uint256(1), uint256(2), uint256(3), uint256(4)];

        vm.prank(relayer);
        vm.expectRevert("Invalid proof");
        escrow.verifyAndRelease(id, a, b, c, pubSignals);

        // Verify bond was NOT released
        (, , , , bool zkpVerified, bool released) = escrow.commitments(id);
        assertFalse(zkpVerified);
        assertFalse(released);
    }

    function test_VerifyAndReleaseRejectsNonRelayer() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 1 ether}(commitment);

        (
            uint256[2] memory a,
            uint256[2][2] memory b,
            uint256[2] memory c,
            uint256[4] memory pubSignals
        ) = _validProof();

        vm.prank(user);
        vm.expectRevert("Not the relayer");
        escrow.verifyAndRelease(id, a, b, c, pubSignals);
    }

    function test_VerifyAndReleaseRejectsAlreadyReleased() public {
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 1 ether}(commitment);

        (
            uint256[2] memory a,
            uint256[2][2] memory b,
            uint256[2] memory c,
            uint256[4] memory pubSignals
        ) = _validProof();

        // First release via ZKP
        vm.prank(relayer);
        escrow.verifyAndRelease(id, a, b, c, pubSignals);

        // Second attempt should fail
        vm.prank(relayer);
        vm.expectRevert("Already released");
        escrow.verifyAndRelease(id, a, b, c, pubSignals);
    }

    function test_VerifyAndReleaseBeforeDeadline() public {
        // Verify that ZKP release works BEFORE the challenge period ends
        // (this is the whole point — skip the 7-day wait)
        vm.prank(relayer);
        uint256 id = escrow.postCommitment{value: 1 ether}(commitment);

        (
            uint256[2] memory a,
            uint256[2][2] memory b,
            uint256[2] memory c,
            uint256[4] memory pubSignals
        ) = _validProof();

        // Don't warp time — we're still within the challenge period
        uint256 balanceBefore = relayer.balance;

        vm.prank(relayer);
        escrow.verifyAndRelease(id, a, b, c, pubSignals);

        // Bond released immediately despite being within challenge period
        assertEq(relayer.balance, balanceBefore + 1 ether);
    }
}
