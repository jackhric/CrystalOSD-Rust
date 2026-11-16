#include <cstdio>
#include <cstdlib>
#include <cstring>
#include "session.hpp"

int main(int argc, char** argv) {
    const ColdAssets assets = coldAssets();
    const bool perFrame = argc > 1 && std::strcmp(argv[1], "frames") == 0;
    const char* only = argc > 2 ? argv[2] : nullptr;
    for (const Scenario& s : scenarios()) {
        if (only && std::strcmp(only, s.name) != 0) continue;
        report(std::string("native ") + s.name, runScenario<NativeArithmetic>(assets, s, perFrame));
#ifndef SCENE_NATIVE_ONLY
        report(std::string("ee ") + s.name, runScenario<EeArithmetic>(assets, s, perFrame));
#endif
    }
}
