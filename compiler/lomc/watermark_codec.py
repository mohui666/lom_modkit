# -*- coding: utf-8 -*-
"""Algorithm v1 channel coding and keyed spread-spectrum carrier layout.

The public algorithm key makes encoder and offline detector deterministic.  It
is domain separation, not a secret and not an anti-removal claim.
"""

from __future__ import annotations

import hashlib

from .errors import LomcError
from .watermark_protocol import PAYLOAD_SIZE


ALGORITHM_VERSION = 1
DATA_BITS = PAYLOAD_SIZE * 8
ECC_BITS = DATA_BITS // 4 * 7
GRID_COLUMNS = 28
GRID_ROWS = 14
CELL_SIZE = 16
TILE_WIDTH = GRID_COLUMNS * CELL_SIZE
TILE_HEIGHT = GRID_ROWS * CELL_SIZE
OVERLAY_ALPHA = 4
_KEY = b"lom_modkit:watermark:carrier-prng:algorithm:1"


class _XorShift32:
    def __init__(self, seed: int):
        self.state = seed & 0xFFFFFFFF or 0x6D2B79F5

    def next(self) -> int:
        value = self.state
        value ^= (value << 13) & 0xFFFFFFFF
        value ^= value >> 17
        value ^= (value << 5) & 0xFFFFFFFF
        self.state = value & 0xFFFFFFFF
        return self.state


def _seed() -> int:
    return int.from_bytes(hashlib.sha256(_KEY).digest()[:4], "big")


def hamming_encode(payload: bytes) -> tuple[int, ...]:
    """Encode 224 payload bits as 56 independent Hamming(7,4) words."""
    if not isinstance(payload, (bytes, bytearray)) or len(payload) != PAYLOAD_SIZE:
        raise LomcError(f"水印 payload 必须恰好是 {PAYLOAD_SIZE} 字节")
    bits = tuple((byte >> shift) & 1 for byte in payload for shift in range(7, -1, -1))
    encoded = []
    for offset in range(0, len(bits), 4):
        d1, d2, d3, d4 = bits[offset : offset + 4]
        p1 = d1 ^ d2 ^ d4
        p2 = d1 ^ d3 ^ d4
        p4 = d2 ^ d3 ^ d4
        encoded.extend((p1, p2, d1, p4, d2, d3, d4))
    return tuple(encoded)


def hamming_decode(encoded) -> tuple[bytes, int]:
    """Decode and correct one flipped bit per Hamming word."""
    values = list(encoded)
    if len(values) != ECC_BITS or any(bit not in (0, 1) for bit in values):
        raise LomcError(f"ECC 序列必须是 {ECC_BITS} 个 0/1")
    decoded = []
    corrections = 0
    for offset in range(0, len(values), 7):
        word = values[offset : offset + 7]
        syndrome = (
            (word[0] ^ word[2] ^ word[4] ^ word[6])
            | ((word[1] ^ word[2] ^ word[5] ^ word[6]) << 1)
            | ((word[3] ^ word[4] ^ word[5] ^ word[6]) << 2)
        )
        if syndrome:
            word[syndrome - 1] ^= 1
            corrections += 1
        decoded.extend((word[2], word[4], word[5], word[6]))
    output = bytearray(PAYLOAD_SIZE)
    for index, bit in enumerate(decoded):
        output[index // 8] |= bit << (7 - index % 8)
    return bytes(output), corrections


def carrier_layout() -> tuple[tuple[int, ...], tuple[int, ...]]:
    """Return encoded-bit→cell permutation and per-bit public PN polarity."""
    if GRID_COLUMNS * GRID_ROWS != ECC_BITS:
        raise AssertionError("carrier grid must contain exactly one cell per ECC bit")
    random = _XorShift32(_seed())
    cells = list(range(ECC_BITS))
    for index in range(ECC_BITS - 1, 0, -1):
        other = random.next() % (index + 1)
        cells[index], cells[other] = cells[other], cells[index]
    polarity = tuple(1 if random.next() & 1 else -1 for _ in range(ECC_BITS))
    return tuple(cells), polarity


def carrier_signs(payload: bytes) -> tuple[int, ...]:
    """Return one bipolar sign per spatial cell in row-major order."""
    encoded = hamming_encode(payload)
    cells, polarity = carrier_layout()
    signs = [0] * ECC_BITS
    for bit_index, bit in enumerate(encoded):
        signs[cells[bit_index]] = (1 if bit else -1) * polarity[bit_index]
    return tuple(signs)


def recover_ecc_bits(cell_signs) -> tuple[int, ...]:
    """Inverse carrier mapping for already-correlated bipolar cell decisions."""
    values = tuple(cell_signs)
    if len(values) != ECC_BITS or any(value not in (-1, 1) for value in values):
        raise LomcError(f"载波判决必须是 {ECC_BITS} 个 -1/+1")
    cells, polarity = carrier_layout()
    return tuple(
        1 if values[cells[index]] * polarity[index] > 0 else 0
        for index in range(ECC_BITS)
    )


def tile_rgba(payload: bytes, alpha: int = OVERLAY_ALPHA) -> bytes:
    """Build the repeated 448×224 RGBA carrier tile used by Host algorithm v1."""
    if isinstance(alpha, bool) or not isinstance(alpha, int) or not 1 <= alpha <= 16:
        raise LomcError("水印 overlay alpha 必须是 1~16")
    signs = carrier_signs(payload)
    pixels = bytearray(TILE_WIDTH * TILE_HEIGHT * 4)
    for y in range(TILE_HEIGHT):
        cell_y, local_y = divmod(y, CELL_SIZE)
        for x in range(TILE_WIDTH):
            cell_x, local_x = divmod(x, CELL_SIZE)
            sign = signs[cell_y * GRID_COLUMNS + cell_x]
            checker = 1 if ((local_x // 2 + local_y // 2) & 1) == 0 else -1
            value = 255 if sign * checker > 0 else 0
            offset = (y * TILE_WIDTH + x) * 4
            pixels[offset : offset + 4] = bytes((value, value, value, alpha))
    return bytes(pixels)
