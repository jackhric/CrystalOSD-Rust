#include "Scene/Arithmetic.hpp"
#include "Scene/opening/Vu0.hpp"
#include "common.hpp"
using namespace scene; using namespace scene::opening;
float gen(Rng& r, int kind) { if (kind == 0) return asF(r.next32()); if (kind == 1) return asF(edgeFloats()[r.next() % edgeFloats().size()]); return static_cast<float>(static_cast<int32_t>(r.next32())) * (1.0f / 16777216.0f); }
Vec4 v4(Rng& r, int k) { return {gen(r,k),gen(r,k),gen(r,k),gen(r,k)}; }
Mat4 m4(Rng& r, int k) { return {v4(r,k),v4(r,k),v4(r,k),v4(r,k)}; }
void pm(const char* n, int i, const Mat4& m) { std::printf("%d %s", i, n); for (auto& r : m) for (float x : r) std::printf(" %08x", bitsF(x)); std::printf("\n"); }
void pv(const char* n, int i, const Vec4& m) { std::printf("%d %s", i, n); for (float x : m) std::printf(" %08x", bitsF(x)); std::printf("\n"); }
int main() {
  using V = Vu0<NativeArithmetic>; int kind = 0; Rng r(0x0b0e0000ull + kind);
  for (int n = 0; n < 4000; ++n) {
    const Mat4 a = m4(r, kind), b = m4(r, kind); const Vec4 v = v4(r, kind), w = v4(r, kind), u = v4(r, kind);
    pv("apply", n, V::apply(a, v)); pm("mul", n, V::mulMatrix(a, b)); pm("rot", n, V::rotMatrix(a, v)); pm("rotu", n, V::rotMatrix(w));
    pm("trans", n, V::transMatrix(a, w)); pv("sub", n, V::subVector(v, w)); pm("cam", n, V::cameraMatrix(v, w, u));
    float f[9]; for (float& x : f) x = gen(r, kind);
    pm("vs", n, V::viewScreenMatrix(f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7], f[8])); pm("nl", n, V::normalLightMatrix(v, w, u));
    const size_t count = 1 + r.next() % 8; std::vector<Vec4> vs; for (size_t i = 0; i < count; ++i) vs.push_back(v4(r, kind));
    std::printf("%d clip %d\n", n, (int)V::clipAll(v, w, a, vs));
  }
}
