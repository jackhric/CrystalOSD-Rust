// Writes the synthetic render vector stream: frames that exercise every BlendOp, DepthTest, Sampling, SourceKind,
// CoordinateKind, topology, target, flag and texture set, at three output configurations, each target read back.
// usage: Synthetic <out.stream>
#include <cstdio>

#include "Stream.hpp"

using namespace scene;

namespace {

uint32_t seed = 12345;
uint32_t rnd() {
    seed = seed * 1664525u + 1013904223u;
    return seed >> 8;
}
float rf(float lo, float hi) { return lo + (hi - lo) * float(rnd() % 100000) / 100000.0f; }
uint8_t rb(uint32_t lo, uint32_t hi) { return uint8_t(lo + rnd() % (hi - lo + 1)); }

Vertex vtx(float x, float y, uint32_t z, float u, float v, float q, uint8_t r, uint8_t g, uint8_t b, uint8_t a) { return {x, y, z, u, v, q, r, g, b, a}; }

Pass sprite(const char* name, TargetName t, BlendOp blend, uint8_t r, uint8_t g, uint8_t b, uint8_t a, float x0 = 0, float y0 = 0, float x1 = 640, float y1 = 224) {
    Pass p;
    p.name = name;
    p.target = t;
    p.topology = PassTopology::Sprites;
    p.material.blend = blend;
    p.vertices = {vtx(x0, y0, 0, 0, 0, 1, r, g, b, a), vtx(x1, y1, 0, 0, 0, 1, r, g, b, a)};
    return p;
}

// A gradient over the whole picture as two triangles, alpha from aLo to aHi.
Pass gradient(const char* name, TargetName t, uint8_t aLo, uint8_t aHi, float w = 640, float h = 224) {
    Pass p;
    p.name = name;
    p.target = t;
    p.material.blend = BlendOp::Opaque;
    p.material.gouraud = true;
    const Vertex a = vtx(0, 0, 0, 0, 0, 1, 250, 10, 40, aLo), b = vtx(w, 0, 0, 0, 0, 1, 20, 240, 60, aHi), c = vtx(0, h, 0, 0, 0, 1, 30, 50, 230, aHi),
                 d = vtx(w, h, 0, 0, 0, 1, 200, 200, 200, aLo);
    p.vertices = {a, b, c, b, d, c};
    return p;
}

Pass randomTriangles(const char* name, TargetName t, BlendOp blend, int count, uint8_t aMax, float w = 640, float h = 224) {
    Pass p;
    p.name = name;
    p.target = t;
    p.material.blend = blend;
    p.material.gouraud = true;
    for (int i = 0; i < count * 3; ++i) p.vertices.push_back(vtx(rf(-20, w + 20), rf(-10, h + 10), 0, 0, 0, 1, rb(0, 255), rb(0, 255), rb(0, 255), rb(0, aMax)));
    return p;
}

Frame frameOf(std::vector<Pass> passes, TextureSet set = TextureSet::Clock) {
    Frame f;
    f.passes = std::move(passes);
    f.textAt = f.passes.size();
    f.textureSet = set;
    return f;
}

std::vector<uint8_t> textureImage(uint32_t w, uint32_t h, uint32_t alphaMax, uint32_t salt) {
    std::vector<uint8_t> t(size_t(w) * h * 4);
    for (uint32_t y = 0; y < h; ++y)
        for (uint32_t x = 0; x < w; ++x) {
            uint8_t* p = &t[(size_t(y) * w + x) * 4];
            p[0] = uint8_t((x * 255) / (w - 1));
            p[1] = uint8_t((y * 255) / (h - 1));
            p[2] = uint8_t(((x ^ y) * 37 + salt) & 0xff);
            p[3] = uint8_t(((x + y * 3 + salt) * 11) % (alphaMax + 1));
            if ((x / 4 + y / 4) % 7 == 0) p[0] = p[1] = p[2] = 0;
        }
    return t;
}

std::vector<Frame> frames() {
    std::vector<Frame> out;
    const BlendOp ops[] = {BlendOp::Opaque, BlendOp::Add, BlendOp::AlphaOver, BlendOp::Subtract, BlendOp::FixedOver, BlendOp::FixedAdd, BlendOp::AddDestinationAlpha, BlendOp::SubtractFixed};
    const uint8_t constants[] = {0x00, 0x20, 0x55, 0x80};
    // 1. every blend over a gradient whose alpha runs 0 .. 0x80, with each constant.
    for (BlendOp op : ops)
        for (uint8_t k : constants) {
            Pass top = randomTriangles("blended", TargetName::Display, op, 40, 0x80);
            top.material.blendConstant = k;
            Pass flat = sprite("blended sprite", TargetName::Display, op, 200, 90, 30, 0x40, 100, 50, 400, 180);
            flat.material.blendConstant = k;
            out.push_back(frameOf({gradient("ground", TargetName::Display, 0, 0x80), top, flat}));
        }
    // 2. depth: tests and writes, 32 and 24 bits (z above 2^24 masked).
    const DepthTest tests[] = {DepthTest::Always, DepthTest::GreaterEqual, DepthTest::Greater};
    for (int bits : {32, 24})
        for (DepthTest test : tests)
            for (bool write : {true, false}) {
                Pass clear = sprite("clear", TargetName::Display, BlendOp::Opaque, 0, 0, 0, 0x80);
                clear.material.depthTest = DepthTest::Always;
                clear.material.depthWrite = true;
                Pass a = randomTriangles("near", TargetName::Display, BlendOp::Opaque, 30, 0x80);
                for (size_t i = 0; i < a.vertices.size(); ++i) a.vertices[i].z = (i / 3) % 2 ? 0x00800000u + uint32_t(i) * 999u : 0x01000000u + uint32_t(i) * 77777u;
                a.material.depthTest = test;
                a.material.depthWrite = write;
                Pass b = randomTriangles("far", TargetName::Display, BlendOp::AlphaOver, 30, 0x80);
                for (size_t i = 0; i < b.vertices.size(); ++i) b.vertices[i].z = 0x00400000u + uint32_t(i) * 30011u;
                b.material.depthTest = test;
                b.material.depthWrite = write;
                Frame f = frameOf({clear, a, b});
                f.depthBits = bits;
                out.push_back(f);
            }
    // 3. textures: every sampling, bilinear or not, texel and projective, colour only, region clamps.
    const Sampling samplings[] = {Sampling::Repeat, Sampling::Clamp, Sampling::ClampToRegion};
    for (Sampling s : samplings)
        for (bool bilinear : {false, true})
            for (CoordinateKind c : {CoordinateKind::Texel, CoordinateKind::Projective})
                for (int texture : {0, 1, 2}) {
                    Pass p;
                    p.name = "textured";
                    p.material.source = SourceKind::Texture;
                    p.material.texture = texture;
                    p.material.sampling = s;
                    p.material.bilinear = bilinear;
                    p.material.coordinates = c;
                    p.material.region = {3, 40, 5, 27};
                    p.material.blend = texture == 2 ? BlendOp::AlphaOver : BlendOp::Opaque;
                    p.material.colourOnly = texture == 1 && bilinear;
                    for (int i = 0; i < 12; ++i) {
                        const float q = c == CoordinateKind::Projective ? rf(0.5f, 2.0f) : 1.0f;
                        const float x = rf(-30, 670), y = rf(-10, 234);
                        const float u = c == CoordinateKind::Projective ? rf(-0.5f, 1.5f) * q : rf(-70, 140);
                        const float v = c == CoordinateKind::Projective ? rf(-0.5f, 1.5f) * q : rf(-40, 90);
                        p.vertices.push_back(vtx(x, y, 0, u, v, q, rb(0, 255), rb(0, 255), rb(0, 255), rb(0x10, 0x80)));
                    }
                    Pass spr = sprite("textured sprite", TargetName::Display, BlendOp::Opaque, 0x80, 0x80, 0x80, 0x80, 10.25f, 20.5f, 300.75f, 200.5f);
                    spr.material = p.material;
                    spr.material.coordinates = CoordinateKind::Texel;
                    spr.vertices[0].u = -3.5f;
                    spr.vertices[0].v = 0.5f;
                    spr.vertices[1].u = 100.25f;
                    spr.vertices[1].v = 64.5f;
                    out.push_back(frameOf({gradient("ground", TargetName::Display, 0x80, 0x80), p, spr}));
                }
    // 4. targets as sources: RefractionSource and Work drawn, then read back into Display every way.
    for (Sampling s : samplings)
        for (bool bilinear : {false, true})
            for (bool colourOnly : {false, true})
                for (CoordinateKind c : {CoordinateKind::Texel, CoordinateKind::Projective}) {
                    if (c == CoordinateKind::Projective && colourOnly) continue;
                    Pass src = gradient("source", TargetName::RefractionSource, 0x10, 0x80);
                    Pass work = randomTriangles("work", TargetName::Work, BlendOp::Opaque, 20, 0x80);
                    Pass read = sprite("read", TargetName::Display, BlendOp::AlphaOver, 0x80, 0x70, 0x60, 0x80, 0, 0, 640, 224);
                    read.material.source = SourceKind::Target;
                    read.material.sourceTarget = TargetName::RefractionSource;
                    read.material.sampling = s;
                    read.material.bilinear = bilinear;
                    read.material.colourOnly = colourOnly;
                    read.material.coordinates = c;
                    read.material.region = {10, 600, 4, 200};
                    if (c == CoordinateKind::Projective) {
                        read.vertices[0].u = -0.1f;
                        read.vertices[0].v = 0.05f;
                        read.vertices[1].u = 1.2f;
                        read.vertices[1].v = 0.9f;
                    } else {
                        read.vertices[0].u = 0.5f;
                        read.vertices[0].v = -20.5f;
                        read.vertices[1].u = 700.5f;
                        read.vertices[1].v = 224.5f;
                    }
                    Pass back = read;
                    back.name = "read work";
                    back.target = TargetName::RefractionSource;
                    back.material.sourceTarget = TargetName::Work;
                    back.material.blend = BlendOp::Add;
                    back.vertices[0].x = 100;
                    back.vertices[1].x = 500;
                    Pass again = read;
                    again.name = "read again";
                    again.material.blend = BlendOp::Subtract;
                    out.push_back(frameOf({gradient("ground", TargetName::Display, 0x80, 0x40), src, work, read, back, again}));
                }
    // 5. lines: x-major and y-major, with and without edge smoothing (no depth write on AA1 lines), blended or not.
    for (bool edge : {false, true})
        for (BlendOp op : {BlendOp::Opaque, BlendOp::Add, BlendOp::AlphaOver}) {
            Pass lines;
            lines.name = "lines";
            lines.topology = PassTopology::Lines;
            lines.edgeSmoothing = edge;
            lines.material.blend = op;
            lines.material.depthTest = DepthTest::GreaterEqual;
            lines.material.depthWrite = true;
            for (int i = 0; i < 60; ++i) {
                const float x = rf(0, 640), y = rf(0, 224);
                const bool steep = i % 2;
                const float x2 = steep ? x + rf(-20, 20) : x + rf(-200, 200), y2 = steep ? y + rf(-100, 100) : y + rf(-10, 10);
                lines.vertices.push_back(vtx(x, y, 0x100000u + uint32_t(i), 0, 0, 1, rb(0, 255), rb(0, 255), rb(0, 255), rb(0x20, 0x80)));
                lines.vertices.push_back(vtx(x2, y2, 0x100000u + uint32_t(i), 0, 0, 1, rb(0, 255), rb(0, 255), rb(0, 255), rb(0x20, 0x80)));
            }
            lines.vertices.push_back(vtx(5, 5, 0, 0, 0, 1, 255, 255, 255, 0x80));
            Pass over = randomTriangles("over", TargetName::Display, BlendOp::Opaque, 10, 0x80);
            for (Vertex& v : over.vertices) v.z = 0x100010u;
            over.material.depthTest = DepthTest::GreaterEqual;
            Pass clear = sprite("clear", TargetName::Display, BlendOp::Opaque, 0, 0, 0, 0x80);
            out.push_back(frameOf({clear, lines, over}));
        }
    // 6. flags the native renderer does not draw differently, and odd vertex counts, empty passes.
    {
        Pass a = randomTriangles("flags", TargetName::Display, BlendOp::AlphaOver, 30, 0x80);
        a.halfLine = true;
        a.primBlend = true;
        a.scissor = std::array<int32_t, 4>{10, 10, 300, 100};
        a.material.gouraud = false;
        a.material.alphaCorrection = true;
        a.material.sourceHeight = 224;
        Pass pabe = sprite("pabe", TargetName::Display, BlendOp::Opaque, 60, 200, 10, 127, 50, 50, 200, 200);
        pabe.material.perPixelAlpha = true;
        Pass odd = sprite("odd", TargetName::Display, BlendOp::Add, 30, 30, 30, 0x40, 300, 20, 600, 200);
        odd.vertices.push_back(vtx(1, 1, 0, 0, 0, 1, 255, 0, 0, 0x80));
        Pass tri = randomTriangles("odd triangles", TargetName::Display, BlendOp::Opaque, 3, 0x80);
        tri.vertices.pop_back();
        Pass empty;
        empty.name = "empty";
        empty.target = TargetName::Work;
        out.push_back(frameOf({gradient("ground", TargetName::Display, 0x80, 0x80), empty, a, pabe, odd, tri}));
        out.push_back(frameOf({}));
    }
    // 7. a picture of another size (scale differs across and down).
    {
        Frame f = frameOf({gradient("ground", TargetName::Display, 0x80, 0x20, 512, 300), randomTriangles("tri", TargetName::Display, BlendOp::Add, 30, 0x80, 512, 300)});
        f.width = 512;
        f.height = 300;
        Pass read = sprite("read", TargetName::RefractionSource, BlendOp::Opaque, 0x80, 0x80, 0x80, 0x80, 0, 0, 512, 300);
        read.material.source = SourceKind::Target;
        read.material.sourceTarget = TargetName::Display;
        read.material.bilinear = true;
        read.vertices[0].u = 0.5f;
        read.vertices[0].v = 0.5f;
        read.vertices[1].u = 512.5f;
        read.vertices[1].v = 300.5f;
        f.passes.push_back(read);
        out.push_back(f);
    }
    // 8. the opening set: its textures, Store and Extra, Extra read projectively (the 1024 x 256 extent).
    for (bool bilinear : {false, true})
        for (CoordinateKind c : {CoordinateKind::Texel, CoordinateKind::Projective}) {
            Pass t = sprite("opening texture", TargetName::Extra, BlendOp::Opaque, 0x80, 0x80, 0x80, 0x40, 0, 0, 640, 224);
            t.material.source = SourceKind::Texture;
            t.material.texture = 6;
            t.material.bilinear = bilinear;
            t.vertices[0].u = 0.5f;
            t.vertices[0].v = 0.5f;
            t.vertices[1].u = 256.5f;
            t.vertices[1].v = 224.5f;
            Pass store = sprite("store", TargetName::Store, BlendOp::Opaque, 200, 150, 100, 0x80, 0, 0, 320, 224);
            Pass storeAdd = randomTriangles("store tri", TargetName::Store, BlendOp::AddDestinationAlpha, 20, 0x80, 320, 224);
            Pass readStore = sprite("read store", TargetName::Display, BlendOp::FixedOver, 0x80, 0x80, 0x80, 0x80);
            readStore.material.blendConstant = 0x40;
            readStore.material.source = SourceKind::Target;
            readStore.material.sourceTarget = TargetName::Store;
            readStore.material.sampling = Sampling::ClampToRegion;
            readStore.material.region = {0, 639, 0, 223};
            readStore.material.bilinear = bilinear;
            readStore.vertices[1].u = 640.0f;
            readStore.vertices[1].v = 224.0f;
            Pass readExtra = sprite("read extra", TargetName::Display, BlendOp::SubtractFixed, 0x80, 0x80, 0x80, 0x80);
            readExtra.material.blendConstant = 0x30;
            readExtra.material.source = SourceKind::Target;
            readExtra.material.sourceTarget = TargetName::Extra;
            readExtra.material.coordinates = c;
            readExtra.material.sampling = Sampling::ClampToRegion;
            readExtra.material.region = {0, 639, 0, 223};
            readExtra.material.bilinear = bilinear;
            readExtra.vertices[1].u = c == CoordinateKind::Projective ? 0.625f : 640.0f;
            readExtra.vertices[1].v = c == CoordinateKind::Projective ? 0.875f : 224.0f;
            out.push_back(frameOf({gradient("ground", TargetName::Display, 0x80, 0x80), t, store, storeAdd, readStore, readExtra}, TextureSet::Opening));
        }
    return out;
}

}  // namespace

int main(int argc, char** argv) {
    if (argc != 2) {
        std::fprintf(stderr, "usage: Synthetic <out.stream>\n");
        return 1;
    }
    stream::Writer w(argv[1]);
    w.label("synthetic");
    const auto t0 = textureImage(64, 64, 0x80, 1), t1 = textureImage(128, 32, 0x80, 2), t2 = textureImage(16, 16, 0x80, 3), t6 = textureImage(256, 256, 0x80, 4);
    w.setTexture(0, 0, 64, 64, t0.data());
    w.setTexture(0, 1, 128, 32, t1.data());
    w.setTexture(0, 2, 16, 16, t2.data());
    w.setTexture(1, 6, 256, 256, t6.data());
    const std::vector<Frame> list = frames();
    const uint32_t outputs[3][3] = {{640, 448, 1}, {1280, 896, 4}, {960, 672, 2}};
    for (const auto& o : outputs) {
        w.label("output " + std::to_string(o[0]) + "x" + std::to_string(o[1]) + "x" + std::to_string(o[2]));
        w.configure(o[0], o[1], o[2]);
        for (int t = 0; t < 5; ++t) w.readTarget(t, 0);
        for (size_t i = 0; i < list.size(); ++i) {
            w.draw(list[i]);
            for (int t = 0; t < 5; ++t) w.readTarget(t, 0);
        }
        w.present(1280, 720, 0, 4.0f / 3.0f, 0);
        w.present(800, 900, 0, 640.0f / 448.0f, 0);
        w.present(3, 2, 1, 4.0f / 3.0f, 0);
        w.present(1000, 700, 3, 640.0f / 448.0f, 0);
    }
    // Re-uploading a texture of the same number replaces it.
    const auto t0b = textureImage(32, 8, 0x80, 9);
    w.setTexture(0, 0, 32, 8, t0b.data());
    w.draw(list[8 * 4 + 12]);
    for (int t = 0; t < 5; ++t) w.readTarget(t, 0);
    std::printf("synthetic: %zu frames x 3 outputs\n", list.size());
    return 0;
}
