// The clock's ramp record (References/model/clock_math.mjs tickRamp; facts/clock-state.md).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ramp {
    pub length: i32,
    pub counter: i32,
    pub changed: i32,
    pub state: i32,
}

pub fn tick_ramp(ramp: &mut Ramp) {
    ramp.changed = 0;
    if ramp.state == 1 {
        ramp.counter += 1;
        if ramp.counter == ramp.length {
            ramp.changed = 1;
            ramp.state = 2;
        }
    } else if ramp.state == 3 {
        ramp.counter -= 1;
        if ramp.counter == 0 {
            ramp.changed = 1;
            ramp.state = 0;
        }
    }
}
