#!/usr/bin/env python3
"""Derive and verify the PCLMULQDQ folding constants for reflected CRC-32 (PNG / zlib).

The model below mirrors the x86 intrinsics bit-for-bit (128-bit registers as
little-endian integers, _mm_clmulepi64_si128 with imm 0x00/0x11/0x10/0x01,
byte shifts), so the C code in src/crc32_clmul.h is a direct transcription.
Run it to regenerate the constants and self-check against zlib.crc32.
"""
import os, sys, zlib

P = 0x104C11DB7          # CRC-32 polynomial (normal form, degree 32)
M64 = (1 << 64) - 1
M128 = (1 << 128) - 1

def clmul(a, b):
    r = 0
    while b:
        if b & 1:
            r ^= a
        a <<= 1
        b >>= 1
    return r

def pmod(a, m):
    """a mod m over GF(2)."""
    dm = m.bit_length() - 1
    while a.bit_length() - 1 >= dm:
        a ^= m << (a.bit_length() - 1 - dm)
    return a

def xpow_mod(k):
    return pmod(1 << k, P)

def reflect(v, bits):
    r = 0
    for i in range(bits):
        if v >> i & 1:
            r |= 1 << (bits - 1 - i)
    return r

def K(k):
    """Folding constant that advances a reflected register's qword by x^k:
    (x^(k-32) mod P) bit-reflected over 32 bits, shifted left by one."""
    return reflect(xpow_mod(k - 32), 32) << 1

# --- intrinsic model ---------------------------------------------------------
def clmulepi64(a, b, imm):
    x = (a >> 64) if imm & 1 else (a & M64)
    y = (b >> 64) if imm & 0x10 else (b & M64)
    return clmul(x, y)

def srli_si128(a, n): return a >> (8 * n)
def slli_si128(a, n): return (a << (8 * n)) & M128

def le128(bs): return int.from_bytes(bs, 'little')

def fold(x, Kreg):
    """x·x^(16·N) folded into 128 bits using constant pair Kreg = lo | hi<<64."""
    return clmulepi64(x, Kreg, 0x00) ^ clmulepi64(x, Kreg, 0x11)

# constants for folding distances (in bits): 512 (4 blocks) and 128 (1 block)
K512 = K(512 + 64) | (K(512) << 64)
K128 = K(128 + 64) | (K(128) << 64)
# final reduction constants
K96 = K(96)                                   # multiplies a qword by x^96 (folds 32 bits down)

def poly_div(a, b):
    """quotient of a / b over GF(2)."""
    q = 0
    db = b.bit_length() - 1
    while a.bit_length() - 1 >= db:
        s = a.bit_length() - 1 - db
        q |= 1 << s
        a ^= b << s
    return q

MU  = reflect(poly_div(1 << 64, P), 33)       # floor(x^64 / P), 33 bits, bit-reflected
PR  = reflect(P, 33)                          # P itself, 33 bits, bit-reflected
M32 = 0xFFFFFFFF
HIGHQ = M64 << 64

def final_reduce(acc):
    """128-bit accumulator (reflected frame) -> 32-bit CRC, exactly as src/crc32_clmul.h does it.
    Frame: register bit i <-> x^(127-i). Facts used (verified empirically above):
      clmul(Q, K(k)) == q64 * (x^(k-32) mod P) * x^32   (q64 = polynomial of the qword, bit i <-> x^(63-i))
    Step 1: append 32 zero bits (CRC = A*x^32 mod P):      t  = l*x^96 + h*x^32           (bits 1..95)
    Step 2: fold the top 32 degrees back in:               t2 = V*x^32, deg V <= 63     (bits 32..95)
    Step 3: fold once more, shifting the product so it is NOT multiplied by x^32:
                                                           z  = Z, deg Z <= 63          (bits 64..127)
    Step 4: Barrett: q = floor(z_hi * mu / x^32); r = (Z + q*P) mod x^32."""
    t  = clmulepi64(acc, K96, 0x00) ^ ((acc >> 64) << 32)
    t2 = clmulepi64(slli_si128(t, 4), K96, 0x00) ^ (t & ~M32)
    z  = slli_si128(clmulepi64(t2, K96, 0x00), 4) ^ (t2 & HIGHQ)
    q  = clmulepi64(srli_si128(z, 8) & M32, MU, 0x00) & M32
    r  = ((clmulepi64(q, PR, 0x00) ^ srli_si128(z, 8)) >> 32) & M32
    return r ^ M32

def crc32_fold_model(data, crc=0):
    n = len(data)
    assert n >= 64 and n % 16 == 0
    crc ^= M32
    x = [le128(data[i*16:(i+1)*16]) for i in range(4)]
    x[0] ^= crc
    pos = 64
    while n - pos >= 64:
        for i in range(4):
            x[i] = fold(x[i], K512) ^ le128(data[pos + i*16: pos + (i+1)*16])
        pos += 64
    acc = x[0]
    for i in range(1, 4):
        acc = fold(acc, K128) ^ x[i]
    while n - pos >= 16:
        acc = fold(acc, K128) ^ le128(data[pos:pos+16])
        pos += 16
    return final_reduce(acc)

if __name__ == '__main__':
    import random
    ok = True
    for trial in range(200):
        n = random.choice([64, 80, 96, 128, 256, 1024, 4096])
        d = os.urandom(n)
        init = random.getrandbits(32)
        got = crc32_fold_model(d, init)
        want = zlib.crc32(d, init)
        if got != want:
            ok = False
            print(f"MISMATCH n={n} init={init:08x} got={got:08x} want={want:08x}")
            break
    print("model ok" if ok else "model BROKEN")
    print(f"K512 lo=0x{K512 & M64:x} hi=0x{K512 >> 64:x}")
    print(f"K128 lo=0x{K128 & M64:x} hi=0x{K128 >> 64:x}")
    print(f"K96 =0x{K96:x}")
    print(f"MU  =0x{MU:x}  PR =0x{PR:x}")
    print("(K96 is also used for the two 32-bit fold-down steps)")
