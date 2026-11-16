#include <cstdio>
#include <fstream>
#include <iterator>
#include "Scene/ColdStart.hpp"
#include "Scene/ElfRecords.hpp"
using namespace scene;
static std::vector<uint8_t> readAll(const char* p) { std::ifstream f(p, std::ios::binary); return {std::istreambuf_iterator<char>(f), {}}; }
int main() {
    auto program = std::make_shared<const ProgramImage>(readAll("/home/jackh/Documents/git/CrystalOSD/Resources/hddosd.elf"));
    for (int pal = 0; pal < 2; ++pal) {
        ColdAssets a{program, nullptr, {}, {}};
        ColdInputs in; in.pal = pal; in.rtc = {2026,1,2,3,4,5};
        auto s = coldStart<NativeArithmetic>(a, in);
        auto& m = s.clock.menus->menus;
        std::printf("pal %d count %d entries %x selected %d gate %d\n", pal, m.page.count, m.page.entries, m.page.selected, *m.configGate);
        for (int i = 0; i < 9; ++i) { auto& e = m.entries[i]; std::printf(" %d id %x n %d idx %d item %d table %x enter %x str %x frame %x conf %x canc %x\n", i, e.id, e.valueCount, e.valueIndex, e.item, e.valueTable, e.enter, e.stringCallback, e.frameCallback, e.confirm, e.cancel); }
        for (auto& f : m.adjustFields) std::printf(" field %d %d %d\n", f.item, f.lowest, f.highest);
        std::printf(" items:"); for (int x : s.clock.menus->items) std::printf(" %d", x); std::printf("\n");
    }
}
