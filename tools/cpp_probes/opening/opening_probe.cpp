#include <cstring>
#include <exception>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

#include "Scene/Arithmetic.hpp"
#include "Scene/ColdState.hpp"
#include "Scene/ProgramImage.hpp"
#include "Scene/opening/Opening.hpp"
#include "Scene/opening/TowerVu1.hpp"
#include "Scene/opening/Vu0.hpp"
#include "common.hpp"

using namespace scene;
using namespace scene::opening;

static const char* kElf = "/home/jackh/Documents/git/CrystalOSD/Resources/hddosd.elf";

// Canonical mode hashes every NaN bit pattern as 0x7fc00000: when two NaNs meet, which one an SSE instruction returns follows the
// operand order the compiler picked (clang -O2 commutes, -O0 does not), so NaN payloads are not part of the C++'s meaning.
bool gCanonical = false;
uint32_t canonical(uint32_t w) { return gCanonical && (w & 0x7f800000u) == 0x7f800000u && (w & 0x007fffffu) != 0 ? 0x7fc00000u : w; }
void hf(Hash& h, float x) { h.u32(canonical(bitsF(x))); }
void h4(Hash& h, const Vec4& v) { for (float x : v) hf(h, x); }
void hm(Hash& h, const Mat4& m) { for (const Vec4& r : m) h4(h, r); }
void hname(Hash& h, const std::string& s) {
    h.u32(static_cast<uint32_t>(s.size()));
    for (char c : s) h.u32(static_cast<uint8_t>(c));
}
void hvertex(Hash& h, const Vertex& v) {
    hf(h, v.x); hf(h, v.y); h.u32(v.z); hf(h, v.u); hf(h, v.v); hf(h, v.q);
    h.u32(v.r); h.u32(v.g); h.u32(v.b); h.u32(v.a);
}
void hmaterial(Hash& h, const Material& m) {
    h.i32(int(m.source)); h.i32(m.texture); h.i32(int(m.sourceTarget)); h.boolean(m.colourOnly); h.i32(int(m.coordinates)); h.i32(int(m.sampling));
    for (int32_t x : m.region) h.i32(x);
    h.boolean(m.bilinear); h.i32(m.sourceHeight); h.i32(int(m.blend)); h.u32(m.blendConstant); h.i32(int(m.depthTest)); h.boolean(m.depthWrite);
    h.boolean(m.gouraud); h.boolean(m.perPixelAlpha); h.boolean(m.alphaCorrection);
}
void hpasses(Hash& h, const std::vector<Pass>& passes) {
    h.u32(static_cast<uint32_t>(passes.size()));
    for (const Pass& p : passes) {
        hname(h, p.name); h.i32(int(p.target)); h.i32(int(p.topology)); hmaterial(h, p.material);
        h.boolean(p.edgeSmoothing); h.boolean(p.halfLine); h.boolean(p.primBlend); h.boolean(p.scissor.has_value());
        if (p.scissor) for (int32_t x : *p.scissor) h.i32(x);
        h.u32(static_cast<uint32_t>(p.vertices.size()));
        for (const Vertex& v : p.vertices) hvertex(h, v);
    }
}

// Source/App/Golden.cpp hashFrame.
struct Fnv {
    uint64_t h = 0xcbf29ce484222325ull;
    void bytes(const void* p, size_t n) {
        const auto* b = static_cast<const uint8_t*>(p);
        for (size_t i = 0; i < n; ++i) h = (h ^ b[i]) * 0x100000001b3ull;
    }
    template <class T> void value(const T& v) { bytes(&v, sizeof v); }
};
uint64_t hashFrame(const Frame& f) {
    Fnv x;
    x.value(f.width); x.value(f.height); x.value(f.field); x.value(f.displayIndex); x.value(f.textAt);
    x.value(f.textureSet); x.value(f.depthBits);
    x.value(f.glyphs.address); x.value(f.glyphs.cellWidth); x.value(f.glyphs.cellHeight); x.value(f.glyphs.width);
    x.value(f.glyphs.logWidth); x.value(f.glyphs.logHeight);
    x.value(f.glyphs.cells.size()); x.bytes(f.glyphs.cells.data(), f.glyphs.cells.size() * sizeof(int32_t));
    x.value(f.passes.size());
    for (const Pass& p : f.passes) {
        const Material& m = p.material;
        x.value(p.target); x.value(p.topology);
        x.value(m.source); x.value(m.texture); x.value(m.sourceTarget); x.value(m.colourOnly); x.value(m.coordinates);
        x.value(m.sampling); x.value(m.region); x.value(m.bilinear); x.value(m.sourceHeight); x.value(m.blend);
        x.value(m.blendConstant); x.value(m.depthTest); x.value(m.depthWrite); x.value(m.gouraud); x.value(m.perPixelAlpha);
        x.value(m.alphaCorrection);
        x.value(p.edgeSmoothing); x.value(p.halfLine); x.value(p.primBlend); x.value(p.scissor.has_value());
        if (p.scissor) x.value(*p.scissor);
        x.value(p.vertices.size());
        for (const Vertex& v : p.vertices) {
            x.value(std::bit_cast<uint32_t>(v.x)); x.value(std::bit_cast<uint32_t>(v.y)); x.value(v.z);
            x.value(std::bit_cast<uint32_t>(v.u)); x.value(std::bit_cast<uint32_t>(v.v)); x.value(std::bit_cast<uint32_t>(v.q));
            x.value(v.r); x.value(v.g); x.value(v.b); x.value(v.a);
        }
    }
    return x.h;
}

// Inputs: 0 random bits, 1 the edge floats, 2 moderate values (a random int32 / 2^24).
float gen(Rng& r, int kind) {
    if (kind == 0) return asF(r.next32());
    if (kind == 1) return asF(edgeFloats()[r.next() % edgeFloats().size()]);
    return static_cast<float>(static_cast<int32_t>(r.next32())) * (1.0f / 16777216.0f);
}
Vec4 v4(Rng& r, int kind) { return {gen(r, kind), gen(r, kind), gen(r, kind), gen(r, kind)}; }
Mat4 m4(Rng& r, int kind) { return {v4(r, kind), v4(r, kind), v4(r, kind), v4(r, kind)}; }

template <class A>
void vu0(const char* name) {
    using V = Vu0<A>;
    for (int kind = 0; kind < 3; ++kind) {
        Rng r(0x0b0e0000ull + kind);
        Hash h;
        for (int n = 0; n < 4000; ++n) {
            const Mat4 a = m4(r, kind), b = m4(r, kind);
            const Vec4 v = v4(r, kind), w = v4(r, kind), u = v4(r, kind);
            hm(h, V::unitMatrix());
            h4(h, V::apply(a, v));
            hm(h, V::mulMatrix(a, b));
            hm(h, V::rotMatrix(a, v));
            hm(h, V::rotMatrix(w));
            hm(h, V::transMatrix(a, w));
            h4(h, V::subVector(v, w));
            hm(h, V::cameraMatrix(v, w, u));
            float f[9];
            for (float& x : f) x = gen(r, kind);
            hm(h, V::viewScreenMatrix(f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7], f[8]));
            hm(h, V::normalLightMatrix(v, w, u));
            const size_t count = 1 + r.next() % 8;
            std::vector<Vec4> vertices;
            for (size_t i = 0; i < count; ++i) vertices.push_back(v4(r, kind));
            h.boolean(V::clipAll(v, w, a, vertices));
            Vec4 lo{1728.0f, 1936.0f, 0.0f, 5.0f}, hi{2368.0f, 2160.0f, 0.0f, 16777215.0f};
            h.boolean(V::clipAll(lo, hi, a, vertices));
        }
        report(std::string(name) + " vu0 " + std::to_string(kind), h);
    }
}

void hstate(Hash& h, const TimelineState& s) {
    h.i32(s.counter); h.i32(s.stage);
    for (float x : s.block) h.f32(x);
    h4(h, s.camera); h4(h, s.direction); h4(h, s.up); h.f32(s.roll); h.boolean(s.go); h.i32(s.pending); h.i32(s.scene); h.i32(s.snapshot);
    for (const Vec4& l : s.lights) h4(h, l);
    h.i32(s.logoState); h.i32(s.logoValue); h.i32(s.logoStep);
}
void hsounds(Hash& h, const std::vector<SoundEvent>& sounds) {
    h.u32(static_cast<uint32_t>(sounds.size()));
    for (const SoundEvent& e : sounds) { h.u32(e.id); h.i32(e.argument); h.i32(e.counter); }
}
void hstep(Hash& h, const TimelineStep& s) {
    h.i32(s.result); hm(h, s.matrices.camera); hm(h, s.matrices.viewScreen); hm(h, s.matrices.worldToScreen); hm(h, s.matrices.normalLight);
    h.i32(s.fadeAlpha); h.i32(s.blurLevel); h.i32(s.logoAlpha); h.i32(s.logoValue); hsounds(h, s.sounds);
}

const uint32_t kLater[] = {0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e, 0x6f, 0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0xffffffffu};

std::vector<BootOptions> timelineOptions() {
    std::vector<BootOptions> out;
    for (uint32_t first : {0x65u, 0x6au})
        for (uint32_t later : kLater) {
            BootOptions o;
            o.disc = {first, later};
            out.push_back(o);
        }
    for (int forced = 0; forced < 2; ++forced)
        for (int ready = 0; ready < 2; ++ready)
            for (int exec = 0; exec < 3; ++exec)
                for (uint32_t later : {0x64u, 0x6au, 0x6cu, 0x72u}) {
                    BootOptions o;
                    o.disc = {0x65, later};
                    o.clockForced = forced;
                    o.hddReady = ready;
                    o.hddExec = exec;
                    out.push_back(o);
                }
    return out;
}

// Each set of options from the module's first call to the call that ends the intro (cap 4000), set-up after the first call.
template <class A>
void timeline(const char* name) {
    Hash h;
    for (const BootOptions& o : timelineOptions()) {
        Timeline<A> t(o, Timeline<A>::initialState(1));
        hstate(h, t.state());
        for (int i = 0; i < 4000; ++i) {
            const TimelineStep s = t.step(i == 0 ? o.disc.first : o.disc.later);
            hstep(h, s);
            hstate(h, t.state());
            if (i == 0) t.sceneSetUp();
            if (s.result != 0) break;
        }
    }
    report(std::string(name) + " timeline", h);
    Hash g;
    for (int32_t c : {-5, 0, 1, 2, 77, 1000000}) hstate(g, Timeline<A>::initialState(c));
    report(std::string(name) + " timeline initial", g);
}

// The matrices of the default intro, call by call.
template <class A>
std::vector<std::pair<Matrices, Vec4>> introMatrices() {
    std::vector<std::pair<Matrices, Vec4>> out;
    BootOptions o;
    Timeline<A> t(o, Timeline<A>::initialState(1));
    for (int i = 0; i < 400; ++i) {
        const TimelineStep s = t.step(i == 0 ? o.disc.first : o.disc.later);
        if (i == 0) t.sceneSetUp();
        out.push_back({s.matrices, t.state().camera});
        if (s.result != 0) break;
    }
    return out;
}

void hwork(Hash& h, const CubeWork& w) {
    h.i32(w.index); h4(h, w.angles); hm(h, w.turned); hm(h, w.toWorld); hm(h, w.toScreen); h4(h, w.centre);
    for (const CubeCorner& c : w.corners) { h4(h, c.world); h4(h, c.eye); h4(h, c.screen); h.f32(c.q); for (int32_t x : c.ints) h.i32(x); }
    for (const CubeFace& f : w.faces) {
        for (int32_t x : f.corners) h.i32(x);
        h4(h, f.object); h4(h, f.world); h4(h, f.view); h.f32(f.facing); h4(h, f.colour);
        for (float x : f.edge) h.f32(x);
    }
}

template <class A>
void cubes(const char* name, const ProgramImage& program) {
    Hash h;
    for (const CubePassSetup& s : cubePassSetups()) { h.boolean(s.away); h.i32(s.texture); h.i32(s.mode); h.u32(s.fix); }
    const auto intro = introMatrices<A>();
    for (int field = 0; field < 2; ++field) {
        Cubes<A> c(program);
        int32_t counter = 1;
        for (const auto& [m, camera] : intro) {
            std::vector<Pass> out;
            std::vector<CubeWork> work;
            c.draw(m, camera, out, &work, field == 0 ? 0 : counter & 1);
            hpasses(h, out);
            h.u32(static_cast<uint32_t>(work.size()));
            for (const CubeWork& w : work) hwork(h, w);
            ++counter;
        }
    }
    report(std::string(name) + " cubes intro", h);
    Hash g;
    Cubes<A> c(program);
    Rng r(0xc0be0000ull);
    for (int n = 0; n < 600; ++n) {
        const auto& [m0, camera0] = intro[r.next() % intro.size()];
        Matrices m = m0;
        Vec4 camera = camera0;
        for (Vec4& row : m.worldToScreen) for (float& x : row) x = A::mul(x, 1.0f + gen(r, 2) * 0.002f);
        for (float& x : camera) x = x + gen(r, 2) * 0.1f;
        if (n % 7 == 0) m = {m4(r, 2), m4(r, 2), m4(r, 2), m4(r, 2)};
        std::vector<Pass> out;
        std::vector<CubeWork> work;
        c.draw(m, camera, out, n % 3 == 0 ? nullptr : &work, static_cast<int32_t>(r.next() % 3));
        hpasses(g, out);
        g.u32(static_cast<uint32_t>(work.size()));
        for (const CubeWork& w : work) hwork(g, w);
    }
    report(std::string(name) + " cubes random", g);
}

template <class A>
void fog(const char* name) {
    Hash h;
    Fog<A> f;
    for (const Vec4& v : f.mesh()) h4(h, v);
    for (const auto& b : f.brightness()) for (int32_t x : b) h.i32(x);
    for (float x : f.offsets()) h.f32(x);
    for (const auto& [m, camera] : introMatrices<A>()) {
        std::vector<Pass> out;
        f.draw(m.worldToScreen, out);
        hpasses(h, out);
        for (float x : f.offsets()) h.f32(x);
        h.i32(f.stats().drawn); h.i32(f.stats().leftOut);
    }
    report(std::string(name) + " fog intro", h);
    Hash g;
    Rng r(0xf0900000ull);
    const auto intro = introMatrices<A>();
    for (int n = 0; n < 300; ++n) {
        Mat4 m = intro[r.next() % intro.size()].first.worldToScreen;
        for (Vec4& row : m) for (float& x : row) x = A::mul(x, 1.0f + gen(r, 2) * 0.004f);
        if (n % 5 == 0) m = m4(r, n % 10 == 0 ? 0 : 2);
        std::vector<Pass> out;
        f.draw(m, out);
        hpasses(g, out);
        for (float x : f.offsets()) g.f32(x);
        g.i32(f.stats().drawn); g.i32(f.stats().leftOut);
    }
    report(std::string(name) + " fog random", g);
}

template <class A>
void hlights(Hash& h, const Lights<A>& l) {
    h.i32(l.head()); h.i32(l.tail());
    for (const auto& light : l.matrices()) for (const Mat4& m : light) hm(h, m);
    for (const auto& light : l.ring()) for (const auto& e : light) for (int32_t x : e) h.i32(x);
    for (const Vec4& c : l.centres()) h4(h, c);
    for (float x : l.angles()) h.f32(x);
    for (float x : l.cosines()) h.f32(x);
    for (float x : l.sines()) h.f32(x);
    const LightStats& s = l.stats();
    h.i32(s.spritePackets); h.i32(s.quadsDrawn); h.i32(s.quadsLeftOut); h.i32(s.trailPackets); h.i32(s.trailHidden);
}

template <class A>
void lights(const char* name) {
    Hash h;
    const auto intro = introMatrices<A>();
    for (uint32_t phase : {0u, 1u, 0xD80u, 0x1000u, 0x16A8u, 4000u, 77777u}) {
        Lights<A> l(phase);
        hlights(h, l);
        int32_t counter = 1;
        for (int lap = 0; lap < 2; ++lap)
            for (const auto& [m, camera] : intro) {
                std::vector<Pass> out;
                l.draw(counter++, m.worldToScreen, out);
                hpasses(h, out);
                hlights(h, l);
            }
    }
    report(std::string(name) + " lights intro", h);
    Hash g;
    Rng r(0x11900000ull);
    Lights<A> l(0x1234);
    for (int n = 0; n < 400; ++n) {
        Mat4 m = intro[r.next() % intro.size()].first.worldToScreen;
        for (Vec4& row : m) for (float& x : row) x = A::mul(x, 1.0f + gen(r, 2) * 0.004f);
        if (n % 9 == 0) m = m4(r, 2);
        std::vector<Pass> out;
        l.draw(static_cast<int32_t>(r.next() % 5000), m, out);
        hpasses(g, out);
        hlights(g, l);
    }
    report(std::string(name) + " lights random", g);
}

template <class A>
void flat(const char* name) {
    using F = Flat<A>;
    Hash h;
    h.i32(F::pictureHeight()); h.i32(F::barRows());
    for (int32_t x : kProcessScissor) h.i32(x);
    Rng r(0xf1a70000ull);
    for (int n = 0; n < 3000; ++n) {
        FlatInputs in;
        in.counter = static_cast<int32_t>(r.next32());
        in.displayIndex = static_cast<int32_t>(r.next() % 2);
        in.field = static_cast<int32_t>(r.next() % 2);
        in.logoAlpha = static_cast<int32_t>(r.next() % 400) - 50;
        in.fadeAlpha = static_cast<int32_t>(r.next() % 400) - 50;
        in.blurLevel = static_cast<int32_t>(r.next() % 7) - 1;
        in.fillSprites = static_cast<int32_t>(r.next() % 4);
        if (n % 50 == 0) { in.logoAlpha = static_cast<int32_t>(r.next32()); in.fadeAlpha = static_cast<int32_t>(r.next32()); }
        std::vector<Pass> out;
        F::scissor(in, out);
        F::ghost(in, out);
        F::copyToStore(in, out);
        F::blur(in, out);
        F::fade(in, out);
        F::logo(in, out);
        F::bars(in, out);
        hpasses(h, out);
        h.i32(F::page(in)); h.i32(F::blurWhich(in));
    }
    std::vector<Pass> out;
    F::scissor(out);
    hpasses(h, out);
    report(std::string(name) + " flat", h);
}

void handoff() {
    Hash h;
    for (uint32_t snapshot = 0; snapshot < 0x100; ++snapshot)
        for (int forced = 0; forced < 2; ++forced)
            for (int ready = 0; ready < 2; ++ready)
                for (int32_t exec : {-1, 0, 1, 2})
                    for (int32_t cdda : {-1, 0, 1, 2}) {
                        const HandOff o = decide({snapshot, forced != 0, ready != 0, exec, cdda});
                        h.i32(o.module); h.i32(o.executeAppType); h.boolean(o.previousWasOpening);
                    }
    for (uint32_t snapshot : {0xffffffffu, 0x80000000u, 0x10000u}) {
        const HandOff o = decide({snapshot, false, false, 0, 0});
        h.i32(o.module); h.i32(o.executeAppType); h.boolean(o.previousWasOpening);
    }
    h.i32(kFramesToClock);
    report("handoff", h);
}

History demoHistory() {
    History history{};
    const char* names[] = {"DEMO A", "DEMO B", "DEMO C", "DEMO D", "DEMO E"};
    for (size_t i = 0; i < 5; ++i) {
        for (size_t c = 0; names[i][c]; ++c) history[i].name[c] = names[i][c];
        history[i].count = static_cast<uint8_t>(3 + 4 * i);
        history[i].mask = static_cast<uint8_t>(0x0F << i);
        history[i].mainCell = static_cast<uint8_t>(i);
    }
    return history;
}

History randomHistory(uint64_t seed) {
    Rng r(seed);
    History history{};
    for (HistoryEntry& e : history) {
        if (r.next() % 3 != 0) {
            const size_t length = 1 + r.next() % 15;
            for (size_t c = 0; c < length; ++c) e.name[c] = static_cast<char>('A' + r.next() % 26);
        }
        e.count = static_cast<uint8_t>(r.next());
        e.mask = static_cast<uint8_t>(r.next());
        e.mainCell = static_cast<uint8_t>(r.next() % 8);
    }
    return history;
}

void htables(Hash& h, const TowerTables& t) {
    for (const auto& c : t.flag) for (int32_t x : c) h.i32(x);
    for (const auto& c : t.fade) for (int32_t x : c) h.i32(x);
    for (const auto& c : t.sway) for (float x : c) h.f32(x);
    for (const auto& c : t.tall) for (float x : c) h.f32(x);
    for (const auto& c : t.height) for (float x : c) h.f32(x);
    for (const auto& c : t.place) for (const Vec4& v : c) h4(h, v);
}
void hchain(Hash& h, const TowerChain& c) {
    for (size_t i = 0; i < c.bytes.size(); i += 4) {
        uint32_t w;
        std::memcpy(&w, c.bytes.data() + i, 4);
        h.u32(canonical(w));
    }
}
void hvertices(Hash& h, const TowerVertices& v) {
    for (const Vertex& x : v.vertices) hvertex(h, x);
    for (bool k : v.kicked) h.boolean(k);
    h.u64(v.alpha);
}

template <class A>
void towers(const char* name, const ProgramImage& program) {
    std::vector<History> histories = {History{}, demoHistory()};
    for (uint64_t s = 0; s < 6; ++s) histories.push_back(randomHistory(0x70e40000ull + s));
    const auto intro = introMatrices<A>();
    Hash h;
    for (const History& history : histories) {
        const Towers<A> t(history, program);
        htables(h, t.tables());
        for (const auto& row : t.brightness()) for (float x : row) h.f32(x);
        for (int32_t c = -400; c < 800; c += 7) h.f32(t.swing(c));
        int32_t counter = 1;
        for (const auto& [m, camera] : intro) {
            TowerMatrices tm;
            for (size_t i = 0; i < 4; ++i) tm.base[i][i] = 1.0f;
            tm.worldToScreen = m.worldToScreen;
            tm.light = m.normalLight;
            std::vector<Pass> out;
            std::vector<TowerChain> chains;
            t.draw(counter++, tm, camera, out, &chains);
            hpasses(h, out);
            h.u32(static_cast<uint32_t>(chains.size()));
            if (counter % 16 == 0) for (const TowerChain& c : chains) hchain(h, c);
        }
    }
    report(std::string(name) + " towers intro", h);

    Hash g;
    Rng r(0x70e50000ull);
    const Towers<A> t(demoHistory(), program);
    for (int n = 0; n < 1500; ++n) {
        const int32_t column = static_cast<int32_t>(r.next() % kTowerColumns), row = static_cast<int32_t>(r.next() % kTowerRows);
        const float sine = gen(r, 2) * (1.0f / 128.0f);
        const auto& [m, camera] = intro[r.next() % intro.size()];
        TowerMatrices tm{m4(r, 2), m.worldToScreen, m.normalLight};
        if (n % 2 == 0) {
            tm.base = Vu0<A>::unitMatrix();
            for (Vec4& rr : tm.worldToScreen) for (float& x : rr) x = A::mul(x, 1.0f + gen(r, 2) * 0.004f);
        }
        if (n % 11 == 0) tm = {m4(r, 0), m4(r, 1), m4(r, 2)};
        const TowerChain chain = t.chain(column, row, sine, tm);
        hchain(g, chain);
        hvertices(g, TowerVu1<A>::run(chain));
    }
    report(std::string(name) + " towers chains", g);

    // The microprogram's checks: chains with a word changed, their vertices or the message they stop with.
    Hash e;
    Rng q(0x70e60000ull);
    TowerMatrices tm{Vu0<A>::unitMatrix(), intro[100].first.worldToScreen, intro[100].first.normalLight};
    const TowerChain valid = t.chain(5, 4, 0.25f, tm);
    for (int n = 0; n < 3000; ++n) {
        TowerChain chain = valid;
        const int changes = 1 + static_cast<int>(q.next() % 3);
        for (int k = 0; k < changes; ++k) {
            const size_t at = (q.next() % (chain.bytes.size() / 4)) * 4;
            uint32_t w;
            std::memcpy(&w, chain.bytes.data() + at, 4);
            const uint64_t how = q.next() % 4;
            if (how == 0) w = q.next32();
            else if (how == 1) w ^= 1u << (q.next() % 32);
            else if (how == 2) w = (w & 0x00ffffffu) | (static_cast<uint32_t>(q.next() % 0x80) << 24);
            else w = (w & 0xffff0000u) | static_cast<uint32_t>(q.next() % 0x10);
            std::memcpy(chain.bytes.data() + at, &w, 4);
        }
        try {
            const TowerVertices v = TowerVu1<A>::run(chain);
            e.u32(0);
            hvertices(e, v);
        } catch (const std::runtime_error& error) {
            e.u32(1);
            hname(e, error.what());
        }
    }
    report(std::string(name) + " towers vu1 mutated", e);
}

struct Combo {
    BootOptions options;
    int fieldMode = 0;
};

std::vector<Combo> combos() {
    std::vector<Combo> out;
    std::vector<std::optional<History>> histories = {std::nullopt, demoHistory(), randomHistory(0x0be50001ull), randomHistory(0x0be50002ull)};
    std::vector<std::optional<uint32_t>> phases = {std::nullopt, 0u, 0x1000u, 0x16A8u, 77777u};
    int n = 0;
    for (uint32_t later : kLater) {
        Combo c;
        c.options.disc = {0x65, later};
        c.options.history = histories[n % histories.size()];
        c.options.lightsPhase = phases[n % phases.size()];
        c.options.coldStart = n % 3 != 2;
        c.options.module = n % 4 == 3 ? 4 : n % 4 == 2 ? 2 : 1;
        c.fieldMode = n % 2;
        out.push_back(c);
        ++n;
    }
    for (int forced = 0; forced < 2; ++forced)
        for (int ready = 0; ready < 2; ++ready)
            for (int exec = 0; exec < 3; ++exec) {
                Combo c;
                c.options.disc = {n % 2 ? 0x6au : 0x65u, n % 3 == 0 ? 0x6cu : 0x64u};
                c.options.clockForced = forced;
                c.options.hddReady = ready;
                c.options.hddExec = exec;
                c.options.history = histories[n % histories.size()];
                c.options.lightsPhase = phases[n % phases.size()];
                c.options.coldStart = n % 4 != 1;
                c.options.module = 1 + (n % 4);
                c.options.pal = n % 5 == 0;
                c.fieldMode = (n / 2) % 2;
                out.push_back(c);
                ++n;
            }
    return out;
}

void hcommands(Hash& h, const std::vector<SoundCommand>& commands) {
    h.u32(static_cast<uint32_t>(commands.size()));
    for (const SoundCommand& c : commands) { h.u32(c.id); h.u32(c.a1); h.u32(c.a2); h.u32(c.a3); h.boolean(c.carriedA2); h.boolean(c.carriedA3); }
}

template <class A>
void whole(const char* name, const std::shared_ptr<const ProgramImage>& program) {
    int index = 0;
    Hash all;
    for (const Combo& c : combos()) {
        Opening<A> o(c.options, program);
        Hash h;
        hcommands(h, o.startCommands());
        h.u32(o.randCalls()); h.u32(o.randState()); h.i32(o.counter()); h.boolean(o.handOff() != nullptr);
        int32_t display = 0;
        int frames = 0;
        while (!o.ended() && frames < 5000) {
            const int32_t field = c.fieldMode ? frames & 1 : 0;
            const Frame f = o.frame(display, field);
            h.u64(hashFrame(f));
            hpasses(h, f.passes);
            hsounds(h, o.sounds());
            hcommands(h, o.commands());
            h.i32(o.counter()); h.boolean(o.ended()); h.u32(o.randCalls()); h.u32(o.randState());
            hstate(h, o.timeline().state());
            h.i32(o.fog().stats().drawn); h.i32(o.fog().stats().leftOut);
            hlights(h, o.lights());
            h.u32(static_cast<uint32_t>(o.cubeWork().size()));
            for (const CubeWork& w : o.cubeWork()) hwork(h, w);
            display ^= 1;
            ++frames;
        }
        h.i32(frames);
        h.boolean(o.handOff() != nullptr);
        if (const HandOff* hand = o.handOff()) { h.i32(hand->module); h.i32(hand->executeAppType); h.boolean(hand->previousWasOpening); }
        std::printf("%s whole %d frames %d %llu 0x%016llx\n", name, index, frames, (unsigned long long)h.count, (unsigned long long)h.h);
        all.u64(h.h);
        ++index;
    }
    report(std::string(name) + " whole all", all);
}

int main(int argc, char** argv) {
    const std::string only = argc > 1 ? argv[1] : "";
    const auto program = std::make_shared<const ProgramImage>(ProgramImage::load(kElf));
    const auto want = [&](const char* part) { return only.empty() || only == part; };
    if (want("rand")) {
        Hash h;
        for (uint32_t s : {0u, 1u, 0x12345678u, 0xffffffffu})
            for (uint32_t calls : {0u, 1u, 2u, 385u, 386u, 27314u}) h.u32(advanceRand(s, calls));
        report("rand", h);
    }
    if (want("vu0")) { vu0<EeArithmetic>("ee"); vu0<NativeArithmetic>("native"); }
    if (want("timeline")) { timeline<EeArithmetic>("ee"); timeline<NativeArithmetic>("native"); }
    if (want("cubes")) { cubes<EeArithmetic>("ee", *program); cubes<NativeArithmetic>("native", *program); }
    if (want("fog")) { fog<EeArithmetic>("ee"); fog<NativeArithmetic>("native"); }
    if (want("lights")) { lights<EeArithmetic>("ee"); lights<NativeArithmetic>("native"); }
    if (want("flat")) { flat<EeArithmetic>("ee"); flat<NativeArithmetic>("native"); }
    if (want("handoff")) handoff();
    if (want("towers")) { towers<EeArithmetic>("ee", *program); towers<NativeArithmetic>("native", *program); }
    if (only == "canonical") { gCanonical = true; vu0<NativeArithmetic>("native canonical"); towers<NativeArithmetic>("native canonical", *program); }
    if (only == "o0") { vu0<NativeArithmetic>("native o0"); towers<NativeArithmetic>("native o0", *program); }
    if (want("whole")) { whole<EeArithmetic>("ee", program); whole<NativeArithmetic>("native", program); }
    return 0;
}
