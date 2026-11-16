#include <cstdio>
#include <cstring>
#include <filesystem>
#include <memory>
#include <string>
#include <vector>

#include "Audio/ClockSound.hpp"
#include "Audio/LiveAudio.hpp"
#include "Audio/driver/Driver.hpp"
#include "Audio/driver/SnapshotBuilder.hpp"

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
    void str(const std::string& s) { u64(s.size()); for (char c : s) byte(uint8_t(c)); }
    void pcm(const std::vector<int16_t>& v) { for (int16_t x : v) u16(uint16_t(x)); }
};

struct Send {
    uint32_t frame;
    uint16_t id, a1, a2, a3;
    bool c2, c3;
};

static std::vector<Send> fixedBoot() {
    std::vector<Send> s{{1, 0x6140, 0, 0, 0, false, false}, {123, 0x6140, 1, 0xB800, 0x8000, false, false}, {250, 0x6150, 1, 0, 0, false, false},
                        {281, 0x6150, 6, 0, 0xF, false, false}, {281, 0x6140, 2, 0, 0, true, true}};
    for (uint16_t k = 0; k < 16; ++k) s.push_back({uint32_t(400 + 60 * k), 0x6300, 1, k, uint16_t(k == 3 ? 0 : 0xF), false, k == 3});
    s.push_back({1400, 0x6300, 1, 2, 0x180, false, false});
    s.push_back({1400, 0x6300, 1, 3, 0, false, true});
    s.push_back({1460, 0x6300, 1, 0, 0x8000, false, false});
    s.push_back({1460, 0x6300, 1, 1, 0, false, true});
    for (uint32_t f = 1500; f < 1531; ++f) s.push_back({f, 0x6300, 1, 6, 1, false, false});
    for (int k = 0; k < 3; ++k) s.push_back({1550, 0x6300, 1, 6, 1, false, false});
    s.push_back({1550, 0x6300, 1, 5, 0x57, false, false});
    s.push_back({1550, 0x6300, 1, 6, 0x57, false, false});
    s.push_back({1660, 0x6300, 1, 4, 0, false, false});
    s.push_back({1660, 0x6300, 1, 10, 0, false, false});
    s.push_back({5900, 0x6300, 0, 3, 0, false, false});
    return s;
}

static std::vector<Send> fixedOpening() {
    return {{1, 0x6150, 2, 0, 0, false, false},      {1, 0x6140, 6, 0, 0, true, true},       {123, 0x6140, 7, 0xB800, 0x8000, false, false},
            {123, 0x6150, 0, 0, 0x11, false, false}, {250, 0x6150, 1, 0, 0, false, false},   {281, 0x6150, 6, 0, 0xF, false, false},
            {281, 0x6140, 2, 0, 0, true, true},      {600, 0x6150, 2, 0, 0xE, false, false}, {900, 0x6150, 2, 0, 0xF, false, false},
            {1200, 0x6150, 2, 0, 0, false, false},   {1500, 0x6150, 0, 0, 0xF, false, false}, {1800, 0x6140, 1, 0xB800, 0x8000, false, false},
            {2100, 0x6150, 3, 0, 0, false, false},   {2101, 0x6150, 3, 0, 0, false, false},  {2400, 0x6140, 5, 0, 0, false, false},
            {2700, 0x6150, 1, 0, 0x22, false, false}};
}

static std::vector<Send> fuzz(uint64_t seed, uint32_t frames) {
    Rng rng{seed};
    std::vector<Send> s;
    for (uint32_t f = 1; f < frames; ++f) {
        if (rng.below(40) != 0) continue;
        const uint32_t n = 1 + rng.below(3);
        for (uint32_t k = 0; k < n; ++k) {
            const uint32_t kind = rng.below(10);
            Send c{f, 0, 0, 0, 0, false, false};
            if (kind < 5) {
                c.id = 0x6300;
                c.a1 = 1;
                c.a2 = uint16_t(rng.below(16));
                c.a3 = uint16_t(rng.next());
                c.c3 = rng.below(3) == 0;
            } else if (kind < 8) {
                c.id = 0x6140;
                c.a1 = uint16_t(rng.below(8));
                c.a2 = uint16_t(rng.next());
                c.a3 = uint16_t(rng.next());
                c.c2 = rng.below(2) == 0;
                c.c3 = rng.below(2) == 0;
            } else {
                c.id = 0x6150;
                c.a1 = uint16_t(rng.below(8));
                const uint16_t a3s[] = {0, 0xE, 0xF, 0x11, 0x10, 0x3F};
                c.a3 = a3s[rng.below(6)];
            }
            s.push_back(c);
        }
    }
    return s;
}

static void runLive(const char* name, const ClockSoundSources& sources, Video video, const std::vector<Send>& sends, uint32_t frames, uint32_t discardAt) {
    LiveAudioOptions options;
    options.device = false;
    options.video = video;
    LiveAudio live(sources, options);
    Hash total;
    std::vector<uint64_t> blocks;
    Hash block;
    size_t next = 0;
    for (uint32_t f = 1; f <= frames; ++f) {
        while (next < sends.size() && sends[next].frame == f) {
            const Send& c = sends[next++];
            live.queue({c.id, c.a1, c.a2, c.a3}, c.c2, c.c3);
        }
        if (f == discardAt) live.discardPending();
        const auto drained = live.drain();
        Hash fh;
        fh.u64(drained.size());
        for (const auto& c : drained) { fh.u16(c.id); fh.u16(c.a1); fh.u16(c.a2); fh.u16(c.a3); }
        live.step();
        fh.pcm(live.lastFrame());
        fh.byte(live.stats().failed);
        total.u64(fh.h);
        block.u64(fh.h);
        if (f % 250 == 0 || f == frames) {
            blocks.push_back(block.h);
            block = Hash{};
        }
    }
    const std::string line = live.finish();
    Hash lh;
    lh.str(line);
    const LiveAudioStats& st = live.stats();
    std::printf("    (\"%s\", 0x%016llx, 0x%016llx, %llu, %u, %llu, %d, &[", name, (unsigned long long)total.h, (unsigned long long)lh.h, (unsigned long long)st.samples, st.peak,
                (unsigned long long)st.clipped, st.failed ? 1 : 0);
    for (size_t i = 0; i < blocks.size(); ++i) std::printf("%s0x%016llx", i ? ", " : "", (unsigned long long)blocks[i]);
    std::printf("]),\n");
    std::fprintf(stderr, "%s: %s\n", name, line.c_str());
}

class Recorder final : public WriteTiming {
public:
    Recorder(bool modelled, uint64_t delay) : m_modelled(modelled), m_delay(delay) {}
    uint64_t sample(const WriteContext& c) override {
        h.byte(c.tick); h.u32(c.id); h.u64(c.eventSample); h.u64(c.index);
        h.u32(c.write.frame); h.u32(c.write.sample); h.u32(c.write.address); h.u16(c.write.value); h.u32(c.write.cycle); h.u32(c.write.resume);
        ++writes;
        return c.eventSample + (m_modelled ? c.write.cycle / 768 : 0) + m_delay;
    }
    Hash h;
    uint64_t writes = 0;

private:
    bool m_modelled;
    uint64_t m_delay;
};

struct Timed {
    uint64_t sample;
    uint32_t frame, id;
    std::array<uint32_t, 5> words;
};

static void runClock(const char* name, const ClockSoundSources& sources, ClockSoundOptions options, bool recordModelled, uint64_t delay, bool manualTicks, uint32_t chunks) {
    ClockSound sound(sources, options);
    auto recorder = std::make_shared<Recorder>(recordModelled, delay);
    sound.setWriteTiming(recorder);
    std::vector<Timed> commands{{15000, 0, 0x6140, {0, 0, 0, 0, 0}},           {15003, 0, 0x8070, {10, 0, 2033250, 0, 0}},
                                {15800, 0, 0x60d0, {1, 0, 0, 0, 0}},           {17402, 21, 0x60d0, {1, 344, 344, 0, 0}},
                                {112697, 140, 0x6140, {1, 0xFFFFB800, 0xFFFF8000, 0, 0}}, {213598, 266, 0x6150, {1, 0, 0, 0, 0}},
                                {238423, 297, 0x6150, {6, 0, 15, 0, 0}},       {238446, 297, 0x6140, {2, 1772, 1768, 0, 0}}};
    for (uint32_t k = 0; k < 16; ++k) commands.push_back({uint64_t(250000 + 20000 * k), 0, 0x6300, {1, k, 0xF, 0, 0}});
    commands.push_back({600000, 0, 0x6300, {1, 2, 384, 0, 0}});
    commands.push_back({600000, 0, 0x6300, {1, 3, 1872, 0, 0}});
    commands.push_back({600026, 0, 0x6150, {2, 0, 0xE, 0, 0}});
    commands.push_back({700000, 0, 0x6140, {7, 0, 0, 0, 0}});
    commands.push_back({700003, 0, 0x6150, {0, 0, 0x11, 0, 0}});
    commands.push_back({820000, 0, 0x6150, {3, 0, 0, 0, 0}});
    Hash total;
    std::vector<uint64_t> blocks;
    Hash block;
    std::vector<int16_t> pcm(2 * 801);
    size_t next = 0;
    std::string error;
    for (uint32_t chunk = 0; chunk < chunks && error.empty(); ++chunk) {
        const uint64_t at = uint64_t(chunk) * 801;
        while (next < commands.size() && commands[next].sample < at + 801) {
            DriverCommand c;
            c.sample = commands[next].sample;
            c.frame = commands[next].frame;
            c.id = commands[next].id;
            c.words = commands[next].words;
            sound.queue(c);
            ++next;
        }
        if (manualTicks) sound.scheduleTick(at + 400, chunk);
        try {
            sound.render(pcm.data(), chunk % 7 == 3 ? 300 : 801);
            if (chunk % 7 == 3) sound.render(pcm.data() + 600, 501);
        } catch (const std::exception& e) {
            error = e.what();
        }
        Hash fh;
        fh.pcm(pcm);
        fh.u64(recorder->h.h);
        fh.u64(sound.position());
        total.u64(fh.h);
        block.u64(fh.h);
        if ((chunk + 1) % 250 == 0 || chunk + 1 == chunks) {
            blocks.push_back(block.h);
            block = Hash{};
        }
    }
    Hash log;
    for (const LoggedCommand& c : sound.commandLog()) {
        log.u64(c.sample); log.u32(c.command.frame); log.u64(c.command.sample); log.u32(c.command.id);
        for (uint32_t w : c.command.words) log.u32(w);
    }
    Hash eh;
    eh.str(error);
    std::printf("    (\"%s\", 0x%016llx, 0x%016llx, 0x%016llx, %llu, &[", name, (unsigned long long)total.h, (unsigned long long)log.h, (unsigned long long)eh.h,
                (unsigned long long)recorder->writes);
    for (size_t i = 0; i < blocks.size(); ++i) std::printf("%s0x%016llx", i ? ", " : "", (unsigned long long)blocks[i]);
    std::printf("]),\n");
    std::fprintf(stderr, "%s: %llu writes, error '%s'\n", name, (unsigned long long)recorder->writes, error.c_str());
}

static void runDriver(const char* name, const driver::SoundImages& images, bool coldStart, bool timing, uint64_t seed) {
    driver::EnvxFeed feed;
    feed.tick = [n = uint32_t(0)](uint32_t c, uint32_t v) mutable {
        ++n;
        const uint32_t x = (n * 2654435761u) >> 9;
        return uint16_t((x % 7 == 0) ? (x & 3) : ((x ^ (c << 8) ^ v) & 0x7fff));
    };
    feed.allocation = [n = uint32_t(0)](uint32_t c, uint32_t v) mutable {
        ++n;
        const uint32_t x = (n * 40503u + v * 977u + c) >> 3;
        return uint16_t((x % 5 == 0) ? (x & 1) : (x & 0x7fff));
    };
    driver::Driver d(images.snapshot, images.libsd, feed, coldStart);
    d.seedRegister(0x1F90019A, 0xC080);
    if (timing) {
        d.enableTiming();
        d.setDmaCompletion([](uint32_t ordinal, uint32_t entry) -> std::optional<uint32_t> {
            if (ordinal % 3 == 2) return std::nullopt;
            return entry + 500 + 1000 * ordinal;
        });
    }
    Rng rng{seed};
    const std::array<uint32_t, 5> init[] = {{4, 68, 4096, 0, 64}};
    (void)init;
    struct Cmd { uint32_t id; std::array<uint32_t, 5> w; };
    std::vector<Cmd> script{{0x6010, {4, 68, 4096, 0, 64}},       {0x61a0, {965632, 20496, 65536, 0, 7}}, {0x6070, {4, 68, 0, 0, 64}},
                            {0x8070, {2, 1, 4096, 0, 5}},         {0x8070, {3, 1, 0, 0, 64}},             {0x8010, {2049, 4095, 0, 0, 64}},
                            {0x60c0, {0, 4, 0, 0, 64}},           {0x60c0, {1, 4, 0, 0, 64}},             {0x6060, {965632, 20496, 0, 0, 64}},
                            {0x6060, {990208, 544784, 0, 0, 64}}, {0x60a0, {60, 68, 0, 0, 64}}};
    for (uint32_t k = 0; k < 8; ++k) script.push_back({0x6090, {0, 969728 + 4096 * k + (k >= 5 ? 4096 : 0), 0, 0, 64}});
    for (uint32_t k = 0; k < 8; ++k) script.push_back({0x6130, {k, 27 + 5 * k, 0, 0, 64}});
    script.push_back({0x6310, {1, 28, 0, 0, 64}});
    script.push_back({0x6120, {0, 16383, 16383, 0, 64}});
    script.push_back({0x6120, {1, 16383, 16383, 0, 64}});
    script.push_back({0x6200, {4, 68, 0, 0, 64}});
    script.push_back({0x6140, {0, 0, 0, 0, 0}});
    Hash h;
    uint32_t errors = 0;
    const auto event = [&](bool tick, const Cmd& c, uint32_t frame) {
        d.beginFrame(frame);
        std::string error;
        try {
            if (tick) d.tick();
            else {
                DriverCommand x;
                x.id = c.id;
                x.words = c.w;
                d.command(x);
            }
        } catch (const std::exception& e) {
            error = e.what();
            ++errors;
        }
        h.str(error);
        for (const SpuWrite& w : d.takeWrites()) { h.u32(w.frame); h.u32(w.sample); h.u32(w.address); h.u16(w.value); h.u32(w.cycle); h.u32(w.resume); }
        for (const driver::FillEvent& f : d.takeFills()) { h.u32(f.core); h.u32(f.start); h.u64(f.bytes); h.u64(f.transferred); }
    };
    uint32_t frame = 0;
    for (const Cmd& c : script) event(false, c, frame++);
    const std::vector<Cmd> refusals{{0x6ff0, {}}, {0x1234, {}}, {0x60c0, {0, 7, 0, 0, 0}}, {0x60c0, {1, 9, 0, 0, 0}}, {0x60c0, {0, 10, 0, 0, 0}},
                                    {0x8070, {0xa, 1, 0, 0, 0}}, {0x8070, {0xa, 0, 0, 0, 0}}, {0x6120, {2, 1, 1, 0, 0}}, {0x60d0, {0xffff, 1, 1, 0, 0}},
                                    {0x6300, {1, 0x7fff, 0, 0, 0}}, {0x6300, {1, 0xffff, 0, 0, 0}}, {0x8070, {8, 0x15, 0, 0, 0}}, {0x8070, {4, 1, 0, 0, 0}},
                                    {0x8070, {5, 0, 0, 0, 0}}, {0x60c0, {0, 0xFFFF, 0, 0, 0}}, {0x6060, {0, 0, 0, 0, 0}}, {0x6090, {0x80, 969728, 0, 0, 0}},
                                    {0x6090, {0, 0, 0, 0, 0}}, {0x6130, {0x20, 3, 0, 0, 0}}, {0x6150, {0x30, 0, 0, 0, 0}}, {0x6140, {0xFFFF, 0, 0, 0, 0}}};
    for (const Cmd& c : refusals) event(false, c, frame++);
    for (int n = 0; n < 2500; ++n) {
        const uint32_t pick = rng.below(20);
        if (pick < 12) {
            event(true, {}, frame++);
            continue;
        }
        Cmd c{};
        if (pick < 15) c = {0x6300, {rng.below(3), rng.below(18), 0, 0, 0}};
        else if (pick < 17) c = {0x6140, {rng.below(8), 0, 0, 0, 0}};
        else if (pick < 19) c = {0x6150, {rng.below(8), rng.below(5), rng.below(2) ? 0u : 0x11u, 0, 0}};
        else {
            const uint32_t ids[] = {0x60d0, 0x6120, 0x8010, 0x60a0, 0x6130};
            const uint32_t id = ids[rng.below(5)];
            const uint32_t a = rng.below(2), b = rng.below(0x4000), e = rng.below(0x4000);
            c = {id, {id == 0x8010 ? 2049u + 2 * rng.below(8) : id == 0x60a0 ? 60u : a, b, e, 0, 0}};
        }
        event(false, c, frame++);
    }
    std::printf("    (\"%s\", 0x%016llx, %u),\n", name, (unsigned long long)h.h, errors);
}

int main(int argc, char** argv) {
    const std::filesystem::path resources = argv[1];
    const std::string part = argc > 2 ? argv[2] : "all";
    const ClockSoundSources sources = clockSoundSources(resources);
    if (part == "all" || part == "live") {
        std::printf("const LIVE: &[(&str, u64, u64, u64, u32, u64, i32, &[u64])] = &[\n");
        runLive("boot-ntsc", sources, Video::Ntsc, fixedBoot(), 6000, 0);
        runLive("boot-pal", sources, Video::Pal, fixedBoot(), 5000, 0);
        runLive("opening", sources, Video::Ntsc, fixedOpening(), 4000, 1201);
        runLive("fuzz-1", sources, Video::Ntsc, fuzz(11, 3000), 3000, 0);
        runLive("fuzz-2", sources, Video::Ntsc, fuzz(12, 3000), 3000, 0);
        runLive("fuzz-3", sources, Video::Pal, fuzz(13, 3000), 3000, 0);
        runLive("fuzz-4", sources, Video::Ntsc, fuzz(14, 3000), 3000, 0);
        std::printf("];\n");
    }
    if (part == "all" || part == "clock") {
        std::printf("const CLOCK: &[(&str, u64, u64, u64, u64, &[u64])] = &[\n");
        runClock("modelled-ntsc", sources, ClockSoundOptions{Video::Ntsc, true, true, true, 15000}, true, 0, false, 1200);
        runClock("entry-pal", sources, ClockSoundOptions{Video::Pal, true, true, false, 0}, false, 0, false, 1200);
        runClock("delayed", sources, ClockSoundOptions{Video::Ntsc, true, true, false, 0}, false, 9, false, 600);
        runClock("manual-ticks", sources, ClockSoundOptions{Video::Ntsc, false, true, true, 0}, true, 0, true, 600);
        runClock("cold", sources, ClockSoundOptions{Video::Ntsc, true, false, false, 0}, false, 0, false, 400);
        std::printf("];\n");
    }
    if (part == "all" || part == "driver") {
        const driver::SoundImages images = driver::buildSoundImages(resources);
        std::printf("const DRIVER: &[(&str, u64, u32)] = &[\n");
        runDriver("cold-timed", images, true, true, 21);
        runDriver("cold-untimed", images, true, false, 22);
        runDriver("warm-timed", images, false, true, 23);
        std::printf("];\n");
    }
    return 0;
}
