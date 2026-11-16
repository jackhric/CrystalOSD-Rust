#include <algorithm>
#include <cstdint>
#include <cstdio>
int main() {
    uint64_t h = 0xcbf29ce484222325ull;
    for (float volume : {0.0f, 0.25f, 0.5f, 0.7f, 0.999f, 1.5f, 3.0f, 0.1f, 0.33333334f}) {
        for (int32_t x = -32768; x <= 32767; ++x) {
            const int16_t v = int16_t(std::clamp(int32_t(float(int16_t(x)) * volume), -32768, 32767));
            h = (h ^ uint8_t(v)) * 0x100000001b3ull;
            h = (h ^ uint8_t(uint16_t(v) >> 8)) * 0x100000001b3ull;
        }
    }
    std::printf("0x%016llx\n", (unsigned long long)h);
}
