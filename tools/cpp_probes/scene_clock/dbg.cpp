#include <cstdio>
#include "session.hpp"
int main() {
    const ColdAssets assets = coldAssets();
    const Scenario& s = scenarios()[0];
    ColdInputs cold = hostInputs(s.pal, goldenNow(s.start, 0), s.zone, s.summer);
    cold.language = s.language; cold.aspect = s.aspect; cold.dateFormat = s.dateFormat; cold.timeFormat = s.timeFormat; cold.wide = s.wide; cold.randState = s.rand; cold.gsAllocator = 770048;
    Session<NativeArithmetic> session(assets, cold);
    auto presses = pressesOf(s);
    const CubeState& q = *session.clock->cubes();
    std::printf("cube constants"); for (float c : q.constants) std::printf(" %a", c); std::printf("\n");
    for (int i = 0; i < 400; ++i) {
        for (const Press& p : presses) { if (p.at == uint64_t(i)) session.reader.down(p.bit); if (p.at + p.hold == uint64_t(i)) session.reader.up(p.bit); }
        Frame f = session.step(goldenNow(s.start, i));
        const MenusState& m = *session.clock->menus();
        if (i % 10 == 0 || session.inputs.menu.pad.pressed) std::printf("%d pressed %x mainramp %d/%d sel %d page %d/%d level %d psel %d notes %zu mode %d overlay %d\n", i, session.inputs.menu.pad.pressed, m.mainMenu.ramp.state, m.mainMenu.ramp.counter, m.mainMenu.selected, m.page.ramp.state, m.page.ramp.counter, m.page.level, m.page.selected, session.clock->notes().size(), session.clock->state().mode, session.clock->state().overlayLevel);
    }
}
