// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Groth16Verifier} from "./Groth16Verifier.sol";

contract StealthEscrow {
    struct Commitment {
        bytes32 commitmentHash;
        uint256 bondAmount;
        address relayer;
        uint256 challengeDeadline;
        bool zkpVerified;
        bool released;
    }

    uint256 public minimumBond = 1 ether;
    uint256 public constant CHALLENGE_PERIOD = 7 days;
    uint256 public constant RECOVERY_TIMEOUT = 14 days;

    Groth16Verifier public immutable verifier;

    uint256 public nextCommitmentId;
    mapping(uint256 => Commitment) public commitments;

    event CommitmentPosted(uint256 indexed id, address indexed relayer, bytes32 commitmentHash, uint256 bondAmount);
    event BondReleased(uint256 indexed id, address indexed relayer, uint256 bondAmount);
    event ChallengeSubmitted(uint256 indexed id, address indexed challenger);
    event FundsRecovered(uint256 indexed id, address indexed recoverer, uint256 amount);
    event ZkpVerified(uint256 indexed id, address indexed relayer);

    constructor(address _verifier) {
        verifier = Groth16Verifier(_verifier);
    }

    function postCommitment(bytes32 commitment) external payable returns (uint256) {
        require(msg.value >= minimumBond, "Bond below minimum");

        uint256 id = nextCommitmentId++;
        commitments[id] = Commitment({
            commitmentHash: commitment,
            bondAmount: msg.value,
            relayer: msg.sender,
            challengeDeadline: block.timestamp + CHALLENGE_PERIOD,
            zkpVerified: false,
            released: false
        });

        emit CommitmentPosted(id, msg.sender, commitment, msg.value);
        return id;
    }

    function releaseBond(uint256 id) external {
        Commitment storage c = commitments[id];
        require(c.relayer == msg.sender, "Not the relayer");
        require(!c.released, "Already released");
        require(block.timestamp >= c.challengeDeadline, "Challenge period not over");

        c.released = true;
        uint256 amount = c.bondAmount;

        emit BondReleased(id, msg.sender, amount);
        (bool success,) = msg.sender.call{value: amount}("");
        require(success, "Transfer failed");
    }

    function challenge(uint256 id) external {
        Commitment storage c = commitments[id];
        require(!c.released, "Already released");
        require(block.timestamp < c.challengeDeadline, "Challenge period over");

        emit ChallengeSubmitted(id, msg.sender);
    }

    function recoverFunds(uint256 id) external {
        Commitment storage c = commitments[id];
        require(!c.released, "Already released");
        require(block.timestamp >= c.challengeDeadline + RECOVERY_TIMEOUT, "Recovery timeout not reached");

        c.released = true;
        uint256 amount = c.bondAmount;

        emit FundsRecovered(id, msg.sender, amount);
        (bool success,) = msg.sender.call{value: amount}("");
        require(success, "Transfer failed");
    }

    function verifyAndRelease(
        uint256 commitmentId,
        uint256[2] memory a,
        uint256[2][2] memory b,
        uint256[2] memory c,
        uint256[4] memory publicInputs
    ) external {
        Commitment storage cm = commitments[commitmentId];
        require(cm.relayer == msg.sender, "Not the relayer");
        require(!cm.released, "Already released");

        bool valid = verifier.verifyProof(a, b, c, publicInputs);
        require(valid, "Invalid proof");

        cm.zkpVerified = true;
        cm.released = true;
        uint256 amount = cm.bondAmount;

        emit ZkpVerified(commitmentId, msg.sender);
        emit BondReleased(commitmentId, msg.sender, amount);
        (bool success,) = msg.sender.call{value: amount}("");
        require(success, "Transfer failed");
    }
}
