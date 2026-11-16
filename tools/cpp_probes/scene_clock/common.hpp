#pragma once
#include <bit>
#include <cstdint>
#include <cstdio>
#include <string>
#include <vector>

struct Rng {
    uint64_t s;
    explicit Rng(uint64_t seed) : s(seed) {}
    uint64_t next() {
        s += 0x9e3779b97f4a7c15ull;
        uint64_t z = s;
        z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ull;
        z = (z ^ (z >> 27)) * 0x94d049bb133111ebull;
        return z ^ (z >> 31);
    }
    uint32_t next32() { return static_cast<uint32_t>(next() >> 32); }
};

struct Hash {
    uint64_t h = 0xcbf29ce484222325ull;
    uint64_t count = 0;
    void byte(uint8_t b) { h = (h ^ b) * 0x100000001b3ull; }
    void u32(uint32_t v) { for (int i = 0; i < 4; ++i) byte(static_cast<uint8_t>(v >> (8 * i))); ++count; }
    void u64(uint64_t v) { for (int i = 0; i < 8; ++i) byte(static_cast<uint8_t>(v >> (8 * i))); ++count; }
    void i32(int32_t v) { u32(static_cast<uint32_t>(v)); }
    void f32(float v) { u32(std::bit_cast<uint32_t>(v)); }
    void f64(double v) { u64(std::bit_cast<uint64_t>(v)); }
    void boolean(bool v) { u32(v ? 1u : 0u); }
};

inline void report(const std::string& name, const Hash& h) { std::printf("%s %llu 0x%016llx\n", name.c_str(), (unsigned long long)h.count, (unsigned long long)h.h); }

inline float asF(uint32_t b) { return std::bit_cast<float>(b); }
inline uint32_t bitsF(float f) { return std::bit_cast<uint32_t>(f); }

inline const std::vector<uint32_t>& edgeFloats() {
    static const std::vector<uint32_t> e = {
        0x00000000, 0x80000000, 0x00000001, 0x80000001, 0x007fffff, 0x807fffff, 0x00400000, 0x00800000, 0x80800000, 0x00800001, 0x00ffffff,
        0x3f800000, 0xbf800000, 0x3f800001, 0x3f7fffff, 0xbf7fffff, 0x7f7fffff, 0xff7fffff, 0x7f7ffffe, 0x7f000000, 0x7f800000, 0xff800000,
        0x7fc00000, 0xffc00000, 0x7f800001, 0x7fa00000, 0xffffffff, 0x40000000, 0x3f000000, 0x34000000, 0x33800000, 0x33000000, 0x4b800000,
        0x4b7fffff, 0x4f000000, 0x4effffff, 0xcf000000, 0xcf000001, 0x4f800000, 0x4f7fffff, 0x3fc90fdb, 0x40490fdb, 0x3f490fd8, 0x3f490fd9,
        0x4016cbe3, 0x4016cbe4, 0x43490f80, 0x43490f81, 0x3fc90fd0, 0x31ffffff, 0x32000000, 0x3e999999, 0x3e99999a, 0x3f480000, 0x3f480001,
        0x0c000000, 0x1f800000, 0x5f800000, 0x447a0000, 0x42700000, 0x47800000, 0xc7000000, 0x46fffe00, 0x3dcccccd, 0xbdcccccd, 0x3f666666,
        0x3f7d70a4, 0x01000000, 0x00800002, 0x7e800000, 0xfe800000};
    return e;
}

// Pairs: 0 random bits, 1 exponents within 64 of each other, 2 all edge pairs.
inline std::vector<std::pair<uint32_t, uint32_t>> pairSet(int kind, uint32_t n) {
    std::vector<std::pair<uint32_t, uint32_t>> out;
    if (kind == 2) {
        for (uint32_t a : edgeFloats())
            for (uint32_t b : edgeFloats()) out.push_back({a, b});
        return out;
    }
    Rng r(0x5eed0000ull + static_cast<uint64_t>(kind));
    out.reserve(n);
    for (uint32_t i = 0; i < n; ++i) {
        if (kind == 0) {
            const uint32_t a = r.next32();
            out.push_back({a, r.next32()});
        } else {
            const uint64_t x = r.next();
            const uint32_t a = static_cast<uint32_t>(x), raw = static_cast<uint32_t>(x >> 32);
            int32_t eb = static_cast<int32_t>((a >> 23) & 0xff) + static_cast<int32_t>(r.next() % 129) - 64;
            eb = eb < 0 ? 0 : eb > 255 ? 255 : eb;
            out.push_back({a, (raw & 0x807fffffu) | (static_cast<uint32_t>(eb) << 23)});
        }
    }
    return out;
}

// Singles: 0 random bits, 1 every exponent with 64 random mantissas of each sign, 2 the edges.
inline std::vector<uint32_t> singleSet(int kind, uint32_t n) {
    std::vector<uint32_t> out;
    if (kind == 2) return edgeFloats();
    Rng r(0x51e60000ull + static_cast<uint64_t>(kind));
    if (kind == 0) {
        for (uint32_t i = 0; i < n; ++i) out.push_back(r.next32());
        return out;
    }
    for (uint32_t e = 0; e < 256; ++e)
        for (uint32_t k = 0; k < 64; ++k) {
            const uint32_t m = r.next32() & 0x7fffffu;
            out.push_back((e << 23) | m);
            out.push_back(0x80000000u | (e << 23) | m);
        }
    return out;
}

// Doubles for cut: 0 random bits, 1 singles perturbed in the low 30 bits of the double, 2 edges.
inline std::vector<uint64_t> doubleSet(int kind, uint32_t n) {
    std::vector<uint64_t> out;
    if (kind == 2) {
        out = {0x0000000000000000ull, 0x8000000000000000ull, 0x0000000000000001ull, 0x000fffffffffffffull, 0x0010000000000000ull,
               0x47efffffe0000000ull, 0x47efffffe0000001ull, 0x47efffffdfffffffull, 0x47efffffefffffffull, 0x47effffff0000000ull,
               0x47effffff0000001ull, 0x47f0000000000000ull, 0xc7efffffe0000000ull, 0xc7effffff0000000ull, 0xc7f0000000000000ull,
               0x7ff0000000000000ull, 0xfff0000000000000ull, 0x7ff8000000000000ull, 0xfff8000000000000ull, 0x7ff0000000000001ull,
               0x3810000000000000ull, 0x380fffffffffffffull, 0xb80fffffffffffffull, 0x3810000000000001ull, 0x36a0000000000000ull,
               0x3ff0000000000000ull, 0x3fefffffffffffffull, 0x3ff0000000000001ull, 0x41dfffffffc00000ull, 0x41e0000000000000ull};
        return out;
    }
    Rng r(0xd0b1e000ull + static_cast<uint64_t>(kind));
    for (uint32_t i = 0; i < n; ++i) {
        if (kind == 0) {
            out.push_back(r.next());
        } else {
            const double d = static_cast<double>(asF(r.next32()));
            const int64_t delta = static_cast<int64_t>(r.next() % (1ull << 30)) - (1ll << 29);
            out.push_back(std::bit_cast<uint64_t>(d) + static_cast<uint64_t>(delta));
        }
    }
    return out;
}

inline std::vector<int32_t> intSet(uint32_t n) {
    std::vector<int32_t> out = {0, 1, -1, 16777216, 16777217, -16777217, 2147483647, -2147483647 - 1, 0x7fffff80, 0x7fffffc0, 0x7fffffbf,
                                33554431, 33554433, -33554435};
    Rng r(0x1e7e9e40ull);
    for (uint32_t i = 0; i < n; ++i) out.push_back(static_cast<int32_t>(r.next32()));
    return out;
}

// Trig inputs: 0 every 211th single from 0 to 0x43500000 of both signs and every 13499th above; 1 a linear sweep of
// -210..210; for sineCosine 2 every 101st single from 0 to 0x40a00000 of both signs, 3 random bits.
inline std::vector<uint32_t> trigSet(int kind, uint32_t n) {
    std::vector<uint32_t> out;
    if (kind == 0) {
        for (uint32_t b = 0; b <= 0x43500000u; b += 211) {
            out.push_back(b);
            out.push_back(b | 0x80000000u);
        }
        for (uint32_t b = 0x43500000u; b < 0x7fc00000u; b += 13499) {
            out.push_back(b);
            out.push_back(b | 0x80000000u);
        }
        for (uint32_t b : edgeFloats()) out.push_back(b);
    } else if (kind == 1) {
        for (uint32_t i = 0; i < n; ++i) out.push_back(bitsF(static_cast<float>(-210.0 + i * (420.0 / n))));
    } else if (kind == 2) {
        for (uint32_t b = 0; b <= 0x40a00000u; b += 101) {
            out.push_back(b);
            out.push_back(b | 0x80000000u);
        }
        for (uint32_t b : edgeFloats()) out.push_back(b);
    } else {
        Rng r(0x7819e000ull);
        for (uint32_t i = 0; i < n; ++i) out.push_back(r.next32());
    }
    return out;
}
