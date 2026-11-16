#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <string>
#include <vector>

#include "Assets/Bytes.hpp"
#include "Audio/EeSoundQueue.hpp"
#include "Audio/data/HdBank.hpp"
#include "Audio/data/SndImage.hpp"
#include "Audio/data/SqSequence.hpp"
#include "Audio/driver/IopClock.hpp"
#include "Audio/driver/SnapshotBuilder.hpp"
#include "Audio/spu2/Adpcm.hpp"
#include "Audio/spu2/Envelope.hpp"
#include "Audio/spu2/Gauss.hpp"
#include "Audio/spu2/Reverb.hpp"
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

int main(int argc, char** argv) {
    const std::filesystem::path resources = argv[1];
    const data::SndImage image = data::SndImage::fromFolder(resources);
    const assets::Bytes ram = image.spuRam();

    {
        Hash h;
        spu2::AdpcmState state;
        for (size_t at = 0; at + 16 <= ram.size(); at += 16) {
            const spu2::AdpcmBlock b = spu2::decodeBlock(&ram[at], state);
            for (int16_t s : b.pcm) h.u16(uint16_t(s));
            h.byte(b.loopStart); h.byte(b.loopEnd); h.byte(b.loopRepeat); h.i32(state.prev1); h.i32(state.prev2);
        }
        Rng rng{1};
        for (int n = 0; n < 20000; ++n) {
            uint8_t block[16];
            for (uint8_t& x : block) x = uint8_t(rng.next());
            spu2::AdpcmState s{int32_t(rng.below(65536)) - 32768, int32_t(rng.below(65536)) - 32768};
            const spu2::AdpcmBlock b = spu2::decodeBlock(block, s);
            for (int16_t x : b.pcm) h.u16(uint16_t(x));
            h.byte(b.loopStart); h.byte(b.loopEnd); h.byte(b.loopRepeat); h.i32(s.prev1); h.i32(s.prev2);
        }
        out("ADPCM", h.h);
    }
    {
        Hash h;
        for (uint32_t i = 0; i < 1024; ++i) h.u16(uint16_t(spu2::gaussTap(i)));
        Rng rng{2};
        for (int n = 0; n < 300; ++n) {
            std::array<int16_t, 4> s;
            for (int16_t& x : s) x = int16_t(rng.next());
            for (uint32_t c = 0; c < 0x1000; c += 1) h.i32(spu2::interpolate(s, c | (rng.next() & 0xFFFF0000u)));
        }
        out("GAUSS", h.h);
    }
    {
        Hash h;
        Rng rng{3};
        for (int n = 0; n < 1500; ++n) {
            spu2::Envelope e;
            const uint16_t r1 = uint16_t(rng.next()), r2 = uint16_t(rng.next());
            if (n % 3 == 0) e.keyOn();
            else {
                e.phase = spu2::Phase(rng.below(5));
                e.level = int32_t(rng.below(0x8000));
                e.counter = rng.below(0x8000);
            }
            for (int k = 0; k < 2500; ++k) {
                if (k == 1200) e.keyOff();
                h.byte(e.step(r1, r2));
                h.byte(uint8_t(e.phase)); h.i32(e.level); h.u32(e.counter);
            }
        }
        for (int n = 0; n < 1500; ++n) {
            spu2::VolumeSweep s;
            s.write(uint16_t(rng.next()));
            if (rng.below(2)) s.level = int32_t(rng.below(0x10000)) - 0x8000;
            for (int k = 0; k < 2000; ++k) {
                if (k == 1000 && rng.below(2)) s.write(uint16_t(rng.next()));
                s.step();
                h.u16(s.reg); h.i32(s.level); h.u32(s.counter);
            }
        }
        out("ENVELOPE", h.h);
    }
    {
        Hash h;
        Rng rng{4};
        for (int round = 0; round < 8; ++round) {
            assets::Bytes ram2(spu2::kRamBytes);
            for (auto& b : ram2) b = uint8_t(rng.next());
            spu2::Core core;
            core.fxEnable = rng.below(4) != 0;
            const uint32_t start = rng.below(0xF0000);
            core.effectsStart = start;
            core.effectsEnd = round == 7 ? start : start + 0x100 + rng.below(0xFFFFF - start - 0x100);
            spu2::ReverbRegs& r = core.reverb;
            for (int16_t* v : {&r.inCoefL, &r.inCoefR, &r.apf1Vol, &r.apf2Vol, &r.iirVol, &r.wallVol, &r.comb1Vol, &r.comb2Vol, &r.comb3Vol, &r.comb4Vol}) *v = int16_t(rng.next());
            if (round == 3) r.iirVol = -0x8000;
            for (uint32_t* a : {&r.apf1Size, &r.apf2Size, &r.sameLSrc, &r.sameRSrc, &r.diffLSrc, &r.diffRSrc, &r.sameLDst, &r.sameRDst, &r.diffLDst, &r.diffRDst, &r.comb1LSrc,
                                &r.comb1RSrc, &r.comb2LSrc, &r.comb2RSrc, &r.comb3LSrc, &r.comb3RSrc, &r.comb4LSrc, &r.comb4RSrc, &r.apf1LDst, &r.apf1RDst, &r.apf2LDst, &r.apf2RDst})
                *a = round == 5 ? rng.next() : rng.below(0x4000);
            for (uint32_t cycles = 0; cycles < 6000; ++cycles) {
                const spu2::Stereo in{int32_t(rng.next() % 140000) - 70000, int32_t(rng.next() % 140000) - 70000};
                const spu2::Stereo o = spu2::runReverb(core, ram2, cycles + rng.below(3), in, spu2::ReverbPath::Avx);
                h.i32(o.left); h.i32(o.right); h.u32(core.reverbPosition);
            }
            h.bytes(ram2);
            hashCore(h, core);
        }
        for (int n = 0; n < 2000; ++n) {
            std::array<int16_t, 128> hist;
            for (int16_t& x : hist) x = int16_t(rng.next());
            const uint32_t pos = rng.below(64);
            h.u16(uint16_t(spu2::reverbFilter(hist, pos, false, spu2::ReverbPath::Avx)));
            h.u16(uint16_t(spu2::reverbFilter(hist, pos, true, spu2::ReverbPath::Sse)));
        }
        out("REVERB", h.h);
    }
    {
        Hash h;
        Rng rng{5};
        for (uint32_t a = 0x1F900000; a < 0x1F910000; ++a) {
            const spu2::Where w = spu2::decode(a);
            h.byte(w.core); h.byte(uint8_t(w.kind)); h.byte(w.voice); h.u16(w.reg); h.byte(spu2::isStatus(a));
        }
        int refused = 0;
        for (uint32_t a : {0x1F8FFFFEu, 0x1F910000u, 0u, 0xFFFFFFFFu}) {
            try { spu2::decode(a); } catch (const std::out_of_range&) { ++refused; }
        }
        h.u32(uint32_t(refused));
        spu2::Spu2 spu;
        spu.loadRam(ram);
        for (int n = 0; n < 300000; ++n) {
            uint32_t a;
            const uint32_t pick = rng.below(10);
            if (pick < 7) a = 0x1F900000 + rng.below(0x800);
            else if (pick < 9) a = 0x1F900000 + (rng.below(0x800) & ~1u);
            else a = 0x1F900000 + rng.below(0x10000);
            uint16_t v = uint16_t(rng.next());
            if (rng.below(8) == 0) v &= 0x00FF;
            spu.write(a, v);
            h.u16(spu.read(a));
            if (n % 50 == 0) {
                const spu2::StageSample s = spu.mix();
                for (int32_t f : s.field) h.i32(f);
            }
            if (n % 20011 == 0) {
                const uint16_t words[3] = {uint16_t(rng.next()), uint16_t(rng.next()), uint16_t(rng.next())};
                const uint32_t to = rng.next();
                spu.upload(to, words, 3);
                const uint32_t from = rng.below(0x200100);
                const uint32_t count = rng.below(300);
                spu.clearRam(from, count);
            }
        }
        spu.setReverbPath(spu2::ReverbPath::Sse);
        std::vector<int16_t> pcm(2 * 4000);
        spu.render(pcm.data(), 4000);
        for (int16_t s : pcm) h.u16(uint16_t(s));
        spu.advance(1000);
        hashSpu(h, spu);
        out("SPU2_REGISTERS", h.h);
    }
    {
        Hash h;
        for (std::string_view name : data::kSoundMembers) { h.u64(image.member(name).size()); h.bytes(image.member(name)); }
        h.bytes(ram);
        out("SND_IMAGE", h.h);
        Hash b;
        for (const char* name : {"SNDBOOTH", "SNDOSDDH"}) {
            const data::HdBank bank = data::HdBank::parse(image.member(name));
            b.u64(bank.programs.size());
            for (const auto& p : bank.programs) {
                b.byte(p.id); b.byte(p.head); b.byte(p.volume); b.byte(p.pan); b.u32(p.offset); b.u64(p.tones.size());
                for (const auto& t : p.tones) { b.byte(t.keyLow); b.byte(t.keyHigh); b.byte(t.rootKey); b.byte(uint8_t(t.fineTune)); b.u16(t.sampleStart); b.u16(t.adsr1); b.u16(t.adsr2); b.byte(t.pan); b.byte(t.bendRange); b.byte(t.flags); }
            }
            b.bytes(bank.velocity);
            for (const auto& e : bank.effects) { for (uint16_t x : {e.left, e.right, e.pitch, e.flagsA, e.adsr1, e.adsr2, e.word12, e.word14}) b.u16(x); b.u32(e.bdOffset); }
            b.byte(bank.effectTable.has_value()); b.u32(bank.effectTable.value_or(0)); b.byte(bank.effectMaster);
        }
        out("HD_BANKS", b.h);
        Hash q;
        for (const char* name : {"SNDBOOTS", "SNDTNNLS", "SNDCLOKS", "SNDTM30S", "SNDTM60S", "SNDLOGOS", "SNDWARNS", "SNDRCLKS"}) {
            const data::SqSequence s = data::SqSequence::parse(image.member(name));
            q.u16(s.volume); q.u16(s.resolution); q.u32(s.tempo); q.u64(s.channels.size()); q.u64(s.events.size());
            for (const auto& c : s.channels) { q.byte(c.channel); q.byte(c.program); q.byte(c.volume); q.byte(c.pan); }
            for (const auto& e : s.events) { q.u32(e.tick); q.byte(e.status); q.byte(e.data1); q.byte(e.data2); q.byte(e.hasData2); q.u32(e.tempo); }
        }
        out("SQ_SEQUENCES", q.h);
        const driver::SoundImages images = driver::buildSoundImages(resources);
        Hash s;
        s.u64(images.snapshot.osdsnd.size()); s.bytes(images.snapshot.osdsnd);
        s.u64(images.snapshot.iop.size()); s.bytes(images.snapshot.iop);
        s.u64(images.libsd.size()); s.bytes(images.libsd);
        s.bytes(images.spuRam);
        out("SOUND_IMAGES", s.h);
    }
    {
        Hash h;
        Rng rng{6};
        for (int round = 0; round < 40; ++round) {
            EeSoundQueue q(int32_t(rng.below(20)) - 2, rng.below(2) != 0);
            for (int n = 0; n < 400; ++n) {
                if (rng.below(3) == 0) {
                    const auto sends = q.drain();
                    h.u64(sends.size());
                    for (const auto& c : sends) { h.u16(c.id); h.u16(c.a1); h.u16(c.a2); h.u16(c.a3); }
                } else {
                    const uint16_t ids[] = {0x6300, 0x6300, 0x6140, 0x6150, 0x60D0, 0, 1};
                    SoundCommand c{ids[rng.below(7)], uint16_t(rng.below(3)), uint16_t(rng.below(4)), uint16_t(rng.next())};
                    const int burst = 1 + int(rng.below(rng.below(10) == 0 ? 140 : 3));
                    for (int b = 0; b < burst; ++b) h.byte(q.enqueue(c));
                }
                h.u16(q.carriedA2()); h.u16(q.carriedA3());
            }
        }
        out("SOUND_QUEUE", h.h);
    }
    {
        Hash h;
        Rng rng{7};
        for (int round = 0; round < 3000; ++round) {
            std::vector<SpuWrite> w(rng.below(12));
            uint32_t cycle = 0, resume = 0;
            for (auto& x : w) {
                cycle += rng.below(30000);
                if (rng.below(4) == 0) resume = cycle + rng.below(5000);
                x.cycle = cycle;
                x.resume = resume;
            }
            const uint64_t entry = rng.next() % 100000;
            std::vector<uint64_t> vb;
            uint64_t t = entry > 20000 ? entry - rng.below(20000) : 0;
            for (int k = 0; k < 6; ++k) { t += rng.below(40000); vb.push_back(t); }
            driver::addVblankPreemption(w, entry, vb);
            for (const auto& x : w) h.u32(x.cycle);
            h.u64(driver::sampleOfCycle(entry + rng.below(100000), entry));
        }
        out("PREEMPTION", h.h);
    }
    return 0;
}
