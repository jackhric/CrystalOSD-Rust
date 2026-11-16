#include <cstdio>
#include <cstring>
#include "session.hpp"

static void htext(Hash& h, const TextFrame& f) {
    h.u32(uint32_t(f.draws.size()));
    for (const TextDraw& d : f.draws) {
        h.i32(int(d.kind)); h.i32(d.texture);
        for (int32_t x : d.glyph.region) h.i32(x);
        for (const GlyphVertex& v : d.glyph.fan) { h.f32(v.x); h.f32(v.y); h.f32(v.s); h.f32(v.t); h.f32(v.q); for (uint8_t c : v.colour) h.u32(c); }
        const Rect& r = d.icon;
        for (int32_t x : r.colour) h.i32(x);
        for (int32_t x : {r.x0, r.y0, r.u0, r.v0, r.x1, r.y1, r.u1, r.v1, r.z, r.blend, r.textured}) h.i32(x);
    }
    h.u32(uint32_t(f.strings.size()));
    for (const StringRun& s : f.strings) { hbytes(h, s.text); h.boolean(s.measuring); hstate(h, s.own); }
    h.u32(f.glyphs.address); h.i32(f.glyphs.cellWidth); h.i32(f.glyphs.cellHeight); h.i32(f.glyphs.width); h.i32(f.glyphs.logWidth); h.i32(f.glyphs.logHeight);
    h.u32(uint32_t(f.glyphs.cells.size())); for (int32_t c : f.glyphs.cells) h.i32(c);
}

struct Gen {
    Rng r;
    uint32_t u(uint32_t n) { return r.next32() % n; }
    Ramp ramp(int32_t length) {
        const int32_t state = int32_t(u(4));
        const int32_t counter = state == 0 ? 0 : state == 2 ? length : int32_t(u(uint32_t(length) + 1));
        return {length, counter, int32_t(u(2)), state};
    }
    TextRamps ramps() {
        TextRamps t;
        t.config = ramp(1 + int32_t(u(100)));
        t.mainMenu = ramp(1 + int32_t(u(20)));
        t.version = ramp(1 + int32_t(u(20)));
        if (u(6) == 0) t.dialogClosing = ramp(1 + int32_t(u(20)));
        if (u(6) == 0) t.firstRun = ramp(1 + int32_t(u(20)));
        t.dialog = ramp(1 + int32_t(u(20)));
        t.lead = int32_t(u(50));
        t.body = int32_t(u(50));
        t.panel7 = u(5) == 0 ? 1 : 0;
        t.panel8On = u(3) == 0 ? 1 : 0;
        t.panel8 = int32_t(u(129));
        t.adjustRow = int32_t(u(2));
        static const std::array<int32_t, 4> ids[4] = {{1, 1, 1, 1}, {0x5e, 0x55, 0x56, 0x57}, {1, 0x55, 1, 0x57}, {0x5e, 1, 0x56, 1}};
        t.panel8Ids = ids[u(4)];
        return t;
    }
};

static ColdInputs textCold(int language, bool pal, int timeFormat, int dateFormat, int aspect, bool summer) {
    ColdInputs c;
    c.pal = pal; c.language = language; c.timeFormat = timeFormat; c.dateFormat = dateFormat; c.aspect = aspect; c.summerTime = summer;
    c.rtc = {2026, 3, 4, 5, 6, 7}; c.screenHeight = pal ? 0x100 : 0xE0; c.gsAllocator = 770048;
    return c;
}

template <class A>
void run(const char* tag, const ColdAssets& assets) {
    // Every string of each language's table that resolves, drawn then measured, the cache carried through the language.
    Hash strings;
    for (int pal = 0; pal < 2; ++pal)
        for (int language = 1; language < 8; ++language) {
            ColdStartOut cold = coldStart<A>(assets, textCold(language, pal, 0, 0, 0, false));
            std::optional<Text<A>> text;
            text.emplace(assets.font, assets.program, cold.clock.text);
            const uint32_t table = assets.program->word(0x002ad200 + 4u * uint32_t(language));
            for (uint32_t id = 0; id < 0x240; ++id) {
                std::string s;
                try { s = assets.program->string(assets.program->word(table + 4 * id)); } catch (const std::exception&) { strings.u32(0xdead); continue; }
                try {
                    htext(strings, text->drawString(s, 640, pal ? 256 : 224));
                } catch (const std::exception&) {
                    strings.u32(0xbad);
                    text.emplace(assets.font, assets.program, cold.clock.text);
                }
                strings.i32(text->stringWidth(s));
            }
            hstate(strings, text->font());
        }
    report(std::string(tag) + " strings", strings);

    Hash frames;
    Gen g{Rng(0x7e47f4a3)};
    for (int c = 0; c < 140; ++c) {
        const bool pal = g.u(2);
        const int language = 1 + int(g.u(7)), timeFormat = int(g.u(2)), dateFormat = int(g.u(3)), aspect = int(g.u(3));
        const bool summer = g.u(2);
        ColdStartOut cold = coldStart<A>(assets, textCold(language, pal, timeFormat, dateFormat, aspect, summer));
        TextInputs inputs = cold.clock.text;
        if (g.u(4) == 0) inputs.ramps.config.state = 2;
        Text<A> text(assets.font, assets.program, inputs);
        const MenusState& m0 = cold.clock.menus->menus;
        const int32_t height = pal ? 256 : 224;
        for (int k = 0; k < 24; ++k) {
            PagesInputs p;
            p.mainMenu = m0.mainMenu;
            p.mainMenu.selected = int32_t(g.u(uint32_t(m0.mainMenu.count)));
            p.mainMenu.ramp = g.ramp(1 + int32_t(g.u(20)));
            if (g.u(2)) p.mainMenu.ramp.state = 2;
            p.page = m0.page;
            p.page.selected = int32_t(g.u(uint32_t(m0.page.count)));
            p.page.level = int32_t(g.u(3));
            p.page.glow = int32_t(g.u(2 * 0x7aa8)) - 0x7aa8;
            p.entries = m0.entries;
            for (ConfigEntry& e : p.entries) e.valueIndex = e.valueCount > 0 ? int32_t(g.u(uint32_t(e.valueCount))) : 0;
            p.listFade = {int32_t(g.u(129)), g.u(2) ? 0 : int32_t(g.u(129)), int32_t(g.u(uint32_t(m0.page.count)))};
            p.listConstants = m0.listConstants;
            p.adjustFields = m0.adjustFields;
            p.items = cold.clock.menus->items;
            p.items[0] = int32_t(g.u(3)); p.items[6] = 2000 + int32_t(g.u(100)); p.items[7] = 1 + int32_t(g.u(12)); p.items[8] = 1 + int32_t(g.u(31));
            p.items[9] = int32_t(g.u(24)); p.items[10] = int32_t(g.u(60)); p.items[11] = int32_t(g.u(60)); p.items[0xd] = int32_t(g.u(2)); p.items[0xe] = int32_t(g.u(3));
            p.menuRamp = g.ramp(1 + int32_t(g.u(30)));
            p.tail = 1 + int32_t(g.u(20));
            p.overlayLevel = int32_t(g.u(129));
            p.ramps = g.ramps();
            if (g.u(2)) p.ramps.config.state = 2;
            p.width = 640; p.height = height;
            p.pending = g.u(5) == 0 ? 1 : 0;
            p.version = m0.version;
            for (size_t i = 0; i < m0.versionList.size(); ++i) p.version.rows[i] = m0.versionList[i];
            p.version.count = int32_t(g.u(4));
            p.version.shown = 1 + int32_t(g.u(6));
            p.version.selected = int32_t(g.u(4));
            p.version.first = int32_t(g.u(3));
            if (g.u(3) == 0) p.version.rows[g.u(3)].value.clear();
            if (g.u(3) == 0) p.version.rows[g.u(3)].label = 0;
            const PagesFrame pf = text.pages(p);
            htext(frames, pf.text);
            frames.boolean(pf.titleWidth.has_value()); frames.i32(pf.titleWidth.value_or(0));
            frames.u32(uint32_t(pf.unmodelled.size())); for (uint32_t x : pf.unmodelled) frames.u32(x);
            TextFrameInputs t;
            const bool negativeYear = g.u(8) == 0, negativeHour = g.u(8) == 0;
            t.items = {negativeYear ? -1 : 2000 + int32_t(g.u(100)), 1 + int32_t(g.u(12)), 1 + int32_t(g.u(31)), negativeHour ? -1 : int32_t(g.u(24)), int32_t(g.u(60)), int32_t(g.u(60))};
            t.item0 = int32_t(g.u(3));
            t.overlayLevel = int32_t(g.u(129));
            t.tail = 1 + int32_t(g.u(20));
            t.menu = g.ramp(1 + int32_t(g.u(30)));
            t.width = 640; t.height = height;
            if (g.u(4) != 0) t.ramps = g.ramps();
            htext(frames, text.frame(t));
            hcache: { const FontCache& fc = text.cache(); frames.u32(uint32_t(fc.list.size())); for (const FontCacheEntry& e : fc.list) { frames.i32(e.code); frames.i32(e.loaded); frames.i32(e.cell); frames.u32(e.block); } }
            hstate(frames, text.font());
        }
    }
    report(std::string(tag) + " text frames", frames);
}

int main() {
    const ColdAssets assets = coldAssets();
    run<NativeArithmetic>("native", assets);
#ifndef SCENE_NATIVE_ONLY
    run<EeArithmetic>("ee", assets);
#endif
}
