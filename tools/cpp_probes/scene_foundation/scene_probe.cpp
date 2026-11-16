#include <cmath>
#include <fstream>
#include <iterator>
#include <optional>

#include "Scene/Arithmetic.hpp"
#include "Scene/Camera.hpp"
#include "Scene/ClockDate.hpp"
#include "Scene/ClockState.hpp"
#include "Scene/ConsoleClock.hpp"
#include "Scene/ElfRecords.hpp"
#include "Scene/Font.hpp"
#include "Scene/FrameHead.hpp"
#include "Scene/Orbs.hpp"
#include "Scene/ProgramImage.hpp"
#include "Scene/Rods.hpp"
#include "common.hpp"

using namespace scene;

static const char* kResources = "../CrystalOSD/Resources/";

void h4(Hash& h, const Vec4& v) { for (float x : v) h.f32(x); }
void hm(Hash& h, const Mat4& m) { for (const Vec4& r : m) h4(h, r); }
void hc(Hash& h, const Colour& c) { for (int32_t x : c) h.i32(x); }
void hr(Hash& h, const Ramp& r) { h.i32(r.length); h.i32(r.counter); h.i32(r.changed); h.i32(r.state); }
void hrect(Hash& h, const Rect& r) {
    hc(h, r.colour);
    for (int32_t x : {r.x0, r.y0, r.u0, r.v0, r.x1, r.y1, r.u1, r.v1, r.z, r.blend, r.textured}) h.i32(x);
}

void hclock(Hash& h, const ClockState& s) {
    h.f32(s.time.milliseconds); h.i32(s.time.seconds); h.i32(s.time.minutes); h.i32(s.time.hours);
    h.i32(s.eased.secondHand); h.i32(s.eased.hourHand); h.f32(s.eased.progress); h.f32(s.eased.fraction);
    h.i32(s.state.currentRod); h.i32(s.state.secondsAngle); h.i32(s.state.rodAngle);
    for (const RodState& r : s.state.rods) { h.f32(r.appearance); h.f32(r.progress); hc(h, r.base); hc(h, r.reflection); }
    hc(h, s.state.accent); hc(h, s.state.fourth); h.f32(s.state.progressTarget); hc(h, s.state.accentFrame); hc(h, s.state.currentTarget);
    hr(h, s.appearance);
    hc(h, s.colours.base); hc(h, s.colours.reflection); hc(h, s.colours.fourth); hc(h, s.colours.currentReflection);
    h.i32(s.cycleCounters.base.index); h.i32(s.cycleCounters.base.wait); h.i32(s.cycleCounters.accent.index); h.i32(s.cycleCounters.accent.wait); h.i32(s.cycleCounters.last);
    for (const Colour& c : s.cycleTables.base) hc(h, c);
    for (const Colour& c : s.cycleTables.accent) hc(h, c);
    h.f32(s.logicConstants.rodEasing); h.f32(s.logicConstants.secondsEasing); h.f32(s.logicConstants.progressStep);
    h.f32(s.scene.scale); h.i32(s.scene.leaving); h.i32(s.scene.field);
    h.f32(s.scaleTarget); h.f32(s.scaleFactor); h.i32(s.timeFilled); h.boolean(s.spin.has_value()); h.u32(s.spin.value_or(0));
    h.i32(s.mode); h.i32(s.level); h.i32(s.overlayLevel); hr(h, s.vignetteRamp); h.i32(s.vignetteLength); hr(h, s.menuRamp); h.i32(s.tail); h.i32(s.counter);
    h.f32(s.proportions.ax); h.f32(s.proportions.ay); h4(h, s.position); h4(h, s.direction); h4(h, s.up); h4(h, s.rotation);
    h.f32(s.cameraOffset); h.f32(s.zmax); h.f32(s.cameraFactor);
}

void hhead(Hash& h, const HeadState& s) {
    hr(h, s.greyRamp); for (int32_t g : s.greys) h.i32(g); hr(h, s.vignetteRamp);
    for (int32_t x : {s.ring.alpha, s.ring.cx, s.ring.cy, s.ring.rx, s.ring.ry, s.ring.z}) h.i32(x);
    for (const Rect* r : {&s.tint, &s.blur, &s.copy, &s.fade, &s.bars, &s.column}) hrect(h, *r);
}

void hdraws(Hash& h, const std::vector<HeadDraw>& draws) {
    h.u32(static_cast<uint32_t>(draws.size()));
    for (const HeadDraw& d : draws) {
        for (int x : {int(d.part), int(d.target), int(d.source), int(d.alpha), d.depthTest, int(d.topology), int(d.textured), int(d.blended), int(d.gouraud), int(d.coordinates)}) h.i32(x);
        h.u32(static_cast<uint32_t>(d.vertices.size()));
        for (const HeadVertex& v : d.vertices) {
            h.f32(v.x); h.f32(v.y); h.u32(v.z); h.u32(v.fog);
            for (float x : {v.r, v.g, v.b, v.a, v.q, v.s, v.t, v.u, v.v}) h.f32(x);
        }
    }
}

void hrecord(Hash& h, const RodRecord& r) {
    h.i32(r.number); h.i32(r.faces); hm(h, r.local); h.f32(r.sx); h.f32(r.sy); h.f32(r.sz); hc(h, r.base); h.f32(r.strength); hc(h, r.textured);
    h.f32(r.pair[0]); h.f32(r.pair[1]); h.f32(r.refraction); hc(h, r.reflection); hc(h, r.extra);
}

void htransform(Hash& h, const RodTransform& t) {
    h.f32(t.cx); h.f32(t.cy); h.f32(t.cz); h.u32(static_cast<uint32_t>(t.faces.size()));
    for (const RodFace& f : t.faces) {
        h.i32(f.index); h4(h, f.normal); h.i32(f.flag);
        for (const RodFaceVertex& v : f.vertices) { h4(h, v.eye); h4(h, v.screen); h.f32(v.q); h.f32(v.s); h.f32(v.t); for (int32_t x : v.fixed) h.i32(x); }
    }
}

void hsend(Hash& h, const RodSend& s) {
    h.i32(int(s.kind)); h.u32(static_cast<uint32_t>(s.faces.size()));
    for (const RodFaceDraw& d : s.faces) {
        h.i32(d.face); h.i32(int(d.piece)); h.boolean(d.edgeSmoothing);
        for (const RodVertex& v : d.strip) for (float x : {v.x, v.y, v.z, v.u, v.v, v.q, v.r, v.g, v.b, v.a}) h.f32(x);
    }
}

void hrods(Hash& h, const RodsFrame& f) {
    h.u32(static_cast<uint32_t>(f.list.size()));
    for (const DepthNode& n : f.list) { h.boolean(n.orb); h.i32(n.index); h.f32(n.key); }
    h.u32(static_cast<uint32_t>(f.rods.size()));
    for (const Rod& r : f.rods) {
        h.i32(r.placement); hrecord(h, r.record); h.f32(r.t); h.f32(r.key); hm(h, r.matrix); h.boolean(r.drawn); htransform(h, r.whole); h.f32(r.cx); h.f32(r.cy);
        h.u32(static_cast<uint32_t>(r.pieces.size()));
        for (const RodPieceRecord& p : r.pieces) { h.i32(int(p.piece)); hrecord(h, p.record); htransform(h, p.transform); }
        h.u32(static_cast<uint32_t>(r.sends.size()));
        for (const RodSend& s : r.sends) hsend(h, s);
    }
    for (const auto& pass : f.extraPasses) {
        h.u32(static_cast<uint32_t>(pass.size()));
        for (const RodExtraDraw& d : pass) { h.u64(d.rod); h.boolean(d.split); h.i32(d.grainTexture); hsend(h, d.reflection); hsend(h, d.grain); }
    }
}

void hov(Hash& h, const OrbVertex& v) { for (float x : {v.x, v.y, v.z, v.u, v.v, v.q, v.r, v.g, v.b, v.a}) h.f32(x); }

void horbs(Hash& h, const OrbFrame& f, const OrbState& s) {
    for (const OrbDraw& o : f.orbs) {
        h.i32(o.k); h.i32(o.drawn); h.f32(o.depthKey); hm(h, o.local); h.f32(o.cx); h.f32(o.cy); h.f32(o.cz);
        for (const OrbSend& send : o.sends) {
            h.u32(static_cast<uint32_t>(send.trail.points.size()));
            for (const OrbVertex& v : send.trail.points) hov(h, v);
            h.f32(send.trail.headerAlpha);
            hov(h, send.glow.first); hov(h, send.glow.second); hov(h, send.disc.first); hov(h, send.disc.second);
        }
    }
    for (int x : f.order) h.i32(x);
    for (const OrbRing& r : s.rings) {
        h.i32(r.head); h.i32(r.count); h.i32(r.full);
        for (const OrbEntry& e : r.entries) { h.f32(e.x); h.f32(e.y); h.f32(e.z); hc(h, e.colour); }
    }
    hr(h, s.spriteFade); h.f32(s.fraction);
}

std::vector<uint8_t> readFile(const std::string& name) {
    std::ifstream in(kResources + name, std::ios::binary);
    return std::vector<uint8_t>((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
}

RodMesh meshOf(const ProgramImage& p) {
    RodMesh m;
    for (uint32_t i = 0; i < 64; ++i) m.positions.push_back(vec4At(p, 0x002b4b90 + 16 * i));
    for (uint32_t i = 0; i < 16; ++i) m.normals.push_back(vec4At(p, 0x002b5390 + 16 * i));
    for (uint32_t i = 0; i < 64; ++i) m.coordinates.push_back(vec4At(p, 0x002b4f90 + 16 * i));
    return m;
}

struct Scenario {
    int mode;
    int wide;
    int item0;
    bool pal;
    uint32_t seed;
    int frames;
    bool jumps;
};

const Scenario kScenarios[] = {
    {0, 0, 0, false, 1, 900, false}, {2, 0, 2, false, 2, 400, false}, {2, 1, 0, true, 3, 400, false},
    {3, 0, 1, false, 4, 400, false}, {1, 0, 0, true, 5, 300, true},  {0, 1, 2, false, 6, 300, true},
};

ClockState clockOf(const ProgramImage& p, const Scenario& sc, Rng& r) {
    ClockState s;
    s.logicConstants = {p.single(0x36FBCC), p.single(0x36FBCC + 4), p.single(0x36FBCC + 8)};
    for (uint32_t i = 0; i < 6; ++i) s.cycleTables.base[i] = colourAt(p, 0x2B5570 + 16 * i);
    for (uint32_t i = 0; i < 3; ++i) s.cycleTables.accent[i] = colourAt(p, 0x2B5570 + 0x60 + 16 * i);
    s.colours = {colourAt(p, 0x2B5600), colourAt(p, 0x2B5610), colourAt(p, 0x2B5620), colourAt(p, 0x2B5630)};
    s.eased.secondHand = static_cast<int16_t>(r.next32());
    s.eased.hourHand = static_cast<int16_t>(r.next32());
    s.eased.progress = static_cast<float>(r.next32() % 1024) / 1024.0f;
    s.eased.fraction = static_cast<float>(r.next32() % 1024) / 1024.0f;
    for (RodState& rod : s.state.rods) rod = {0.0f, 0.0f, colourAt(p, 0x2B5650), colourAt(p, 0x2B5650)};
    s.state.secondsAngle = static_cast<int16_t>(r.next32());
    s.state.rodAngle = static_cast<int16_t>(r.next32());
    s.state.accent = colourAt(p, 0x2B5660);
    s.state.fourth = colourAt(p, 0x2B5660);
    s.appearance = {60, 0, 0, 1};
    s.scaleTarget = p.single(0x370294);
    s.scaleFactor = p.single(0x36FC10);
    s.timeFilled = 1;
    s.spin = static_cast<uint16_t>(r.next32());
    s.mode = sc.mode;
    s.overlayLevel = sc.mode == 3 ? 0x80 : 0;
    s.vignetteRamp = {30, 0, 0, 0};
    s.vignetteLength = 30;
    s.menuRamp = {40, 0, 0, 1};
    s.tail = 10;
    s.proportions = {1.0f, p.single(sc.pal ? 0x36FB84 : 0x36FB88)};
    s.position = vec4At(p, 0x2B2190);
    s.direction = vec4At(p, 0x2B21A0);
    s.up = vec4At(p, 0x2B21B0);
    s.rotation = vec4At(p, 0x2B21C0);
    s.cameraOffset = asFloat(0xC2C80000u);
    s.zmax = p.single(0x36FB8C);
    s.cameraFactor = p.single(0x36FB90);
    return s;
}

HeadState headOf(const ProgramImage& p) {
    HeadState h;
    h.greyRamp = {40, 0, 0, 1};
    for (uint32_t i = 0; i < 3; ++i) h.greys[i] = p.integer(0x370AA8 + 4 * i);
    h.vignetteRamp = {30, 0, 0, 1};
    h.ring = {p.integer(0x2B5F70), p.integer(0x2B5F74), p.integer(0x2B5F78), p.integer(0x2B5F7C), p.integer(0x2B5F80), p.integer(0x2B5F84)};
    h.tint = rectAt(p, 0x2B21E0);
    h.blur = rectAt(p, 0x2B6120);
    h.copy = rectAt(p, 0x2B6060);
    h.fade = rectAt(p, 0x2B5F30);
    h.bars = rectAt(p, 0x2B2220);
    h.column = rectAt(p, 0x2B2500);
    return h;
}

template <class A>
void world(const ProgramImage& p, const Scenario& sc, int index, const char* tag) {
    Rng r(sc.seed);
    ClockState s = clockOf(p, sc, r);
    FrameHead<A> head(headOf(p));
    Rods<A> rods(meshOf(p), rodRecordAt(p, 0x2B5490));
    const TubeConstants tube{p.single(0x36FC14), p.single(0x36FC18), p.single(0x36FC1C), p.single(0x36FC20), p.single(0x36FC24), p.single(0x36FC28), p.single(0x36FC2C), p.single(0x36FC30)};
    OrbState orbState;
    orbState.spriteFade = {256, 0, 0, 1};
    orbState.fraction = s.eased.fraction;
    std::array<int32_t, kOrbCount> random{};
    for (int32_t& a : random) a = static_cast<int32_t>(r.next32() % 65536u);
    std::array<std::array<int32_t, 4>, kOrbCount> orbColours{};
    for (uint32_t k = 0; k < kOrbCount; ++k) orbColours[k] = colourAt(p, 0x2B5680 + 16 * k);
    const int32_t width = 640, height = sc.pal ? 512 : 448;
    uint64_t micro = static_cast<uint64_t>(r.next32()) * 1000u;
    Hash all;
    for (int frame = 0; frame < sc.frames; ++frame) {
        Hash h;
        micro += sc.jumps ? r.next32() % 4000000000u : 16683u;
        const uint64_t ms = micro / 1000u;
        s.time = {static_cast<float>(micro % 1000000u) / 1000.0f, static_cast<int32_t>(ms / 1000u % 60u), static_cast<int32_t>(ms / 60000u % 60u),
                  static_cast<int32_t>(ms / 3600000u % 24u)};
        s.scene.field = frame & 1;
        tickRamp(s.menuRamp);
        const CameraMatrices cam = Camera<A>::matrices(s);
        hm(h, cam.view);
        hm(h, cam.screen);
        HeadInputs in;
        in.width = width;
        in.height = height;
        in.mode = s.mode;
        in.overlayLevel = s.overlayLevel;
        in.level = static_cast<uint32_t>(s.level);
        in.counter = s.counter;
        in.item0 = sc.item0;
        in.proportionX = s.proportions.ax;
        in.proportionY = s.proportions.ay;
        in.tube = tube;
        hdraws(h, head.head(in, cam.view, cam.screen));
        RodsInput ri;
        ri.currentRod = s.state.currentRod;
        ri.secondsAngle = s.state.secondsAngle;
        ri.rodAngle = s.state.rodAngle;
        for (int i = 0; i < 12; ++i) ri.rods[i] = {s.state.rods[i].appearance, s.state.rods[i].progress, s.state.rods[i].base, s.state.rods[i].reflection};
        ri.accent = s.state.accent;
        ri.fourth = s.state.fourth;
        ri.width = width;
        ri.height = height;
        ri.field = s.scene.field;
        hrods(h, rods.frame(ri, cam.view, cam.screen));
        hrecord(h, rods.rodTemplate());
        OrbInputs oi;
        oi.milliseconds = s.time.milliseconds;
        oi.seconds = s.time.seconds;
        oi.minutes = s.time.minutes;
        oi.secondHand = s.eased.secondHand;
        oi.hourHand = s.eased.hourHand;
        oi.progress = s.eased.progress;
        oi.minuteFactor = p.single(0x36FBDC);
        oi.fractionEasing = p.single(0x36FBE0);
        oi.sceneScale = s.scene.scale;
        oi.colour = colourAt(p, 0x2B5670);
        oi.width = width;
        oi.height = height;
        oi.mode = s.mode;
        oi.overlayLevel = s.overlayLevel;
        oi.hasEntryData = true;
        oi.wide = sc.wide;
        oi.random = random;
        oi.colours = orbColours;
        const OrbFrame of = Orbs<A>::frame(orbState, oi, cam.view, cam.screen);
        horbs(h, of, orbState);
        hdraws(h, head.overlay(in));
        hdraws(h, head.tripsAfter(in));
        hdraws(h, head.bars(in));
        hdraws(h, head.column(in));
        hhead(h, head.state());
        ClockLogic<A>::step(s);
        hclock(h, s);
        all.u64(h.h);
        if (std::getenv("PROBE_FRAMES")) std::printf("%s world%d frame %d 0x%016llx\n", tag, index, frame, (unsigned long long)h.h);
    }
    report(std::string(tag) + " world" + std::to_string(index), all);
}

template <class A>
void matrices(const char* tag) {
    using M = Matrix<A>;
    Rng r(0x3a7a1c00ull);
    auto value = [&] { return asF((r.next32() & 0x807fffffu) | ((100u + r.next32() % 54u) << 23)); };
    auto vec = [&] { return Vec4{value(), value(), value(), value()}; };
    auto mat = [&] { return Mat4{vec(), vec(), vec(), vec()}; };
    Hash h;
    for (int i = 0; i < 20000; ++i) {
        const Mat4 a = mat(), b = mat();
        const Vec4 v = vec();
        hm(h, M::multiply(a, b));
        h4(h, M::apply(a, v));
        const int32_t angle = static_cast<int32_t>(r.next32());
        hm(h, M::rotateX(a, angle));
        hm(h, M::rotateY(a, angle));
        hm(h, M::rotateZ(a, angle));
        hm(h, M::move(a, v[0], v[1], v[2]));
        const Vec3 n = M::normalize({v[0], v[1], v[2]});
        for (float x : n) h.f32(x);
        h.f32(M::dot(n, {v[1], v[2], v[3]}));
        hm(h, M::viewScreen(value(), value(), value(), value(), value(), value(), value(), value(), value()));
        const Vec4 rotation{static_cast<float>(static_cast<int32_t>(r.next32() % 8001u) - 4000) / 1000.0f,
                            static_cast<float>(static_cast<int32_t>(r.next32() % 8001u) - 4000) / 1000.0f,
                            static_cast<float>(static_cast<int32_t>(r.next32() % 8001u) - 4000) / 1000.0f, 0.0f};
        hm(h, M::viewMatrix(vec(), vec(), vec(), rotation));
    }
    report(std::string(tag) + " matrices", h);
}

template <class A>
void guarded(const ProgramImage& p, const char* tag) {
    Rng r(0x9a4dedull);
    Hash h;
    Scenario sc{0, 0, 0, false, 7, 0, false};
    ClockState s = clockOf(p, sc, r);
    for (int i = 0; i < 200000; ++i) {
        s.time = {static_cast<float>(r.next32() % 1000000u) / 1000.0f, static_cast<int32_t>(r.next32() % 60u), static_cast<int32_t>(r.next32() % 60u),
                  static_cast<int32_t>(r.next32() % 24u)};
        if (i % 1000 == 0) s.time = {asF(r.next32()), static_cast<int32_t>(r.next32()), static_cast<int32_t>(r.next32()), static_cast<int32_t>(r.next32())};
        if (i % 7 == 0) { s.state.secondsAngle = static_cast<int16_t>(r.next32()); s.eased.hourHand = static_cast<int16_t>(r.next32()); }
        float hours, seconds;
        {
            std::optional<EeRounding> g;
            if (std::is_same_v<A, NativeArithmetic>) g.emplace();
            hours = ClockLogic<A>::hours(s.time);
            seconds = ClockLogic<A>::seconds(s.time);
            ClockLogic<A>::angles(s);
            ClockLogic<A>::colours(s);
            ClockLogic<A>::scale(s);
        }
        h.f32(hours);
        h.f32(seconds);
        hclock(h, s);
    }
    report(std::string(tag) + " guarded", h);
}

void dates() {
    Rng r(0xda7e0000ull);
    Hash h;
    for (int i = 0; i < 200000; ++i) {
        const int32_t y = 1600 + static_cast<int32_t>(r.next32() % 800u), mo = 1 + static_cast<int32_t>(r.next32() % 12u), d = 1 + static_cast<int32_t>(r.next32() % 31u);
        const int32_t hh = static_cast<int32_t>(r.next32() % 24u), mi = static_cast<int32_t>(r.next32() % 60u), se = static_cast<int32_t>(r.next32() % 60u);
        const int64_t t = secondsOf(y, mo, d, hh, mi, se);
        h.u64(static_cast<uint64_t>(t));
        for (int32_t x : dateOf(static_cast<int64_t>(r.next() % (1ull << 36)))) h.i32(x);
        const uint32_t word = r.next32();
        const ConsoleZone z = zoneOf(word);
        h.i32(z.minutes);
        h.boolean(z.summer);
        const DateWords items{2000 + static_cast<int32_t>(r.next32() % 100u), mo, d, hh, mi, se};
        for (int32_t x : localOf(items, z)) h.i32(x);
        for (int32_t x : rtcOf(items, z)) h.i32(x);
        for (int32_t x : rtcAt(static_cast<int64_t>(r.next() % 8000000000ull) - 1000000000)) h.i32(x);
        h.u64(static_cast<uint64_t>(unixOf(items)));
        const DateWords check{1990 + static_cast<int32_t>(r.next32() % 120u), mo, d, hh, mi, se};
        const DateCheck c = dateCheck(check, word);
        for (int32_t x : c.items) h.i32(x);
        for (const auto& range : c.ranges) { h.i32(range[0]); h.i32(range[1]); }
    }
    report("dates", h);
}

void program(const ProgramImage& p) {
    Hash h;
    for (uint32_t a = 0x2B2000; a < 0x2B7000; a += 4) { h.u32(p.word(a)); h.i32(p.integer(a)); h.f32(p.single(a)); h.f64(p.doubleAt(a)); }
    for (uint32_t a = 0x2AD200; a < 0x2AE600; a += 4) {
        const uint32_t w = p.word(a);
        h.u32(w);
        if (w >= 0x200000u && w < 0x380000u) {
            try {
                const std::string s = p.string(w);
                h.u32(static_cast<uint32_t>(s.size()));
                for (unsigned char c : s) h.u32(c);
            } catch (const std::exception&) { h.u32(0xffffffffu); }
        }
    }
    hrecord(h, rodRecordAt(p, 0x2B5490));
    hrect(h, rectAt(p, 0x2B21E0));
    hr(h, rampAt(p, 0x2B46B8));
    hm(h, mat4At(p, 0x2B5490 + 0x20));
    report("program", h);
}

void font() {
    const Font f(readFile("FNTOSD"));
    Hash h;
    h.u32(static_cast<uint32_t>(f.blocks().size()));
    for (const FontBlock& b : f.blocks()) {
        for (uint32_t x : {b.at, b.flags, static_cast<uint32_t>(b.depth)}) h.u32(x);
        h.f32(b.scaleX); h.f32(b.scaleY);
        for (int32_t x : {b.width, b.height, b.ascent, b.descent}) h.i32(x);
        for (uint32_t x : {b.glyphs, b.pictures, b.pictureSize, b.index, b.metrics, b.table}) h.u32(x);
        h.boolean(b.perGlyph);
        h.u32(static_cast<uint32_t>(b.ranges.size()));
        for (const FontRange& rg : b.ranges) for (int32_t x : {rg.start, rg.end, rg.first, rg.add}) h.i32(x);
        for (const Rgba& c : f.table(b)) for (uint8_t x : c) h.u32(x);
        h.boolean(f.blockAt(b.at) == &b);
    }
    for (uint32_t o = 0x50; o < 0x58; o += 2) h.i32(f.header16(o));
    for (int32_t code = -2; code < 0x10002; ++code) {
        const std::optional<Glyph> g = f.glyph(code);
        h.boolean(g.has_value());
        if (!g) continue;
        h.u32(g->block->at); h.i32(g->index); h.u32(g->picture);
        for (int32_t m : g->metrics) h.i32(m);
        uint32_t sum = 0;
        for (int32_t y = 0; y < g->block->height; ++y)
            for (int32_t x = 0; x < g->block->width; ++x) sum = sum * 31u + static_cast<uint32_t>(f.pixel(*g, x, y));
        h.u32(sum);
    }
    report("font", h);
}

int main(int argc, char** argv) {
    std::setvbuf(stdout, nullptr, _IOLBF, 0);
    const std::string which = argc > 1 ? argv[1] : "all";
    const ProgramImage p(readFile("hddosd.elf"));
    if (which == "all" || which == "basic") {
        dates();
        program(p);
        font();
        matrices<EeArithmetic>("ee");
        matrices<NativeArithmetic>("native");
        guarded<EeArithmetic>(p, "ee");
        guarded<NativeArithmetic>(p, "truncating");
    }
    if (which == "all" || which == "ee")
        for (int i = 0; i < 6; ++i) world<EeArithmetic>(p, kScenarios[i], i, "ee");
    if (which == "all" || which == "native")
        for (int i = 0; i < 6; ++i) world<NativeArithmetic>(p, kScenarios[i], i, "native");
    return 0;
}
