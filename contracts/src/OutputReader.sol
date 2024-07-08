// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.16;

library OutputReader {
    function readUint256(bytes memory _output, uint256 _offset) internal pure returns (uint256) {
        uint256 value;
        assembly {
            value := mload(add(add(_output, 0x20), _offset))
        }
        return value;
    }

    function readUint128(bytes memory _output, uint256 _offset) internal pure returns (uint128) {
        uint128 value;
        assembly {
            value := mload(add(add(_output, 0x10), _offset))
        }
        return value;
    }

    function readUint64(bytes memory _output, uint256 _offset) internal pure returns (uint64) {
        uint64 value;
        assembly {
            value := mload(add(add(_output, 0x08), _offset))
        }
        return value;
    }

    function readUint32(bytes memory _output, uint256 _offset) internal pure returns (uint32) {
        uint32 value;
        assembly {
            value := mload(add(add(_output, 0x04), _offset))
        }
        return value;
    }

    function readUint16(bytes memory _output, uint256 _offset) internal pure returns (uint16) {
        uint16 value;
        assembly {
            value := mload(add(add(_output, 0x02), _offset))
        }
        return value;
    }
}
