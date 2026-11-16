#include <cstring>
#include <functional>
#include <optional>

#include "Scene/Arithmetic.hpp"
#include "common.hpp"

using namespace scene;

constexpr uint32_t N = 1u << 20;

// Guarded: NativeArithmetic under EeRounding, as coldStart and coldFont run it.
template <class A, bool Guarded>
struct Run {
    std::string tag;

    template <class F>
    void binary(const char* op, F f) {
        for (int kind = 0; kind < 3; ++kind) {
            const auto set = pairSet(kind, N);
            std::vector<float> out(set.size());
            {
                std::optional<EeRounding> g;
                if (Guarded) g.emplace();
                for (size_t i = 0; i < set.size(); ++i) out[i] = f(asF(set[i].first), asF(set[i].second));
            }
            Hash h;
            for (float x : out) h.f32(x);
            report(tag + " " + op + " pairs" + std::to_string(kind), h);
        }
    }

    template <class R, class F>
    void unary(const char* op, F f) {
        for (int kind = 0; kind < 3; ++kind) {
            const auto set = singleSet(kind, N);
            std::vector<R> out(set.size());
            {
                std::optional<EeRounding> g;
                if (Guarded) g.emplace();
                for (size_t i = 0; i < set.size(); ++i) out[i] = f(asF(set[i]));
            }
            Hash h;
            for (R x : out) {
                if constexpr (std::is_same_v<R, float>) h.f32(x);
                else h.u32(static_cast<uint32_t>(x));
            }
            report(tag + " " + op + " singles" + std::to_string(kind), h);
        }
    }

    void all() {
        binary("add", [](float a, float b) { return A::add(a, b); });
        binary("sub", [](float a, float b) { return A::sub(a, b); });
        binary("mul", [](float a, float b) { return A::mul(a, b); });
        binary("div", [](float a, float b) { return A::div(a, b); });
        binary("vu0Quotient", [](float a, float b) { return A::vu0Quotient(std::fabs(a), std::fabs(b)); });
        binary("addExact", [](float a, float b) { return A::addExact(a, b); });
        binary("subExact", [](float a, float b) { return A::subExact(a, b); });
        binary("quotientExact", [](float a, float b) { return A::quotientExact(a, b); });
        unary<float>("sqrt", [](float a) { return A::sqrt(a); });
        unary<float>("vu0Root", [](float a) { return A::vu0Root(a); });
        unary<float>("rootExact", [](float a) { return A::rootExact(a); });
        unary<int32_t>("toInt", [](float a) { return A::toInt(a); });
        unary<uint32_t>("toUnsigned", [](float a) { return A::toUnsigned(a); });
        for (int kind = 0; kind < 3; ++kind) {
            const auto set = doubleSet(kind, N);
            std::vector<float> out(set.size());
            {
                std::optional<EeRounding> g;
                if (Guarded) g.emplace();
                for (size_t i = 0; i < set.size(); ++i) out[i] = A::cut(std::bit_cast<double>(set[i]));
            }
            Hash h;
            for (float x : out) h.f32(x);
            report(tag + " cut doubles" + std::to_string(kind), h);
        }
        {
            const auto set = intSet(N);
            std::vector<float> out(set.size());
            {
                std::optional<EeRounding> g;
                if (Guarded) g.emplace();
                volatile int32_t sink;
                for (size_t i = 0; i < set.size(); ++i) {
                    sink = set[i];
                    out[i] = static_cast<float>(sink);
                }
            }
            Hash h;
            for (float x : out) h.f32(x);
            report(tag + " fromInt ints", h);
        }
        if (Guarded) return;
        {
            Hash s, c;
            for (int32_t a = -0x20000; a < 0x20000; ++a) {
                s.f32(A::sin16(a));
                c.f32(A::cos16(a));
            }
            Rng r(0xa961e000ull);
            for (uint32_t i = 0; i < 65536; ++i) {
                const int32_t a = static_cast<int32_t>(r.next32());
                s.f32(A::sin16(a));
                c.f32(A::cos16(a));
            }
            report(tag + " sin16 angles", s);
            report(tag + " cos16 angles", c);
        }
        for (int kind = 0; kind < 2; ++kind) {
            const auto set = trigSet(kind, 4u << 20);
            Hash s, c;
            for (uint32_t b : set) {
                s.f32(A::sinf(asF(b)));
                c.f32(A::cosf(asF(b)));
            }
            report(tag + " sinf trig" + std::to_string(kind), s);
            report(tag + " cosf trig" + std::to_string(kind), c);
        }
        for (int kind = 2; kind < 4; ++kind) {
            const auto set = trigSet(kind, N);
            Hash h;
            for (uint32_t b : set) {
                const auto [s, c] = A::sineCosine(asF(b));
                h.f32(s);
                h.f32(c);
            }
            report(tag + " sineCosine trig" + std::to_string(kind), h);
        }
    }
};

int main(int argc, char** argv) {
    std::setvbuf(stdout, nullptr, _IOLBF, 0);
    const std::string which = argc > 1 ? argv[1] : "all";
    if (which == "all" || which == "ee") Run<EeArithmetic, false>{"ee"}.all();
    if (which == "all" || which == "native") Run<NativeArithmetic, false>{"native"}.all();
    if (which == "all" || which == "truncating") Run<NativeArithmetic, true>{"truncating"}.all();
    return 0;
}
