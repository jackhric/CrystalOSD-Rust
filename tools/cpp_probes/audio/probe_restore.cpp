#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <string>
#include <vector>

#include "Assets/Bytes.hpp"
#include "Audio/data/SndImage.hpp"
#include "Audio/spu2/Snapshot.hpp"
#include "Audio/spu2/Spu2.hpp"

using namespace audio;

struct Rng {
    uint64_t s;
    uint32_t next() {
        s = s * 6364136223846793005ull + 1442695040888963407ull;
        return uint32_t(s >> 32);
    }
    uint32_t below(uint32_t n) { return next() % n; }
};

struct Hash {
    uint64_t h = 0xcbf29ce484222325ull;
    void byte(uint8_t b) { h = (h ^ b) * 0x100000001b3ull; }
    void u64(uint64_t v) { for (int i = 0; i < 8; ++i) byte(uint8_t(v >> (8 * i))); }
    void u32(uint32_t v) { for (int i = 0; i < 4; ++i) byte(uint8_t(v >> (8 * i))); }
    void u16(uint16_t v) { byte(uint8_t(v)); byte(uint8_t(v >> 8)); }
    void i32(int32_t v) { u32(uint32_t(v)); }
    void bytes(assets::View b) { for (uint8_t x : b) byte(x); }
};

static void out(const char* name, uint64_t v) { std::printf("const %s: u64 = 0x%016llx;\n", name, (unsigned long long)v); }

static void hashVoice(Hash& h, const spu2::Voice& v) {
    for (const spu2::VolumeSweep* s : {&v.left, &v.right}) { h.u16(s->reg); h.i32(s->level); h.u32(s->counter); }
    h.byte(uint8_t(v.envelope.phase)); h.i32(v.envelope.level); h.u32(v.envelope.counter); h.u16(v.envelope.reg1); h.u16(v.envelope.reg2);
    h.u16(v.pitch); h.u32(v.startAddress); h.u32(v.loopAddress); h.u32(v.nextAddress); h.i32(v.prev1); h.i32(v.prev2);
    h.byte(v.modulated); h.byte(v.noise); h.byte(uint8_t(v.loopMode)); h.byte(uint8_t(v.loopFlags)); h.i32(v.phase); h.i32(v.outX);
    for (int32_t f : v.fifo) h.i32(f);
    h.u32(v.fifoWrite); h.u32(v.fifoRead);
    for (int16_t b : v.block) h.u16(uint16_t(b));
    h.byte(v.blockPending); h.i32(v.dryL); h.i32(v.dryR); h.i32(v.wetL); h.i32(v.wetR);
}

static void hashCore(Hash& h, const spu2::Core& k) {
    h.u32(k.index);
    for (const auto& v : k.voices) hashVoice(h, v);
    for (const spu2::CoreGates* g : {&k.dryGate, &k.wetGate}) { h.i32(g->inpL); h.i32(g->inpR); h.i32(g->sndL); h.i32(g->sndR); h.i32(g->extL); h.i32(g->extR); }
    for (const spu2::VolumeSweep* s : {&k.masterLeft, &k.masterRight}) { h.u16(s->reg); h.i32(s->level); h.u32(s->counter); }
    for (const spu2::StereoVolume* s : {&k.extVol, &k.inpVol, &k.fxVol}) { h.i32(s->left); h.i32(s->right); }
    h.u32(k.irqAddress); h.u32(k.transferAddress); h.u32(k.activeTransferAddress);
    h.byte(k.irqEnable); h.byte(k.fxEnable); h.byte(k.mute); h.byte(k.autoDmaActive); h.byte(uint8_t(k.dmaBits)); h.byte(k.noiseClock);
    h.u32(k.noiseCounter); h.u32(k.noiseOut); h.u16(k.autoDmaControl); h.byte(k.dmaMode); h.byte(k.attrBit0);
    for (uint32_t x : {k.pmon, k.non, k.vmixl, k.vmixr, k.vmixel, k.vmixer, k.endx}) h.u32(x);
    h.u16(k.mmix); h.u16(k.statx); h.u16(k.attr); h.u32(k.keyOn); h.u32(k.keyOff); h.u32(k.effectsStart); h.u32(k.effectsEnd); h.u32(k.reverbPosition);
    const spu2::ReverbRegs& r = k.reverb;
    for (int16_t x : {r.inCoefL, r.inCoefR, r.apf1Vol, r.apf2Vol, r.iirVol, r.wallVol, r.comb1Vol, r.comb2Vol, r.comb3Vol, r.comb4Vol}) h.u16(uint16_t(x));
    for (uint32_t x : {r.apf1Size, r.apf2Size, r.sameLSrc, r.sameRSrc, r.diffLSrc, r.diffRSrc, r.sameLDst, r.sameRDst, r.diffLDst, r.diffRDst, r.comb1LSrc, r.comb1RSrc,
                       r.comb2LSrc, r.comb2RSrc, r.comb3LSrc, r.comb3RSrc, r.comb4LSrc, r.comb4RSrc, r.apf1LDst, r.apf1RDst, r.apf2LDst, r.apf2RDst}) h.u32(x);
    for (const auto& ring : k.down) for (int16_t x : ring) h.u16(uint16_t(x));
    for (const auto& ring : k.up) for (int16_t x : ring) h.u16(uint16_t(x));
}

static void hashSpu(Hash& h, const spu2::Spu2& spu) {
    h.bytes(spu.ram());
    for (int c = 0; c < 2; ++c) hashCore(h, spu.core(c));
    for (uint32_t a = 0x1F900000; a < 0x1F910000; a += 1) h.u16(spu.read(a));
    h.u32(spu.ticks()); h.u16(uint16_t(spu.finalLeft())); h.u16(uint16_t(spu.finalRight()));
}


static std::string fmt(const char* f, uint32_t v) { char b[32]; std::snprintf(b, sizeof b, f, v); return b; }

static std::string num(Rng& r, uint32_t v, bool negative = false) {
    switch (r.below(negative ? 5 : 4)) {
    case 0: return fmt("%u", v);
    case 1: return "\"" + fmt("%x", v) + "\"";
    case 2: return "\"0x" + fmt("%X", v) + "\"";
    case 3: return "\" " + fmt("%x", v) + "\"";
    default: return fmt("%d", v);
    }
}
static std::string flag(Rng& r, bool b) {
    switch (r.below(3)) {
    case 0: return b ? "true" : "false";
    case 1: return b ? "1" : "0";
    default: return b ? "\"1\"" : "\"0\"";
    }
}
static uint32_t sgn(Rng& r, uint32_t span) { return uint32_t(int32_t(r.below(2 * span + 1)) - int32_t(span)); }
static std::string gate(Rng& r) { const uint32_t g = r.below(5) == 0 ? r.next() : (r.below(2) ? 0xFFFFFFFFu : 0u); return num(r, g, true); }
static std::string sweep(Rng& r) {
    std::string s = "{\"reg\":" + num(r, r.below(0x10000));
    s += ",\"value\":" + num(r, sgn(r, 0x7FFF), true);
    s += ",\"counter\":" + num(r, r.below(0x8000)) + "}";
    return s;
}
static std::string voice(Rng& r) {
    std::string s = "{\"volume\":{\"left\":" + sweep(r);
    s += ",\"right\":" + sweep(r) + "},\"adsr\":{\"reg1\":" + num(r, r.below(0x10000));
    s += ",\"reg2\":" + num(r, r.below(0x10000));
    s += ",\"value\":" + num(r, r.below(0x8000));
    s += ",\"phase\":" + num(r, r.below(5));
    s += ",\"counter\":" + num(r, r.below(0x8000)) + "}";
    s += ",\"pitch\":" + num(r, r.below(0x10000));
    s += ",\"startA\":" + num(r, r.next());
    s += ",\"loopStartA\":" + num(r, r.next());
    s += ",\"nextA\":" + num(r, r.next());
    s += ",\"prev1\":" + num(r, sgn(r, 32768), true);
    s += ",\"prev2\":" + num(r, sgn(r, 32768), true);
    s += ",\"modulated\":" + flag(r, r.below(2));
    s += ",\"noise\":" + flag(r, r.below(4) == 0);
    s += ",\"loopMode\":" + num(r, sgn(r, 128), true);
    s += ",\"loopFlags\":" + num(r, sgn(r, 128), true);
    s += ",\"sp\":" + num(r, r.below(0x4000));
    s += ",\"outX\":" + num(r, sgn(r, 32768), true);
    s += ",\"decPosRead\":" + num(r, r.next());
    s += ",\"decPosWrite\":" + num(r, r.next());
    s += ",\"gates\":{\"dryL\":" + gate(r);
    s += ",\"dryR\":" + gate(r);
    s += ",\"wetL\":" + gate(r);
    s += ",\"wetR\":" + gate(r) + "}}";
    return s;
}
static std::string pair(Rng& r) {
    std::string s = "[" + num(r, sgn(r, 0x7FFF), true);
    s += "," + num(r, sgn(r, 0x7FFF), true) + "]";
    return s;
}
static std::string gates(Rng& r) {
    std::string s = "{";
    const char* names[] = {"inpL", "inpR", "sndL", "sndR", "extL", "extR"};
    for (int i = 0; i < 6; ++i) s += std::string(i ? "," : "") + "\"" + names[i] + "\":" + gate(r);
    return s + "}";
}
static std::string core(Rng& r) {
    std::string s = "{\"tsa\":" + num(r, r.next());
    s += ",\"activeTsa\":" + num(r, r.next());
    s += ",\"irqa\":" + num(r, r.below(0x100000));
    s += ",\"irqEnable\":" + flag(r, r.below(2));
    s += ",\"fxEnable\":" + flag(r, r.below(4) != 0);
    s += ",\"mute\":" + flag(r, r.below(4) == 0);
    s += ",\"autoDmaCtrl\":" + num(r, r.below(4));
    s += ",\"admaInProgress\":" + flag(r, r.below(2));
    s += ",\"dmaBits\":" + num(r, r.below(8), true);
    s += ",\"noiseClk\":" + num(r, r.below(0x40));
    s += ",\"noiseCnt\":" + num(r, r.next());
    s += ",\"noiseOut\":" + num(r, r.next());
    s += ",\"keyOn\":" + num(r, r.below(0x1000000));
    s += ",\"keyOff\":" + num(r, r.below(0x1000000));
    s += ",\"regs\":{";
    const char* regs[] = {"pmon", "non", "vmixl", "vmixr", "vmixel", "vmixer", "endx"};
    for (int i = 0; i < 7; ++i) s += std::string(i ? "," : "") + "\"" + regs[i] + "\":" + num(r, r.below(0x1000000));
    s += ",\"mmix\":" + num(r, r.below(0x10000));
    s += ",\"statx\":" + num(r, r.below(0x10000));
    s += ",\"attr\":" + num(r, r.below(0x10000)) + "}";
    s += ",\"masterVol\":{\"left\":" + sweep(r);
    s += ",\"right\":" + sweep(r) + "}";
    s += ",\"extVol\":" + pair(r);
    s += ",\"inpVol\":" + pair(r);
    s += ",\"fxVol\":" + pair(r);
    s += ",\"dryGate\":" + gates(r);
    s += ",\"wetGate\":" + gates(r);
    const uint32_t start = r.below(0xF0000);
    s += ",\"effectsStartA\":" + num(r, start);
    s += ",\"effectsEndA\":" + num(r, r.below(4) == 0 ? start : start + 0x1000 + r.below(0xFFFFF - start - 0x1000));
    s += ",\"revbSampleBufPos\":" + num(r, r.next());
    s += ",\"revb\":{";
    const char* vols[] = {"inCoefL", "inCoefR", "apf1Vol", "apf2Vol", "iirVol", "wallVol", "comb1Vol", "comb2Vol", "comb3Vol", "comb4Vol"};
    for (int i = 0; i < 10; ++i) s += std::string(i ? "," : "") + "\"" + vols[i] + "\":" + num(r, sgn(r, 0x8000), true);
    const char* addrs[] = {"apf1Size", "apf2Size", "sameLSrc", "sameRSrc", "diffLSrc", "diffRSrc", "sameLDst", "sameRDst", "diffLDst", "diffRDst", "comb1LSrc",
                           "comb1RSrc", "comb2LSrc", "comb2RSrc", "comb3LSrc", "comb3RSrc", "comb4LSrc", "comb4RSrc", "apf1LDst", "apf1RDst", "apf2LDst", "apf2RDst"};
    for (const char* a : addrs) s += std::string(",\"") + a + "\":" + num(r, r.below(3) == 0 ? r.next() : r.below(0x8000));
    s += "},\"voices\":[\n";
    for (int v = 0; v < 24; ++v) s += (v ? ",\n" : "") + voice(r);
    return s + "\n]}";
}
static std::string state(Rng& r) {
    std::string s = "{\"ticks\":" + num(r, r.next());
    s += ", \"outPos\":" + num(r, r.below(0x10000));
    s += ", \"playMode\":" + num(r, r.below(8), true);
    s += ", \"extra\": [1.5, null, \"x\\n\", {\"a\": -2e3}],";
    s += " \"spdif\":{\"out\":" + num(r, r.below(0x10000));
    s += ",\"info\":" + num(r, r.below(0x10000));
    s += ",\"mode\":" + num(r, r.below(0x10000));
    s += ",\"media\":" + num(r, r.below(0x10000));
    s += ",\"protection\":" + num(r, r.below(0x10000)) + "},\n \"cores\":[" + core(r);
    s += "," + core(r) + "]}\n";
    return s;
}

int main(int argc, char** argv) {
    const std::filesystem::path resources = argv[1], dir = argv[2];
    std::filesystem::create_directories(dir);
    const assets::Bytes spuRam = data::SndImage::fromFolder(resources).spuRam();
    for (uint64_t seed = 31; seed < 35; ++seed) {
        Rng r{seed};
        const std::string text = state(r);
        Hash t;
        for (char c : text) t.byte(uint8_t(c));
        assets::Bytes regs(0x10000), history(8192);
        for (auto& b : regs) b = uint8_t(r.next());
        for (auto& b : history) b = uint8_t(r.next());
        const std::string name = "probe" + std::to_string(seed);
        assets::writeFile(dir / (name + ".start.spuram.bin"), spuRam);
        assets::writeFile(dir / (name + ".start.spuregs.bin"), regs);
        std::ofstream(dir / (name + ".start.spustate.json")) << text;
        spu2::Spu2 spu;
        spu.restore(spu2::Snapshot::load(dir, name, "start"));
        Hash h;
        hashSpu(h, spu);
        spu.restoreHistory(history);
        hashSpu(h, spu);
        std::vector<int16_t> pcm(2 * 3000);
        spu.render(pcm.data(), 3000);
        for (int16_t s : pcm) h.u16(uint16_t(s));
        hashSpu(h, spu);
        std::printf("    (%llu, 0x%016llx, 0x%016llx),\n", (unsigned long long)seed, (unsigned long long)t.h, (unsigned long long)h.h);
    }
    return 0;
}
