// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

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

    uint256 public nextCommitmentId;
    mapping(uint256 => Commitment) public commitments;

    event CommitmentPosted(uint256 indexed id, address indexed relayer, bytes32 commitmentHash, uint256 bondAmount);
    event BondReleased(uint256 indexed id, address indexed relayer, uint256 bondAmount);
    event ChallengeSubmitted(uint256 indexed id, address indexed challenger);
    event FundsRecovered(uint256 indexed id, address indexed recoverer, uint256 amount);

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
}
