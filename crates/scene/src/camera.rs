use std::marker::PhantomData;

use crate::arithmetic::Arithmetic;
use crate::clock_state::ClockState;
use crate::matrix::{Mat4, Matrix};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraMatrices {
    pub view: Mat4,
    pub screen: Mat4,
}

// facts/clock-camera.md: the frame's two matrices; the approach offset is added to the position's z and then decays.
pub struct Camera<A>(PhantomData<A>);

impl<A: Arithmetic> Camera<A> {
    // facts/clock-camera.md: the screen matrix, the view matrix with the approach offset added to the position's z, the offset's decay.
    pub fn matrices(clock: &mut ClockState) -> CameraMatrices {
        let screen = Matrix::<A>::view_screen(512.0, clock.proportions.ax, clock.proportions.ay, 2048.0, 2048.0, 1.0, clock.zmax, 1.0, 65536.0);
        let mut position = clock.position;
        let offset = clock.camera_offset;
        position[2] = A::add(position[2], offset);
        let view = Matrix::<A>::view_matrix(&position, &clock.direction, &clock.up, &clock.rotation);
        clock.camera_offset = A::mul(offset, clock.camera_factor);
        CameraMatrices { view, screen }
    }
}
