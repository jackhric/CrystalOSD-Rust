#include <cstdio>
#include "session.hpp"
#include "hashes.hpp"

static void hworld(Hash& h, const MenuWorld& w) {
    hclock(h, w.clock); hhead(h, w.head); hramp(h, w.spriteFade);
    const MenusState& m = w.menus;
    for (int32_t x : {m.page.word0, m.page.count, m.page.titleWidth, m.page.selected, m.page.word14, m.page.level, m.page.word2c, m.page.word30, m.page.glow}) h.i32(x);
    hramp(h, m.page.ramp);
    for (const ConfigEntry& e : m.entries) hentry(h, e);
    h.i32(m.mainMenu.selected); hramp(h, m.mainMenu.ramp); hramp(h, m.versionRamp); hramp(h, m.dialogRamp); hramp(h, m.firstRunRamp);
    hversion(h, m.version); hversion(h, m.versionDrawn); hramp(h, m.versionRampDrawn); h.boolean(m.versionDrawnValid);
    h.i32(m.entryActive); h.i32(m.screenCode);
    for (const AdjustField& f : m.adjustFields) { h.i32(f.item); h.i32(f.lowest); h.i32(f.highest); }
    h.boolean(m.configGate.has_value()); h.i32(m.configGate.value_or(0));
    h.i32(m.listFade.first); h.i32(m.listFade.second); h.i32(m.listFade.secondIndex);
    const CubeState& q = w.cubes;
    hramp(h, q.ramp); h.f32(q.list.pulse); for (int32_t x : {q.list.pulsed, q.list.position, q.list.left, q.list.speed, q.list.slowing}) h.i32(x);
    for (int32_t x : w.items) h.i32(x);
}

template <class A>
void run(const char* tag, const ColdAssets& assets) {
    // The clock screen without menus, with and without text: the cold start's clock with its menus removed.
    Hash alone;
    for (int pal = 0; pal < 2; ++pal)
        for (int withText = 0; withText < 2; ++withText) {
            ColdAssets a = assets;
            if (!withText) a.font = nullptr;
            ColdInputs in = hostInputs(pal, goldenNow(1800000000 + pal * 7777, 0), 60, true);
            in.gsAllocator = 770048; in.language = 2 + pal; in.timeFormat = pal; in.dateFormat = 2 - pal;
            ColdStartOut cold = coldStart<A>(a, in);
            cold.clock.menus.reset();
            cold.clock.passNames = false;
            Clock<A> clock(cold.clock);
            FrameInputs f = cold.frame;
            f.configItems.reset();
            for (int i = 0; i < 300; ++i) {
                const ConsoleNowR c = consoleNow(goldenNow(1800000000 + pal * 7777, uint64_t(i)), f.menu.mechaconParam->at(0));
                if (i > 0) { f.time = {c.ms, c.local[5], c.local[4], c.local[3]}; f.items = {c.local[0], c.local[1], c.local[2], c.local[3], c.local[4], c.local[5]}; }
                f.item0 = (i / 100) % 3;
                if (i % 50 == 0) f.timeFilled = (i / 50) % 2; else if (i % 50 == 25) f.timeFilled.reset();
                f.displayIndex = i & 1;
                f.field = (i / 3) & 1;
                const Frame frame = clock.frame(f);
                alone.u64(app::hashFrame(frame));
                if (clock.text()) { const std::vector<uint8_t> image = glyphCacheImage(*assets.font, frame.glyphs); alone.u64(app::hashBytes(image)); }
                alone.u32(uint32_t(clock.strings().size()));
            }
        }
    report(std::string(tag) + " alone", alone);

    // The menus' functions on worlds the app does not reach: externals missing or dirty, the disc changed, leaving, the
    // dialog and first-run ramps up, modes set; random pads.
    Hash menus;
    Rng r(0x3e95);
    const auto u = [&](uint32_t n) { return r.next32() % n; };
    for (int c = 0; c < 40; ++c) {
        ColdInputs in = hostInputs(u(2), goldenNow(1700000000 + int64_t(u(100000000)), 0), int(u(1000)) - 500, u(2));
        in.dateFormat = int(u(3)); in.aspect = int(u(3));
        ColdStartOut cold = coldStart<A>(ColdAssets{assets.program, nullptr, assets.mesh, assets.cubeMesh}, in);
        ClockState clock = cold.clock.state;
        HeadState head = cold.clock.head;
        Ramp fade = cold.clock.orbs.spriteFade;
        MenusState m = cold.clock.menus->menus;
        CubeState q = cold.clock.menus->cubes;
        ConfigItems items = cold.clock.menus->items;
        std::vector<SoundCommand> sounds;
        std::optional<DateWords> rtc;
        MenuWorld w{clock, head, fade, m, q, items, 640, cold.clock.height, &sounds, &rtc};
        Menus menu({u(2) != 0});
        const uint32_t bits[8] = {pad::Up, pad::Down, pad::Left, pad::Right, pad::Cross, pad::Circle, pad::Square, pad::Triangle};
        for (int f = 0; f < 400; ++f) {
            MenuExternals ext = cold.frame.menu;
            if (u(20) == 0) ext.configDirty.reset(); else if (u(5) == 0) (*ext.configDirty)[u(3)] = 1;
            if (u(30) == 0) ext.rtcMirror.reset();
            if (u(40) == 0) ext.mechaconParam.reset();
            if (u(50) == 0) ext.disc = int32_t(u(0x80));
            ext.pad.pressed = u(3) == 0 ? bits[u(8)] : 0;
            ext.pad.repeating = u(4) == 0 ? bits[u(8)] : ext.pad.pressed;
            if (u(200) == 0) clock.scene.leaving = 1;
            if (u(150) == 0) Menus::show(m.dialogRamp);
            if (u(150) == 0) Menus::hide(m.dialogRamp);
            if (u(150) == 0) Menus::show(m.firstRunRamp);
            if (u(100) == 0) Menus::setMode(w, int32_t(u(5)));
            if (u(60) == 0) clock.mode = int32_t(u(4));
            if (u(60) == 0) clock.overlayLevel = u(2) ? 0x80 - clock.tail : 0x80;
            sounds.clear(); rtc.reset();
            std::vector<std::string> notes;
            // A menu function that throws on these externals throws in both; what it left is compared.
            try { Menus::between(w, ext, notes); menus.u32(1); } catch (const std::exception&) { menus.u32(0xe1); }
            try { Menus::menuStep(w, ext); menus.u32(2); } catch (const std::exception&) { menus.u32(0xe2); }
            try { menu.step(w, ext, notes); menus.u32(3); } catch (const std::exception&) { menus.u32(0xe3); }
            try { Menus::endOfFrame(w, ext); menus.u32(4); } catch (const std::exception&) { menus.u32(0xe4); }
            hworld(menus, w);
            menus.u32(uint32_t(sounds.size())); for (const SoundCommand& s : sounds) { menus.u32(s.id); menus.u32(s.a1); menus.u32(s.a2); menus.u32(s.a3); menus.boolean(s.carriedA2); menus.boolean(s.carriedA3); }
            menus.boolean(rtc.has_value()); if (rtc) for (int32_t x : *rtc) menus.i32(x);
            menus.u32(uint32_t(notes.size())); for (const std::string& n : notes) hbytes(menus, n);
            if (ext.configDirty) for (int32_t x : *ext.configDirty) menus.i32(x);
        }
    }
    report(std::string(tag) + " menus", menus);

    // Sessions with menus and no font (the app without FNTOSD): the walk and the tour.
    Hash nofont;
    ColdAssets a = assets;
    a.font = nullptr;
    for (int i : {0, 2}) nofont.u64(runScenario<A>(a, scenarios()[size_t(i)], false).h);
    report(std::string(tag) + " nofont", nofont);
}

int main() {
    const ColdAssets assets = coldAssets();
    run<NativeArithmetic>("native", assets);
#ifndef SCENE_NATIVE_ONLY
    run<EeArithmetic>("ee", assets);
#endif
}
