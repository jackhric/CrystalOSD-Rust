#pragma once
// Render vector stream: the NativeRenderer calls of a run, in order, with every argument.
// Little-endian. Header "CRSV" u32 version 1, then records: u8 tag, payload.
//  1 configure   u32 width, height, samples
//  2 setTexture  i32 set, i32 number, u32 width, height, u8[width*height*4]
//  3 glyphCache  GlyphCache, u64 n, u8[n] (glyphCacheImage(font, cache) as C++ computes it)
//  4 draw        Frame
//  5 readTarget  i32 target, u64 fnv1a64 of the bytes returned (0 when not known)
//  6 label       u64 n, char[n]
//  7 present     u32 width, height, i32 shown, f32 aspect, u64 fnv1a64 of the B8G8R8A8 image read back (0 when not known)
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <stdexcept>
#include <string>
#include <vector>

#include "Scene/Frame.hpp"

namespace stream {

enum Tag : uint8_t { Configure = 1, SetTexture = 2, GlyphCacheTag = 3, Draw = 4, ReadTarget = 5, Label = 6, Present = 7 };

inline uint64_t fnv(const uint8_t* data, size_t n) {
    uint64_t h = 0xcbf29ce484222325ull;
    for (size_t i = 0; i < n; ++i) {
        h ^= data[i];
        h *= 0x100000001b3ull;
    }
    return h;
}

struct Writer {
    std::FILE* f{nullptr};
    explicit Writer(const char* path) {
        f = std::fopen(path, "wb");
        if (!f) throw std::runtime_error(std::string("cannot write ") + path);
        std::fwrite("CRSV", 1, 4, f);
        u32(1);
    }
    ~Writer() {
        if (f) std::fclose(f);
    }
    void raw(const void* p, size_t n) { std::fwrite(p, 1, n, f); }
    void u8(uint8_t v) { raw(&v, 1); }
    void u32(uint32_t v) { raw(&v, 4); }
    void i32(int32_t v) { raw(&v, 4); }
    void u64(uint64_t v) { raw(&v, 8); }
    void f32(float v) { raw(&v, 4); }
    void b(bool v) { u8(v ? 1 : 0); }
    void str(const std::string& s) {
        u64(s.size());
        raw(s.data(), s.size());
    }
    void glyphs(const scene::GlyphCache& g) {
        u32(g.address);
        i32(g.cellWidth);
        i32(g.cellHeight);
        i32(g.width);
        i32(g.logWidth);
        i32(g.logHeight);
        u64(g.cells.size());
        for (int32_t c : g.cells) i32(c);
    }
    void frame(const scene::Frame& fr) {
        i32(fr.width);
        i32(fr.height);
        i32(fr.field);
        i32(fr.displayIndex);
        u64(fr.textAt);
        i32(static_cast<int32_t>(fr.textureSet));
        i32(fr.depthBits);
        glyphs(fr.glyphs);
        u64(fr.passes.size());
        for (const scene::Pass& p : fr.passes) {
            str(p.name);
            i32(static_cast<int32_t>(p.target));
            i32(static_cast<int32_t>(p.topology));
            const scene::Material& m = p.material;
            i32(static_cast<int32_t>(m.source));
            i32(m.texture);
            i32(static_cast<int32_t>(m.sourceTarget));
            b(m.colourOnly);
            i32(static_cast<int32_t>(m.coordinates));
            i32(static_cast<int32_t>(m.sampling));
            for (int k = 0; k < 4; ++k) i32(m.region[k]);
            b(m.bilinear);
            i32(m.sourceHeight);
            i32(static_cast<int32_t>(m.blend));
            u8(m.blendConstant);
            i32(static_cast<int32_t>(m.depthTest));
            b(m.depthWrite);
            b(m.gouraud);
            b(m.perPixelAlpha);
            b(m.alphaCorrection);
            b(p.edgeSmoothing);
            b(p.halfLine);
            b(p.primBlend);
            b(p.scissor.has_value());
            for (int k = 0; k < 4; ++k) i32(p.scissor ? (*p.scissor)[k] : 0);
            u64(p.vertices.size());
            for (const scene::Vertex& v : p.vertices) {
                f32(v.x);
                f32(v.y);
                u32(v.z);
                f32(v.u);
                f32(v.v);
                f32(v.q);
                u8(v.r);
                u8(v.g);
                u8(v.b);
                u8(v.a);
            }
        }
    }
    void configure(uint32_t w, uint32_t h, uint32_t s) {
        u8(Configure);
        u32(w);
        u32(h);
        u32(s);
    }
    void setTexture(int32_t set, int32_t number, uint32_t w, uint32_t h, const uint8_t* rgba) {
        u8(SetTexture);
        i32(set);
        i32(number);
        u32(w);
        u32(h);
        raw(rgba, size_t(w) * h * 4);
    }
    void glyphCache(const scene::GlyphCache& g, const std::vector<uint8_t>& image) {
        u8(GlyphCacheTag);
        glyphs(g);
        u64(image.size());
        raw(image.data(), image.size());
    }
    void draw(const scene::Frame& fr) {
        u8(Draw);
        frame(fr);
    }
    void readTarget(int32_t target, uint64_t hash) {
        u8(ReadTarget);
        i32(target);
        u64(hash);
    }
    void label(const std::string& s) {
        u8(Label);
        str(s);
    }
    void present(uint32_t w, uint32_t h, int32_t shown, float aspect, uint64_t hash) {
        u8(Present);
        u32(w);
        u32(h);
        i32(shown);
        f32(aspect);
        u64(hash);
    }
    void flush() { std::fflush(f); }
};

struct Reader {
    std::vector<uint8_t> d;
    size_t at{0};
    explicit Reader(const char* path) {
        std::FILE* f = std::fopen(path, "rb");
        if (!f) throw std::runtime_error(std::string("cannot read ") + path);
        std::fseek(f, 0, SEEK_END);
        d.resize(size_t(std::ftell(f)));
        std::fseek(f, 0, SEEK_SET);
        if (std::fread(d.data(), 1, d.size(), f) != d.size()) throw std::runtime_error("short read");
        std::fclose(f);
        if (d.size() < 8 || std::memcmp(d.data(), "CRSV", 4) != 0) throw std::runtime_error("not a stream");
        at = 4;
        if (u32() != 1) throw std::runtime_error("stream version");
    }
    bool done() const { return at >= d.size(); }
    void raw(void* p, size_t n) {
        if (at + n > d.size()) throw std::runtime_error("stream past the end");
        std::memcpy(p, d.data() + at, n);
        at += n;
    }
    uint8_t u8() { uint8_t v; raw(&v, 1); return v; }
    uint32_t u32() { uint32_t v; raw(&v, 4); return v; }
    int32_t i32() { int32_t v; raw(&v, 4); return v; }
    uint64_t u64() { uint64_t v; raw(&v, 8); return v; }
    float f32() { float v; raw(&v, 4); return v; }
    bool b() { return u8() != 0; }
    std::string str() {
        std::string s(u64(), '\0');
        raw(s.data(), s.size());
        return s;
    }
    scene::GlyphCache glyphs() {
        scene::GlyphCache g;
        g.address = u32();
        g.cellWidth = i32();
        g.cellHeight = i32();
        g.width = i32();
        g.logWidth = i32();
        g.logHeight = i32();
        g.cells.resize(u64());
        for (int32_t& c : g.cells) c = i32();
        return g;
    }
    scene::Frame frame() {
        scene::Frame fr;
        fr.width = i32();
        fr.height = i32();
        fr.field = i32();
        fr.displayIndex = i32();
        fr.textAt = u64();
        fr.textureSet = static_cast<scene::TextureSet>(i32());
        fr.depthBits = i32();
        fr.glyphs = glyphs();
        fr.passes.resize(u64());
        for (scene::Pass& p : fr.passes) {
            p.name = str();
            p.target = static_cast<scene::TargetName>(i32());
            p.topology = static_cast<scene::PassTopology>(i32());
            scene::Material& m = p.material;
            m.source = static_cast<scene::SourceKind>(i32());
            m.texture = i32();
            m.sourceTarget = static_cast<scene::TargetName>(i32());
            m.colourOnly = b();
            m.coordinates = static_cast<scene::CoordinateKind>(i32());
            m.sampling = static_cast<scene::Sampling>(i32());
            for (int k = 0; k < 4; ++k) m.region[k] = i32();
            m.bilinear = b();
            m.sourceHeight = i32();
            m.blend = static_cast<scene::BlendOp>(i32());
            m.blendConstant = u8();
            m.depthTest = static_cast<scene::DepthTest>(i32());
            m.depthWrite = b();
            m.gouraud = b();
            m.perPixelAlpha = b();
            m.alphaCorrection = b();
            p.edgeSmoothing = b();
            p.halfLine = b();
            p.primBlend = b();
            const bool scissor = b();
            std::array<int32_t, 4> s{};
            for (int k = 0; k < 4; ++k) s[k] = i32();
            if (scissor) p.scissor = s;
            p.vertices.resize(u64());
            for (scene::Vertex& v : p.vertices) {
                v.x = f32();
                v.y = f32();
                v.z = u32();
                v.u = f32();
                v.v = f32();
                v.q = f32();
                v.r = u8();
                v.g = u8();
                v.b = u8();
                v.a = u8();
            }
        }
        return fr;
    }
};

}  // namespace stream
