# -*- coding: utf-8 -*-
"""Binary provenance payload shared by Host embedding and offline detection.

This module defines identification and integrity framing only.  It is not a
signature, DRM, an embedding algorithm, or a claim that a watermark cannot be
removed.
"""

from __future__ import annotations

import binascii
from dataclasses import dataclass
import hashlib
import re
import struct

from .errors import LomcError


MAGIC = b"LOMW"
PROTOCOL_VERSION = 1
PAYLOAD_SIZE = 28
MOD_ID_HASH_SIZE = 16
_DOMAIN = b"lom_modkit:watermark:mod-id:v1\x00"
_MOD_ID_RE = re.compile(r"^[a-z0-9_-]{1,64}$")
_PACKET = struct.Struct(">4sBBBB16sI")


@dataclass(frozen=True)
class WatermarkPacket:
    protocol_version: int
    algorithm_version: int
    flags: int
    mod_id_hash: bytes
    checksum: int
    checksum_valid: bool

    @property
    def mod_id_hash_hex(self) -> str:
        return self.mod_id_hash.hex().upper()


def mod_id_hash(mod_id: str) -> bytes:
    if not isinstance(mod_id, str) or _MOD_ID_RE.fullmatch(mod_id) is None:
        raise LomcError("水印 mod_id 必须匹配 [a-z0-9_-]{1,64}")
    return hashlib.sha256(_DOMAIN + mod_id.encode("ascii")).digest()[:MOD_ID_HASH_SIZE]


def encode_packet(mod_id: str, algorithm_version: int = 1) -> bytes:
    if (
        isinstance(algorithm_version, bool)
        or not isinstance(algorithm_version, int)
        or not 1 <= algorithm_version <= 255
    ):
        raise LomcError("水印 algorithm_version 必须是 1~255 的整数")
    prefix = _PACKET.pack(
        MAGIC,
        PROTOCOL_VERSION,
        algorithm_version,
        0,  # flags: protocol v1 reserves all bits
        0,  # reserved byte
        mod_id_hash(mod_id),
        0,
    )[:-4]
    checksum = binascii.crc32(prefix) & 0xFFFFFFFF
    return prefix + checksum.to_bytes(4, "big")


def parse_packet(payload: bytes) -> WatermarkPacket:
    if not isinstance(payload, (bytes, bytearray)) or len(payload) != PAYLOAD_SIZE:
        raise LomcError(f"水印 payload 必须恰好是 {PAYLOAD_SIZE} 字节")
    magic, protocol, algorithm, flags, reserved, identity, checksum = _PACKET.unpack(
        bytes(payload)
    )
    if magic != MAGIC:
        raise LomcError("水印 magic 不匹配")
    if protocol != PROTOCOL_VERSION:
        raise LomcError(f"不支持的水印协议版本：{protocol}")
    if algorithm == 0:
        raise LomcError("水印 algorithm_version 不能为 0")
    if flags != 0 or reserved != 0:
        raise LomcError("水印协议 v1 的 flags/reserved 必须为 0")
    actual = binascii.crc32(bytes(payload)[:-4]) & 0xFFFFFFFF
    return WatermarkPacket(
        protocol, algorithm, flags, identity, checksum, checksum == actual
    )


def decode_packet(payload: bytes) -> WatermarkPacket:
    packet = parse_packet(payload)
    if not packet.checksum_valid:
        raise LomcError("水印 payload CRC-32 校验失败")
    return packet


def packet_to_bits(payload: bytes) -> tuple[int, ...]:
    """Return the protocol-defined MSB-first bit order used by embedders."""
    if len(payload) != PAYLOAD_SIZE:
        raise LomcError(f"水印 payload 必须恰好是 {PAYLOAD_SIZE} 字节")
    return tuple((byte >> shift) & 1 for byte in payload for shift in range(7, -1, -1))


def bits_to_packet(bits) -> bytes:
    values = tuple(bits)
    if len(values) != PAYLOAD_SIZE * 8 or any(bit not in (0, 1) for bit in values):
        raise LomcError(f"水印 bit 序列必须恰好是 {PAYLOAD_SIZE * 8} 个 0/1")
    output = bytearray(PAYLOAD_SIZE)
    for index, bit in enumerate(values):
        output[index // 8] |= bit << (7 - index % 8)
    return bytes(output)
