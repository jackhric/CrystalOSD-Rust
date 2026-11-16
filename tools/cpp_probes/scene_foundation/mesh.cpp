#include "Scene/Rods.hpp"
#include <cstdio>
int main(int c, char** v) {
    try {
        const scene::RodMesh m = scene::loadRodMesh(v[1]);
        for (const auto* part : {&m.positions, &m.normals, &m.coordinates}) { for (const auto& x : *part) for (float f : x) std::printf("%08x ", scene::floatBits(f)); std::printf("\n"); }
    } catch (const std::exception& e) { std::printf("error: %s\n", e.what()); }
}
