#include <cstdio>
#include "session.hpp"

static void hrect(Hash& h, const Rect& r) { for (int32_t x : r.colour) h.i32(x); for (int32_t x : {r.x0, r.y0, r.u0, r.v0, r.x1, r.y1, r.u1, r.v1, r.z, r.blend, r.textured}) h.i32(x); }
static void hdraw(Hash& h, const HeadDraw& d) {
    for (int x : {int(d.part), int(d.target), int(d.source), int(d.alpha), d.depthTest, int(d.topology), int(d.textured), int(d.blended), int(d.gouraud), int(d.coordinates)}) h.i32(x);
    h.u32(uint32_t(d.vertices.size()));
    for (const HeadVertex& v : d.vertices) { h.f32(v.x); h.f32(v.y); h.u32(v.z); h.u32(v.fog); for (float x : {v.r, v.g, v.b, v.a, v.q, v.s, v.t, v.u, v.v}) h.f32(x); }
}

template <class A>
void run(const char* tag, const ColdAssets& assets) {
    Hash h;
    Rng r(0xc0be5000);
    const auto u = [&](uint32_t n) { return r.next32() % n; };
    for (int c = 0; c < 60; ++c) {
        ColdInputs in;
        in.pal = u(2); in.screenHeight = in.pal ? 0x100 : 0xE0; in.rtc = {2026, 1, 1, 0, 0, 0};
        ColdStartOut cold = coldStart<A>(assets, in);
        CubeState q = cold.clock.menus->cubes;
        HeadState head = cold.clock.head;
        ClockState clock = cold.clock.state;
        Cubes<A> cubes(cold.clock.menus->cubeMesh);
        q.list.position = int32_t(u(180000));
        q.list.left = int32_t(u(40000)) - 20000;
        q.list.speed = int32_t(u(5000));
        q.list.slowing = int32_t(u(300));
        q.list.pulse = u(2) ? cold.clock.menus->menus.listConstants.pulse : 0.0f;
        q.list.pulsed = int32_t(u(60));
        const int32_t body = cold.clock.menus->menus.body;
        q.ramp.state = int32_t(u(4));
        q.ramp.counter = q.ramp.state == 0 ? 0 : q.ramp.state == 2 ? q.ramp.length : int32_t(u(uint32_t(q.ramp.length) + 1));
        clock.spin = uint16_t(u(65536));
        for (int f = 0; f < 30; ++f) {
            clock.level = int32_t(u(7));
            if (u(10) == 0) { q.list.left += int32_t(u(12000)) - 6000; q.list.speed = int32_t(u(4000)); }
            clock.spin = uint16_t(*clock.spin + 0x1e);
            const std::vector<CubeDraw> draws = cubes.frame(q, head, clock, {640, cold.clock.height, int32_t(u(2)), body});
            h.u32(uint32_t(draws.size()));
            for (const CubeDraw& d : draws) {
                h.i32(int(d.send)); hbytes(h, d.label);
                h.u32(uint32_t(d.faces.size()));
                for (const RodFaceDraw& face : d.faces) {
                    h.i32(face.face); h.i32(int(face.piece)); h.boolean(face.edgeSmoothing);
                    for (const RodVertex& v : face.strip) for (float x : {v.x, v.y, v.z, v.u, v.v, v.q, v.r, v.g, v.b, v.a}) h.f32(x);
                }
                h.boolean(d.rectangle.has_value()); if (d.rectangle) hdraw(h, *d.rectangle);
                for (int32_t x : d.clear) h.i32(x);
            }
            h.i32(q.list.position); h.i32(q.list.left); h.i32(q.list.speed); h.f32(q.list.pulse); hramp(h, q.ramp);
            for (const auto& row : q.record.local) for (float x : row) h.f32(x);
            h.f32(q.record.sx); for (int32_t x : q.record.base) h.i32(x); for (int32_t x : q.record.reflection) h.i32(x);
            hrect(h, q.added); hrect(h, q.half); hrect(h, q.chain); hrect(h, head.copy);
        }
    }
    report(std::string(tag) + " cubes", h);
}

int main() {
    const ColdAssets assets = coldAssets();
    run<NativeArithmetic>("native", assets);
#ifndef SCENE_NATIVE_ONLY
    run<EeArithmetic>("ee", assets);
#endif
}
