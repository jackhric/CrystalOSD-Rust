#pragma once
// Mirror of the Rust test scaffold tests/common/shell.rs (itself Main.cpp's startClock/stepClock).
#include <chrono>
#include <fstream>
#include <iterator>
#include <memory>
#include <string>
#include <vector>

#include "App/Golden.hpp"
#include "Scene/Clock.hpp"
#include "Scene/ClockDate.hpp"
#include "Scene/ColdStart.hpp"
#include "Scene/ConsoleClock.hpp"
#include "Scene/ElfRecords.hpp"
#include "common.hpp"

using namespace scene;

inline std::vector<uint8_t> readAll(const std::string& p) { std::ifstream f(p, std::ios::binary); return {std::istreambuf_iterator<char>(f), {}}; }
inline RodMesh meshAt(const ProgramImage& p, uint32_t faces, uint32_t pos, uint32_t nor, uint32_t coo) {
    RodMesh m;
    for (uint32_t i = 0; i < faces * 4; ++i) m.positions.push_back(vec4At(p, pos + 16 * i));
    for (uint32_t i = 0; i < faces; ++i) m.normals.push_back(vec4At(p, nor + 16 * i));
    for (uint32_t i = 0; i < faces * 4; ++i) m.coordinates.push_back(vec4At(p, coo + 16 * i));
    return m;
}
inline ColdAssets coldAssets() {
    const std::string res = "/home/jackh/Documents/git/CrystalOSD/Resources/";
    auto program = std::make_shared<const ProgramImage>(readAll(res + "hddosd.elf"));
    auto font = std::make_shared<const Font>(readAll(res + "FNTOSD"));
    return {program, font, meshAt(*program, 16, 0x002b4b90, 0x002b5390, 0x002b4f90), meshAt(*program, 6, 0x002b5790, 0x002b5910, 0x002b5970)};
}

struct PadReader {
    uint32_t held = 0, previous = 0, pressed = 0, released = 0;
    void down(uint32_t bit) { if (!(held & bit)) pressed |= bit; held |= bit; }
    void up(uint32_t bit) { held &= ~bit; if (!(held & bit)) released |= bit; }
    PadWords frame() {
        PadWords w{held, (held & ~previous) | pressed, (previous & ~held) | released, 0};
        w.repeating = w.pressed;
        previous = held; pressed = released = 0;
        return w;
    }
};

constexpr int64_t kNanos = 1000000000;
inline int64_t floorDiv(int64_t a, int64_t b) { int64_t q = a / b; if ((a % b != 0) && ((a < 0) != (b < 0))) --q; return q; }
struct ConsoleNowR { DateWords rtc, local; float ms; };
inline ConsoleNowR consoleNow(int64_t nowNs, uint32_t word) {
    const int64_t second = floorDiv(nowNs, kNanos);
    const DateWords rtc = rtcAt(second);
    return {rtc, localOf(rtc, zoneOf(word)), static_cast<float>(nowNs - second * kNanos) / 1000000.0f};
}
inline int64_t goldenNow(int64_t startSeconds, uint64_t ticks) { return startSeconds * kNanos + static_cast<int64_t>(double(ticks) * 1001.0 / 60000.0 * 1e9); }

inline ColdInputs hostInputs(bool pal, int64_t nowNs, int zone, bool summer) {
    const DateWords r = rtcAt(floorDiv(nowNs, kNanos));
    ColdInputs c;
    c.language = 1; c.aspect = 0; c.dateFormat = 0; c.timeFormat = 0; c.summerTime = summer; c.pal = pal;
    c.screenWidth = 640; c.screenHeight = pal ? 0x100 : 0xE0; c.randState = 1; c.wide = false; c.screenCode = 0x64; c.timeZone = zone;
    c.rtc = {r[0], r[1], r[2], r[3], r[4], r[5]};
    return c;
}

template <class A>
struct Session {
    std::unique_ptr<Clock<A>> clock;
    FrameInputs inputs;
    ConfigItems items{};
    PadReader reader;
    uint32_t coldWord = 0;
    int64_t offsetNs = 0;
    bool started = true;
    Session(const ColdAssets& assets, const ColdInputs& cold) {
        ColdStartOut start = coldStart<A>(assets, cold);
        if (start.clock.menus) start.clock.menus->options.browserEnters = false;
        start.clock.passNames = false;
        clock = std::make_unique<Clock<A>>(start.clock);
        inputs = start.frame;
        coldWord = inputs.menu.mechaconParam ? inputs.menu.mechaconParam->at(0) : 0u;
        inputs.field = 0;
        inputs.threadStep = false;
        items = start.clock.menus->items;
        offsetNs = cold.clockOffset.value_or(0) * kNanos;
    }
    uint32_t zoneWord() const { return clock->externals().mechaconParam ? clock->externals().mechaconParam->at(0) : coldWord; }
    Frame step(int64_t wallNs) {
        const int64_t now = wallNs + offsetNs;
        if (!started) {
            const ConsoleNowR c = consoleNow(now, zoneWord());
            inputs.time = {c.ms, c.local[5], c.local[4], c.local[3]};
            inputs.items = {c.local[0], c.local[1], c.local[2], c.local[3], c.local[4], c.local[5]};
        }
        started = false;
        if (clock->menus()->page.level != 1) {
            const DateWords l = consoleNow(now, zoneWord()).local;
            for (int i = 0; i < 6; ++i) items[6 + i] = l[i];
        }
        inputs.menu.pad = reader.frame();
        inputs.menu.rtcMirror = consoleNow(now, zoneWord()).rtc;
        inputs.configItems = items;
        Frame produced = clock->frame(inputs);
        if (const auto& w = clock->rtcWrite()) offsetNs = (unixOf(*w) - floorDiv(wallNs, kNanos) - int64_t(kBaseZone) * 60) * kNanos;
        items = clock->items();
        fillTime(inputs);
        inputs.threadStep = true;
        inputs.field = 0;
        inputs.displayIndex ^= 1;
        return produced;
    }
};

inline void hstate(Hash& h, const FontState& s) {
    for (int32_t x : {s.lineHeight, s.fixed, s.percent, s.pitch, s.decoration, s.clip, s.blank, s.ascent, s.dirty}) h.i32(x);
    h.f32(s.tv); h.f32(s.ratio); h.f32(s.locate[0]); h.f32(s.locate[1]);
    for (float c : s.colour) h.f32(c);
    for (const auto& r : s.matrix) for (float x : r) h.f32(x);
}
inline void hbytes(Hash& h, const std::string& s) { h.u32(uint32_t(s.size())); for (unsigned char c : s) h.u32(c); }
inline void hramp(Hash& h, const Ramp& r) { h.i32(r.length); h.i32(r.counter); h.i32(r.changed); h.i32(r.state); }

// The digest of one frame: the scene hash and what else the app reads of the clock.
template <class A>
uint64_t digest(const Clock<A>& c, const Frame& frame) {
    Hash h;
    h.u64(app::hashFrame(frame));
    h.u32(uint32_t(c.sounds().size()));
    for (const SoundCommand& s : c.sounds()) { h.u32(s.id); h.u32(s.a1); h.u32(s.a2); h.u32(s.a3); h.boolean(s.carriedA2); h.boolean(s.carriedA3); }
    h.u32(uint32_t(c.notes().size()));
    for (const std::string& n : c.notes()) hbytes(h, n);
    h.u32(uint32_t(c.strings().size()));
    for (const StringRun& r : c.strings()) { hbytes(h, r.text); h.boolean(r.measuring); hstate(h, r.own); }
    for (int32_t x : c.items()) h.i32(x);
    h.boolean(c.rtcWrite().has_value());
    if (c.rtcWrite()) for (int32_t x : *c.rtcWrite()) h.i32(x);
    const ClockState& s = c.state();
    h.i32(s.mode); h.i32(s.overlayLevel); h.i32(s.timeFilled); h.f32(s.scene.scale); h.f32(s.scaleTarget); h.u32(s.spin.value_or(0)); hramp(h, s.menuRamp); hramp(h, s.vignetteRamp);
    h.i32(s.time.hours); h.i32(s.time.minutes); h.i32(s.time.seconds); h.f32(s.time.milliseconds);
    const MenusState& m = *c.menus();
    h.i32(m.page.level); h.i32(m.page.selected); h.i32(m.page.glow); h.i32(m.page.titleWidth); hramp(h, m.page.ramp); hramp(h, m.mainMenu.ramp); h.i32(m.mainMenu.selected);
    hramp(h, m.versionRamp); h.i32(m.version.selected); h.i32(m.version.first); h.i32(m.version.count); h.i32(m.version.panelAlpha);
    h.i32(m.listFade.first); h.i32(m.listFade.second); h.i32(m.listFade.secondIndex); h.i32(m.entryActive); h.i32(m.configGate.value_or(-99));
    for (const ConfigEntry& e : m.entries) h.i32(e.valueIndex);
    for (const AdjustField& f : m.adjustFields) { h.i32(f.item); h.i32(f.lowest); h.i32(f.highest); }
    const CubeState& q = *c.cubes();
    h.i32(q.list.position); h.i32(q.list.left); h.i32(q.list.speed); h.i32(q.list.slowing); h.f32(q.list.pulse); h.i32(q.list.pulsed); hramp(h, q.ramp);
    if (c.text()) { h.u32(uint32_t(c.text()->cache().list.size())); for (const FontCacheEntry& e : c.text()->cache().list) { h.i32(e.code); h.i32(e.loaded); h.i32(e.cell); h.u32(e.block); } }
    return h.h;
}

struct Scenario {
    const char* name;
    bool pal;
    int language, aspect, zone;
    bool summer;
    int dateFormat, timeFormat;
    bool wide;
    uint32_t rand;
    int64_t start;
    int frames;
    int script;
    uint64_t seed;
};

struct Press { uint64_t at, bit, hold; };

// Script 1: System Configuration entry by entry (the clock adjusted and confirmed, every value row entered); script 2: the
// clock adjust cancelled, the Version page, the menu hidden and shown; script 3: random presses; script 4: none.
inline std::vector<Press> pressesOf(const Scenario& s) {
    std::vector<Press> out;
    const auto at = [&](uint64_t t, uint32_t bit) { out.push_back({t + 150, bit, 4}); };
    if (s.script == 1) {
        at(100, pad::Down); at(130, pad::Cross);
        uint64_t t = 280;
        at(t, pad::Cross);
        const uint32_t edits[] = {pad::Up, pad::Up, pad::Down, pad::Right, pad::Up, pad::Right, pad::Down, pad::Down, pad::Right, pad::Up, pad::Right, pad::Down, pad::Right, pad::Up,
                                  pad::Right, pad::Left, pad::Left, pad::Up};
        for (uint32_t e : edits) at(t += 12, e);
        at(t += 30, pad::Cross);
        for (int entry = 1; entry < 7; ++entry) {
            at(t += 60, pad::Down);
            at(t += 40, pad::Cross);
            at(t += 30, pad::Right);
            at(t += 20, pad::Left);
            at(t += 20, entry % 2 ? pad::Cross : pad::Circle);
        }
        at(t += 60, pad::Down); at(t += 30, pad::Up); at(t += 30, pad::Triangle);
        at(t += 60, pad::Square); at(t += 120, pad::Square); at(t += 120, pad::Circle);
    } else if (s.script == 2) {
        at(80, pad::Triangle); at(120, pad::Down); at(140, pad::Down); at(160, pad::Up); at(180, pad::Triangle); at(220, pad::Circle);
        at(300, pad::Down); at(320, pad::Up); at(340, pad::Down); at(360, pad::Cross);
        at(500, pad::Cross); at(530, pad::Up); at(545, pad::Right); at(560, pad::Down); at(580, pad::Circle);
        at(640, pad::Square); at(760, pad::Square); at(800, pad::Square); at(830, pad::Square);
        at(900, pad::Down); at(930, pad::Cross); at(970, pad::Circle);
        at(1050, pad::Up); at(1080, pad::Circle); at(1200, pad::Triangle); at(1240, pad::Circle);
    } else if (s.script == 3) {
        Rng r(s.seed);
        const uint32_t table[16] = {pad::Up, pad::Down, pad::Up, pad::Down, pad::Left, pad::Right, pad::Left, pad::Right,
                                    pad::Cross, pad::Cross, pad::Circle, pad::Square, pad::Triangle, pad::Down, pad::Cross, pad::Up};
        for (uint64_t t = 60; t < uint64_t(s.frames);) {
            t += 6 + r.next32() % 35;
            out.push_back({t, table[r.next32() % 16], 1 + r.next32() % 8});
        }
    }
    return out;
}

inline const std::vector<Scenario>& scenarios() {
    static const std::vector<Scenario> all = {
        {"ntsc-en-walk", false, 1, 0, 540, false, 0, 0, false, 1, 1772953140, 1700, 1, 0},
        {"pal-fr-walk", true, 3, 1, 60, true, 2, 1, true, 12345, 1919898570, 1700, 1, 0},
        {"ntsc-de-tour", false, 2, 2, -480, false, 1, 1, false, 7, 4102444730, 1450, 2, 0},
        {"pal-es-tour", true, 4, 0, 330, false, 0, 0, false, 99, 1767225590, 1450, 2, 0},
        {"ntsc-it-random", false, 5, 1, -300, true, 2, 0, true, 3, 1800000000, 1600, 3, 11},
        {"pal-nl-random", true, 6, 2, 0, false, 1, 1, false, 5, 2000000000, 1600, 3, 12},
        {"ntsc-pt-random", false, 7, 0, 600, true, 0, 1, false, 9, 1700000123, 1600, 3, 13},
        {"pal-en-random", true, 1, 1, -600, false, 2, 1, true, 77, 3000000000, 1600, 3, 14},
        {"ntsc-ja-idle", false, 0, 0, 540, false, 0, 0, false, 1, 1767225599, 400, 4, 0},
        {"pal-de-idle", true, 2, 2, 120, true, 1, 0, true, 2, 1711846790, 400, 4, 0},
    };
    return all;
}

template <class A>
Hash runScenario(const ColdAssets& assets, const Scenario& s, bool perFrame) {
    ColdInputs cold = hostInputs(s.pal, goldenNow(s.start, 0), s.zone, s.summer);
    cold.language = s.language; cold.aspect = s.aspect; cold.dateFormat = s.dateFormat; cold.timeFormat = s.timeFormat; cold.wide = s.wide;
    cold.randState = s.rand; cold.gsAllocator = 770048;
    Session<A> session(assets, cold);
    const std::vector<Press> presses = pressesOf(s);
    Hash all;
    for (int i = 0; i < s.frames; ++i) {
        for (const Press& p : presses) {
            if (p.at == uint64_t(i)) session.reader.down(p.bit);
            if (p.at + p.hold == uint64_t(i)) session.reader.up(p.bit);
        }
        const Frame f = session.step(goldenNow(s.start, uint64_t(i)));
        const uint64_t d = digest(*session.clock, f);
        if (perFrame) std::printf("%s %d %016llx %016llx\n", s.name, i, (unsigned long long)d, (unsigned long long)app::hashFrame(f));
        all.u64(d);
    }
    return all;
}
