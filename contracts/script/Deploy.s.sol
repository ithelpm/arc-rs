// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script, console} from "forge-std/Script.sol";
import {MediaAccess} from "../MediaAccess.sol";

contract Deploy is Script {
    function run() external {
        vm.startBroadcast();

        MediaAccess access = new MediaAccess();

        console.log("MediaAccess deployed to:", address(access));
        console.log("Owner:", access.owner());

        vm.stopBroadcast();
    }
}
