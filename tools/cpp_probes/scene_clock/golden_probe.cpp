// Mirrors Source/App/Main.cpp's golden branch (no renderer, no audio): opening, gap, clock with the scripted presses.
#include <chrono>
#include <cstdio>
#include <fstream>
#include <iterator>
#include <map>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "App/Golden.hpp"
#include "Scene/Clock.hpp"
#include "Scene/ColdStart.hpp"
#include "Scene/ConsoleClock.hpp"
#include "Scene/ClockDate.hpp"
#include "Scene/ElfRecords.hpp"
#include "Scene/opening/Opening.hpp"
#include "Scene/opening/Flat.hpp"

using namespace scene;
using std::chrono::system_clock;

static std::vector<uint8_t> readAll(const char* p) { std::ifstream f(p, std::ios::binary); return {std::istreambuf_iterator<char>(f), {}}; }

static RodMesh meshAt(const ProgramImage& p, uint32_t faces, uint32_t pos, uint32_t nor, uint32_t coo) {
    RodMesh m;
    for (uint32_t i = 0; i < faces * 4; ++i) m.positions.push_back(vec4At(p, pos + 16 * i));
    for (uint32_t i = 0; i < faces; ++i) m.normals.push_back(vec4At(p, nor + 16 * i));
    for (uint32_t i = 0; i < faces * 4; ++i) m.coordinates.push_back(vec4At(p, coo + 16 * i));
    return m;
}

struct Press { uint64_t after; uint32_t bit; };

int main(int argc, char** argv) {
    const char* res = "../CrystalOSD/Resources/";
    auto program = std::make_shared<const ProgramImage>(readAll((std::string(res) + "hddosd.elf").c_str()));
    auto font = std::make_shared<const Font>(readAll((std::string(res) + "FNTOSD").c_str()));
    const ColdAssets assets{program, font, meshAt(*program, 16, 0x002b4b90, 0x002b5390, 0x002b4f90), meshAt(*program, 6, 0x002b5790, 0x002b5910, 0x002b5970)};
    const auto zone = std::chrono::current_zone();
    const system_clock::time_point goldenStart = zone->to_sys(std::chrono::local_days{std::chrono::year(2026) / 1 / 1} + std::chrono::hours(12), std::chrono::choose::earliest);
    system_clock::time_point fixedNow = goldenStart;
    system_clock::duration offset{};
    uint64_t ticks = 0;
    const std::vector<Press> presses{{120, pad::Down}, {200, pad::Up}, {280, pad::Triangle}, {360, pad::Down}, {420, pad::Up}, {480, pad::Circle}, {560, pad::Down}, {640, pad::Cross},
                                     {760, pad::Down}, {820, pad::Down}, {880, pad::Up}, {940, pad::Cross}, {1040, pad::Circle}, {1120, pad::Square}, {1220, pad::Square}, {1320, pad::Circle}};
    uint32_t held = 0, previous = 0, pressedBits = 0, releasedBits = 0;
    const auto padFrame = [&] {
        PadWords w{held, (held & ~previous) | pressedBits, (previous & ~held) | releasedBits, 0};
        w.repeating = w.pressed;
        previous = held; pressedBits = releasedBits = 0;
        return w;
    };
    std::printf("goldenStart %lld\n", (long long)std::chrono::duration_cast<std::chrono::seconds>(goldenStart.time_since_epoch()).count());

    opening::BootOptions boot;
    opening::Opening<NativeArithmetic> op(boot, program);
    int display = 0, gapLeft = opening::kFramesToClock, gapDisplay = 0;
    int phase = 0;
    std::unique_ptr<Clock<NativeArithmetic>> clock;
    FrameInputs inputs;
    ConfigItems items{};
    uint32_t coldWord = 0;
    const auto zoneWord = [&]() -> uint32_t { if (clock && clock->externals().mechaconParam) return clock->externals().mechaconParam->at(0); return coldWord; };
    const auto consoleNow = [&](system_clock::time_point now, uint32_t word) {
        const auto second = std::chrono::floor<std::chrono::seconds>(now);
        const DateWords rtc = rtcAt(second.time_since_epoch().count());
        struct R { DateWords rtc, local; float ms; };
        return R{rtc, localOf(rtc, zoneOf(word)), std::chrono::duration<float, std::milli>(now - second).count()};
    };
    uint64_t clockFrames = 0;
    for (uint64_t logic = 0; clockFrames < 1440; ++logic) {
        for (const Press& p : presses) {
            if (p.after == clockFrames) { if (!(held & p.bit)) pressedBits |= p.bit; held |= p.bit; }
            if (p.after + 6 == clockFrames) { held &= ~p.bit; if (!(held & p.bit)) releasedBits |= p.bit; }
        }
        const system_clock::time_point now0 = fixedNow + offset;
        { auto c = consoleNow(now0, zoneWord()); inputs.time = {c.ms, c.local[5], c.local[4], c.local[3]}; inputs.items = {c.local[0], c.local[1], c.local[2], c.local[3], c.local[4], c.local[5]}; }
        Frame frame;
        const char* name = "";
        if (phase == 0) {
            frame = op.frame(display, 0);
            display ^= 1;
            if (op.ended()) phase = gapLeft > 0 ? 1 : 2;
            name = "opening";
        } else if (phase == 1) {
            frame = {};
            frame.displayIndex = gapDisplay;
            frame.textureSet = TextureSet::Opening;
            frame.depthBits = 24;
            opening::Flat<NativeArithmetic>::scissor(frame.passes);
            gapDisplay ^= 1;
            if (--gapLeft == 0) phase = 2;
            name = phase == 2 ? "clock" : "black_gap";
        } else {
            name = "clock";
            if (!clock) {
                // hostInputs: no stored settings, the host zone's offset, the console RTC at JST.
                ColdInputs cold;
                cold.language = 1; cold.aspect = 0; cold.dateFormat = 0; cold.timeFormat = 0; cold.summerTime = false; cold.pal = false;
                cold.screenWidth = 640; cold.screenHeight = 0xE0; cold.randState = 1; cold.wide = false; cold.screenCode = 0x64;
                const auto info = zone->get_info(fixedNow);
                cold.timeZone = static_cast<int>(std::chrono::duration_cast<std::chrono::minutes>(info.offset - info.save).count());
                cold.summerTime = info.save != std::chrono::minutes::zero();
                const auto console = std::chrono::floor<std::chrono::seconds>(fixedNow) + std::chrono::minutes(kBaseZone);
                const auto day = std::chrono::floor<std::chrono::days>(console);
                const std::chrono::year_month_day date{day};
                const std::chrono::hh_mm_ss t{console - day};
                cold.rtc = {int32_t(date.year()), int32_t(unsigned(date.month())), int32_t(unsigned(date.day())), int32_t(t.hours().count()), int32_t(t.minutes().count()), int32_t(t.seconds().count())};
                cold.wide = true;
                cold.randState = op.randState();
                cold.gsAllocator = 0x2ec0 * 64 + 64 * 64 * 4 / 256 * 64;
                std::printf("cold: zone %d summer %d rtc %d-%d-%d %d:%d:%d rand %u alloc %u\n", cold.timeZone, cold.summerTime, cold.rtc.year, cold.rtc.month, cold.rtc.day, cold.rtc.hour, cold.rtc.minute, cold.rtc.second, cold.randState, cold.gsAllocator);
                ColdStartOut start = coldStart<NativeArithmetic>(assets, cold);
                start.clock.menus->options.browserEnters = false;
                start.clock.passNames = false;
                clock = std::make_unique<Clock<NativeArithmetic>>(start.clock);
                inputs = start.frame;
                coldWord = inputs.menu.mechaconParam ? inputs.menu.mechaconParam->at(0) : 0u;
                inputs.field = 0;
                inputs.threadStep = false;
                items = start.clock.menus->items;
            }
            const system_clock::time_point now = fixedNow + offset;
            if (clock->menus()->page.level != 1) {
                const DateWords l = consoleNow(now, zoneWord()).local;
                for (int i = 0; i < 6; ++i) items[6 + i] = l[i];
            }
            inputs.menu.pad = padFrame();
            inputs.menu.rtcMirror = consoleNow(now, zoneWord()).rtc;
            inputs.configItems = items;
            frame = clock->frame(inputs);
            if (const auto& w = clock->rtcWrite()) {
                offset = std::chrono::seconds(unixOf(*w) - std::chrono::floor<std::chrono::seconds>(fixedNow).time_since_epoch().count() - int64_t(kBaseZone) * 60);
                std::printf("rtc write at %llu\n", (unsigned long long)logic);
            }
            items = clock->items();
            fillTime(inputs);
            inputs.threadStep = true;
            inputs.field = 0;
            inputs.displayIndex ^= 1;
        }
        std::printf("%06llu %s scene=%016llx\n", (unsigned long long)logic, name, (unsigned long long)app::hashFrame(frame));
        if (phase == 2 && clock) ++clockFrames; else if (phase == 2) ++clockFrames;
        ++ticks;
        fixedNow = goldenStart + std::chrono::duration_cast<system_clock::duration>(std::chrono::duration<double>(double(ticks) * 1001.0 / 60000.0));
    }
    return 0;
}
