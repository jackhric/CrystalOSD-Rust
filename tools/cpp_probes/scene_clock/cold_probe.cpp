#include <cstdio>
#include <cstring>
#include "Scene/ColdFont.hpp"
#include "session.hpp"

static void h4(Hash& h, const Vec4& v) { for (float x : v) h.f32(x); }
static void hm(Hash& h, const Mat4& m) { for (const Vec4& r : m) h4(h, r); }
static void hc(Hash& h, const Colour& c) { for (int32_t x : c) h.i32(x); }
static void hrect(Hash& h, const Rect& r) { hc(h, r.colour); for (int32_t x : {r.x0, r.y0, r.u0, r.v0, r.x1, r.y1, r.u1, r.v1, r.z, r.blend, r.textured}) h.i32(x); }
static void hrecord(Hash& h, const RodRecord& r) {
    h.i32(r.number); h.i32(r.faces); hm(h, r.local); h.f32(r.sx); h.f32(r.sy); h.f32(r.sz); hc(h, r.base); h.f32(r.strength); hc(h, r.textured);
    h.f32(r.pair[0]); h.f32(r.pair[1]); h.f32(r.refraction); hc(h, r.reflection); hc(h, r.extra);
}
static void hclock(Hash& h, const ClockState& s) {
    h.f32(s.time.milliseconds); h.i32(s.time.seconds); h.i32(s.time.minutes); h.i32(s.time.hours);
    h.i32(s.eased.secondHand); h.i32(s.eased.hourHand); h.f32(s.eased.progress); h.f32(s.eased.fraction);
    h.i32(s.state.currentRod); h.i32(s.state.secondsAngle); h.i32(s.state.rodAngle);
    for (const RodState& r : s.state.rods) { h.f32(r.appearance); h.f32(r.progress); hc(h, r.base); hc(h, r.reflection); }
    hc(h, s.state.accent); hc(h, s.state.fourth); h.f32(s.state.progressTarget); hc(h, s.state.accentFrame); hc(h, s.state.currentTarget);
    hramp(h, s.appearance);
    hc(h, s.colours.base); hc(h, s.colours.reflection); hc(h, s.colours.fourth); hc(h, s.colours.currentReflection);
    h.i32(s.cycleCounters.base.index); h.i32(s.cycleCounters.base.wait); h.i32(s.cycleCounters.accent.index); h.i32(s.cycleCounters.accent.wait); h.i32(s.cycleCounters.last);
    for (const Colour& c : s.cycleTables.base) hc(h, c);
    for (const Colour& c : s.cycleTables.accent) hc(h, c);
    h.f32(s.logicConstants.rodEasing); h.f32(s.logicConstants.secondsEasing); h.f32(s.logicConstants.progressStep);
    h.f32(s.scene.scale); h.i32(s.scene.leaving); h.i32(s.scene.field);
    h.f32(s.scaleTarget); h.f32(s.scaleFactor); h.i32(s.timeFilled); h.boolean(s.spin.has_value()); h.u32(s.spin.value_or(0));
    h.i32(s.mode); h.i32(s.level); h.i32(s.overlayLevel); hramp(h, s.vignetteRamp); h.i32(s.vignetteLength); hramp(h, s.menuRamp); h.i32(s.tail); h.i32(s.counter);
    h.f32(s.proportions.ax); h.f32(s.proportions.ay); h4(h, s.position); h4(h, s.direction); h4(h, s.up); h4(h, s.rotation);
    h.f32(s.cameraOffset); h.f32(s.zmax); h.f32(s.cameraFactor);
}
static void hhead(Hash& h, const HeadState& s) {
    hramp(h, s.greyRamp); for (int32_t g : s.greys) h.i32(g); hramp(h, s.vignetteRamp);
    for (int32_t x : {s.ring.alpha, s.ring.cx, s.ring.cy, s.ring.rx, s.ring.ry, s.ring.z}) h.i32(x);
    for (const Rect* r : {&s.tint, &s.blur, &s.copy, &s.fade, &s.bars, &s.column}) hrect(h, *r);
}
static void hcache(Hash& h, const FontCache& c) {
    h.u32(uint32_t(c.list.size()));
    for (const FontCacheEntry& e : c.list) { h.i32(e.code); h.i32(e.loaded); h.i32(e.cell); h.u32(e.block); }
    for (int32_t x : {c.format, c.cellW, c.cellH, c.cells, c.width, c.height, c.logW, c.logH, c.setUp}) h.i32(x);
    h.u32(c.block); h.u32(c.memory); h.u32(c.texture); h.u32(c.table);
}
static void hentry(Hash& h, const ConfigEntry& e) {
    for (int32_t x : {e.id, e.valueCount, e.valueIndex, e.item}) h.i32(x);
    for (uint32_t x : {e.valueTable, e.enter, e.stringCallback, e.frameCallback, e.confirm, e.cancel, e.focus}) h.u32(x);
    for (int32_t x : e.rest) h.i32(x);
}
static void hrow(Hash& h, const VersionRow& r) { h.i32(r.label); hbytes(h, r.value); h.i32(r.subRows); h.i32(r.id); }
static void hversion(Hash& h, const VersionPage& v) {
    for (int32_t x : {v.title, v.count, v.shown, v.selected, v.first}) h.i32(x);
    for (const VersionRow& r : v.rows) hrow(h, r);
    for (int32_t x : {v.job, v.panelOn, v.panelAlpha, v.arrows}) h.i32(x);
    for (int32_t x : v.hints) h.i32(x);
    h.i32(v.polls);
}
static void hramps(Hash& h, const TextRamps& r) {
    for (const Ramp* x : {&r.config, &r.mainMenu, &r.version, &r.dialogClosing, &r.firstRun, &r.dialog}) hramp(h, *x);
    for (int32_t x : {r.lead, r.body, r.panel7, r.panel8On, r.panel8, r.adjustRow}) h.i32(x);
    for (int32_t x : r.panel8Ids) h.i32(x);
}

static uint64_t hcold(const ColdStartOut& o) {
    Hash h;
    const ClockInputs& c = o.clock;
    hclock(h, c.state); hhead(h, c.head); hrecord(h, c.rodTemplate);
    for (const OrbRing& r : c.orbs.rings) { h.i32(r.head); h.i32(r.count); h.i32(r.full); for (const OrbEntry& e : r.entries) { h.f32(e.x); h.f32(e.y); h.f32(e.z); hc(h, e.colour); } }
    hramp(h, c.orbs.spriteFade); h.f32(c.orbs.fraction);
    hc(h, c.clearColour); hc(h, c.firstDisplayClear);
    for (float x : {c.tube.near, c.tube.turn, c.tube.scroll, c.tube.ripple, c.tube.scrollEnd, c.tube.turnEnd, c.tube.far, c.tube.radius}) h.f32(x);
    h.f32(c.minuteFactor); h.f32(c.fractionEasing); hc(h, c.orbColour); h.boolean(c.hasEntryData); h.i32(c.wide);
    for (int32_t x : c.orbRandom) h.i32(x);
    for (const Colour& x : c.orbColours) hc(h, x);
    h.i32(c.width); h.i32(c.height); h.boolean(c.font != nullptr); h.boolean(c.program != nullptr);
    hcache(h, c.text.cache); hstate(h, c.text.font); hramps(h, c.text.ramps);
    h.i32(c.text.settings.timeFormat); h.i32(c.text.settings.dateFormat); h.u32(c.text.settings.settingsWord); h.i32(c.text.settings.videoMode);
    const MenusInputs& mi = *c.menus;
    const MenusState& m = mi.menus;
    for (int32_t x : {m.page.word0, m.page.count, m.page.titleWidth, m.page.selected, m.page.word14, m.page.level, m.page.word2c, m.page.word30, m.page.glow}) h.i32(x);
    h.u32(m.page.entries); hramp(h, m.page.ramp);
    for (const ConfigEntry& e : m.entries) hentry(h, e);
    for (int32_t x : {m.mainMenu.word0, m.mainMenu.count, m.mainMenu.word0c, m.mainMenu.selected, m.mainMenu.word14}) h.i32(x);
    h.u32(m.mainMenu.items); hramp(h, m.mainMenu.ramp);
    hramp(h, m.versionRamp); hramp(h, m.dialogRamp); hramp(h, m.firstRunRamp);
    hversion(h, m.version);
    h.u32(uint32_t(m.versionList.size())); for (const VersionRow& r : m.versionList) hrow(h, r);
    hversion(h, m.versionDrawn); hramp(h, m.versionRampDrawn); h.boolean(m.versionDrawnValid);
    for (int32_t x : m.pagePointers) h.i32(x);
    h.i32(m.entryActive); for (int32_t x : m.menuLengths) h.i32(x);
    h.f32(m.listConstants.rate); h.f32(m.listConstants.divisor); h.f32(m.listConstants.pulse); h.f32(m.listConstants.standing);
    h.i32(m.screenCode);
    for (const AdjustField& f : m.adjustFields) { h.i32(f.item); h.i32(f.lowest); h.i32(f.highest); }
    h.boolean(m.configGate.has_value()); h.i32(m.configGate.value_or(0));
    h.i32(m.listFade.first); h.i32(m.listFade.second); h.i32(m.listFade.secondIndex); h.i32(m.body); h.i32(m.videoMode);
    const CubeState& q = mi.cubes;
    hramp(h, q.ramp); h.f32(q.list.pulse); h.i32(q.list.pulsed); h.i32(q.list.position); h.i32(q.list.left); h.i32(q.list.speed); h.i32(q.list.slowing);
    hc(h, q.colours.selected); hc(h, q.colours.plain); hc(h, q.colours.live); hrecord(h, q.record);
    for (float x : q.constants) h.f32(x);
    h.f32(q.centreFactors[0]); h.f32(q.centreFactors[1]); hm(h, q.view); hm(h, q.screen); hc(h, q.layerClear); hrect(h, q.added); hrect(h, q.half); hrect(h, q.chain);
    for (int32_t x : mi.items) h.i32(x);
    h.boolean(mi.options.browserEnters);
    const FrameInputs& f = o.frame;
    h.f32(f.time.milliseconds); h.i32(f.time.seconds); h.i32(f.time.minutes); h.i32(f.time.hours);
    h.i32(f.field); h.i32(f.displayIndex); h.i32(f.item0);
    for (int32_t x : {f.items.year, f.items.month, f.items.day, f.items.hour, f.items.minute, f.items.second}) h.i32(x);
    h.i32(f.menu.disc); h.boolean(f.menu.configDirty.has_value()); for (int32_t x : *f.menu.configDirty) h.i32(x);
    for (int32_t x : *f.menu.rtcMirror) h.i32(x);
    h.u32(f.menu.mechaconParam->at(0)); h.u32(f.menu.mechaconParam->at(1));
    for (int32_t x : *f.configItems) h.i32(x);
    h.boolean(f.timeFilled.has_value()); h.boolean(f.threadStep);
    return h.h;
}

static ColdInputs coldCase(Rng& r, int i) {
    ColdInputs c;
    c.pal = r.next32() & 1;
    c.language = int(r.next32() % 9);
    if (i % 17 == 0) c.language = int(r.next32() % 32);
    c.city = int(r.next32() % 0x90);
    c.aspect = int(r.next32() % 4);
    c.timeZone = int(r.next32() % 1501) - 720;
    c.summerTime = r.next32() & 1;
    c.dateFormat = int(r.next32() % 4);
    c.timeFormat = int(r.next32() % 2);
    c.rtc = {int32_t(2000 + r.next32() % 100), int32_t(1 + r.next32() % 12), int32_t(1 + r.next32() % 28), int32_t(r.next32() % 24), int32_t(r.next32() % 60), int32_t(r.next32() % 60)};
    if (i % 5 == 0) c.rtc = {int32_t(2000 + 99 * (r.next32() & 1)), int32_t(1 + 11 * (r.next32() & 1)), int32_t(1 + 30 * (r.next32() & 1) - (r.next32() & 1)), int32_t(23 * (r.next32() & 1)), int32_t(59 * (r.next32() & 1)), int32_t(59 * (r.next32() & 1))};
    c.randState = r.next32();
    c.screenWidth = 640;
    c.screenHeight = c.pal ? 0x100 : 0xE0;
    c.wide = r.next32() & 1;
    c.screenCode = r.next32() % 0x80;
    c.field = r.next32() & 1;
    c.gsAllocator = r.next32() % 0x100000;
    return c;
}

template <class A>
void run(const char* tag, const ColdAssets& withFont, const ColdAssets& noFont, int fontCases, int plainCases) {
    Hash all, font;
    Rng r(0xc01d5747);
    for (int i = 0; i < plainCases; ++i) all.u64(hcold(coldStart<A>(noFont, coldCase(r, i))));
    Rng rf(0xf0e75747);
    for (int i = 0; i < fontCases; ++i) {
        const ColdInputs c = coldCase(rf, i);
        font.u64(hcold(coldStart<A>(withFont, c)));
        const ColdFontOut fo = coldFont<A>(withFont.program, withFont.font, c.pal, c.gsAllocator);
        Hash g; hcache(g, fo.cache); hstate(g, fo.state); for (int32_t w : fo.widths) g.i32(w);
        font.u64(g.h);
    }
    report(std::string(tag) + " cold", all);
    report(std::string(tag) + " cold font", font);
}

int main(int argc, char** argv) {
    const ColdAssets withFont = coldAssets();
    ColdAssets noFont = withFont;
    noFont.font = nullptr;
    const int fontCases = argc > 1 ? std::atoi(argv[1]) : 64, plainCases = argc > 2 ? std::atoi(argv[2]) : 20000;
    run<NativeArithmetic>("native", withFont, noFont, fontCases, plainCases);
#ifndef SCENE_NATIVE_ONLY
    run<EeArithmetic>("ee", withFont, noFont, fontCases, plainCases);
#endif
}
