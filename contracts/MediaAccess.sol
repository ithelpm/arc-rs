// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title MediaAccess — Soulbound content access registry
/// @notice Access grants are permanent and non-transferable (soulbound).
///         Only the contract owner can grant access; grants cannot be revoked.
contract MediaAccess {
    address public immutable owner;

    // wallet → contentId → has access
    mapping(address => mapping(bytes32 => bool)) private _access;

    event AccessGranted(address indexed wallet, bytes32 indexed contentId);

    error Unauthorized();

    constructor() {
        owner = msg.sender;
    }

    modifier onlyOwner() {
        if (msg.sender != owner) revert Unauthorized();
        _;
    }

    /// @notice Grant permanent soulbound access to a content item.
    /// @param wallet  The recipient's address.
    /// @param contentId  A bytes32 content identifier (e.g. keccak256 of the item ID).
    function grantAccess(address wallet, bytes32 contentId) external onlyOwner {
        _access[wallet][contentId] = true;
        emit AccessGranted(wallet, contentId);
    }

    /// @notice Check whether a wallet has access to a content item.
    function hasAccess(address wallet, bytes32 contentId) external view returns (bool) {
        return _access[wallet][contentId];
    }
}
