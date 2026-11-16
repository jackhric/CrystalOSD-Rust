// Linked into the real CrystalOSD app objects with -Wl,--wrap=<NativeRenderer member>: every call the app makes is
// recorded to $CRYSTALOSD_STREAM, then forwarded to the real renderer unchanged.
#include <cstdlib>
#include <memory>

#include "Render/NativeRenderer.hpp"
#include "Scene/Text.hpp"
#include "Stream.hpp"

using render::NativeOutput;
using render::NativeRenderer;

namespace {
stream::Writer& out() {
    static std::unique_ptr<stream::Writer> w = [] {
        const char* path = std::getenv("CRYSTALOSD_STREAM");
        if (!path) {
            std::fprintf(stderr, "wrap: CRYSTALOSD_STREAM is not set\n");
            std::abort();
        }
        return std::make_unique<stream::Writer>(path);
    }();
    return *w;
}
int depth = 0;
struct Depth {
    Depth() { ++depth; }
    ~Depth() { --depth; }
};
}  // namespace

extern "C" {
void __real__ZN6render14NativeRenderer4drawERKN5scene5FrameE(NativeRenderer*, const scene::Frame&);
void __real__ZN6render14NativeRenderer9configureERKNS_12NativeOutputE(NativeRenderer*, const NativeOutput&);
void __real__ZN6render14NativeRenderer10setTextureEijjSt4spanIKhLm18446744073709551615EE(NativeRenderer*, int32_t, uint32_t, uint32_t, std::span<const uint8_t>);
void __real__ZN6render14NativeRenderer10setTextureEN5scene10TextureSetEijjSt4spanIKhLm18446744073709551615EE(NativeRenderer*, scene::TextureSet, int32_t, uint32_t, uint32_t,
                                                                                                          std::span<const uint8_t>);
void __real__ZN6render14NativeRenderer13setGlyphCacheERKN5scene4FontERKNS1_10GlyphCacheE(NativeRenderer*, const scene::Font&, const scene::GlyphCache&);
std::vector<uint8_t> __real__ZN6render14NativeRenderer10readTargetEN5scene10TargetNameE(NativeRenderer*, scene::TargetName);

void __wrap__ZN6render14NativeRenderer4drawERKN5scene5FrameE(NativeRenderer* self, const scene::Frame& frame) {
    if (depth == 0) out().draw(frame);
    Depth d;
    __real__ZN6render14NativeRenderer4drawERKN5scene5FrameE(self, frame);
}

void __wrap__ZN6render14NativeRenderer9configureERKNS_12NativeOutputE(NativeRenderer* self, const NativeOutput& o) {
    if (depth == 0) out().configure(o.width, o.height, o.samples);
    Depth d;
    __real__ZN6render14NativeRenderer9configureERKNS_12NativeOutputE(self, o);
}

void __wrap__ZN6render14NativeRenderer10setTextureEijjSt4spanIKhLm18446744073709551615EE(NativeRenderer* self, int32_t n, uint32_t w, uint32_t h, std::span<const uint8_t> rgba) {
    if (depth == 0 && rgba.size() == size_t(w) * h * 4) out().setTexture(0, n, w, h, rgba.data());
    Depth d;
    __real__ZN6render14NativeRenderer10setTextureEijjSt4spanIKhLm18446744073709551615EE(self, n, w, h, rgba);
}

void __wrap__ZN6render14NativeRenderer10setTextureEN5scene10TextureSetEijjSt4spanIKhLm18446744073709551615EE(NativeRenderer* self, scene::TextureSet set, int32_t n, uint32_t w,
                                                                                                          uint32_t h, std::span<const uint8_t> rgba) {
    if (depth == 0 && rgba.size() == size_t(w) * h * 4) out().setTexture(static_cast<int32_t>(set), n, w, h, rgba.data());
    Depth d;
    __real__ZN6render14NativeRenderer10setTextureEN5scene10TextureSetEijjSt4spanIKhLm18446744073709551615EE(self, set, n, w, h, rgba);
}

void __wrap__ZN6render14NativeRenderer13setGlyphCacheERKN5scene4FontERKNS1_10GlyphCacheE(NativeRenderer* self, const scene::Font& font, const scene::GlyphCache& cache) {
    static scene::GlyphCache last;
    static bool any = false;
    if (depth == 0) {
        // Every call is recorded (the renderer decides whether it re-uploads); the image only when the cache changed.
        std::vector<uint8_t> image;
        if (!any || !(cache == last)) image = scene::glyphCacheImage(font, cache);
        out().glyphCache(cache, image);
        last = cache;
        any = true;
    }
    Depth d;
    __real__ZN6render14NativeRenderer13setGlyphCacheERKN5scene4FontERKNS1_10GlyphCacheE(self, font, cache);
}

std::vector<uint8_t> __wrap__ZN6render14NativeRenderer10readTargetEN5scene10TargetNameE(NativeRenderer* self, scene::TargetName t) {
    std::vector<uint8_t> r;
    {
        Depth d;
        r = __real__ZN6render14NativeRenderer10readTargetEN5scene10TargetNameE(self, t);
    }
    if (depth == 0) {
        out().readTarget(static_cast<int32_t>(t), stream::fnv(r.data(), r.size()));
        out().flush();
    }
    return r;
}
}
