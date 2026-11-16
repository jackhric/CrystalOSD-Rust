static void h4(Hash& h, const Vec4& v) { for (float x : v) h.f32(x); }
static void hm(Hash& h, const Mat4& m) { for (const Vec4& r : m) h4(h, r); }
static void hc(Hash& h, const Colour& c) { for (int32_t x : c) h.i32(x); }
static void hrect(Hash& h, const Rect& r) { hc(h, r.colour); for (int32_t x : {r.x0, r.y0, r.u0, r.v0, r.x1, r.y1, r.u1, r.v1, r.z, r.blend, r.textured}) h.i32(x); }
static void hrecord(Hash& h, const RodRecord& r) {
    h.i32(r.number); h.i32(r.faces); hm(h, r.local); h.f32(r.sx); h.f32(r.sy); h.f32(r.sz); hc(h, r.base); h.f32(r.strength); hc(h, r.textured);
    h.f32(r.pair[0]); h.f32(r.pair[1]); h.f32(r.refraction); hc(h, r.reflection); hc(h, r.extra);
}
static void hclock(Hash& h, const ClockState& s) {
    h.f32(s.time.milliseconds); h.i32(s.time.seconds); h.i32(s.time.minutes); h.i32(s.time.hours);
    h.i32(s.eased.secondHand); h.i32(s.eased.hourHand); h.f32(s.eased.progress); h.f32(s.eased.fraction);
    h.i32(s.state.currentRod); h.i32(s.state.secondsAngle); h.i32(s.state.rodAngle);
    for (const RodState& r : s.state.rods) { h.f32(r.appearance); h.f32(r.progress); hc(h, r.base); hc(h, r.reflection); }
    hc(h, s.state.accent); hc(h, s.state.fourth); h.f32(s.state.progressTarget); hc(h, s.state.accentFrame); hc(h, s.state.currentTarget);
    hramp(h, s.appearance);
    hc(h, s.colours.base); hc(h, s.colours.reflection); hc(h, s.colours.fourth); hc(h, s.colours.currentReflection);
    h.i32(s.cycleCounters.base.index); h.i32(s.cycleCounters.base.wait); h.i32(s.cycleCounters.accent.index); h.i32(s.cycleCounters.accent.wait); h.i32(s.cycleCounters.last);
    for (const Colour& c : s.cycleTables.base) hc(h, c);
    for (const Colour& c : s.cycleTables.accent) hc(h, c);
    h.f32(s.logicConstants.rodEasing); h.f32(s.logicConstants.secondsEasing); h.f32(s.logicConstants.progressStep);
    h.f32(s.scene.scale); h.i32(s.scene.leaving); h.i32(s.scene.field);
    h.f32(s.scaleTarget); h.f32(s.scaleFactor); h.i32(s.timeFilled); h.boolean(s.spin.has_value()); h.u32(s.spin.value_or(0));
    h.i32(s.mode); h.i32(s.level); h.i32(s.overlayLevel); hramp(h, s.vignetteRamp); h.i32(s.vignetteLength); hramp(h, s.menuRamp); h.i32(s.tail); h.i32(s.counter);
    h.f32(s.proportions.ax); h.f32(s.proportions.ay); h4(h, s.position); h4(h, s.direction); h4(h, s.up); h4(h, s.rotation);
    h.f32(s.cameraOffset); h.f32(s.zmax); h.f32(s.cameraFactor);
}
static void hhead(Hash& h, const HeadState& s) {
    hramp(h, s.greyRamp); for (int32_t g : s.greys) h.i32(g); hramp(h, s.vignetteRamp);
    for (int32_t x : {s.ring.alpha, s.ring.cx, s.ring.cy, s.ring.rx, s.ring.ry, s.ring.z}) h.i32(x);
    for (const Rect* r : {&s.tint, &s.blur, &s.copy, &s.fade, &s.bars, &s.column}) hrect(h, *r);
}
static void hcache(Hash& h, const FontCache& c) {
    h.u32(uint32_t(c.list.size()));
    for (const FontCacheEntry& e : c.list) { h.i32(e.code); h.i32(e.loaded); h.i32(e.cell); h.u32(e.block); }
    for (int32_t x : {c.format, c.cellW, c.cellH, c.cells, c.width, c.height, c.logW, c.logH, c.setUp}) h.i32(x);
    h.u32(c.block); h.u32(c.memory); h.u32(c.texture); h.u32(c.table);
}
static void hentry(Hash& h, const ConfigEntry& e) {
    for (int32_t x : {e.id, e.valueCount, e.valueIndex, e.item}) h.i32(x);
    for (uint32_t x : {e.valueTable, e.enter, e.stringCallback, e.frameCallback, e.confirm, e.cancel, e.focus}) h.u32(x);
    for (int32_t x : e.rest) h.i32(x);
}
static void hrow(Hash& h, const VersionRow& r) { h.i32(r.label); hbytes(h, r.value); h.i32(r.subRows); h.i32(r.id); }
static void hversion(Hash& h, const VersionPage& v) {
    for (int32_t x : {v.title, v.count, v.shown, v.selected, v.first}) h.i32(x);
    for (const VersionRow& r : v.rows) hrow(h, r);
    for (int32_t x : {v.job, v.panelOn, v.panelAlpha, v.arrows}) h.i32(x);
    for (int32_t x : v.hints) h.i32(x);
    h.i32(v.polls);
}
static void hramps(Hash& h, const TextRamps& r) {
    for (const Ramp* x : {&r.config, &r.mainMenu, &r.version, &r.dialogClosing, &r.firstRun, &r.dialog}) hramp(h, *x);
    for (int32_t x : {r.lead, r.body, r.panel7, r.panel8On, r.panel8, r.adjustRow}) h.i32(x);
    for (int32_t x : r.panel8Ids) h.i32(x);
}

