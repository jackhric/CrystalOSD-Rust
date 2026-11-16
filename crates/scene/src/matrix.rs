use std::marker::PhantomData;

use crate::arithmetic::Arithmetic;

pub type Vec3 = [f32; 3];
pub type Vec4 = [f32; 4];
pub type Mat4 = [Vec4; 4];

// The clock's matrix routines, rows as the EE holds them (facts/clock-camera.md).
pub struct Matrix<A>(PhantomData<A>);

impl<A: Arithmetic> Matrix<A> {
    pub fn identity() -> Mat4 {
        [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
    }

    // sceVu0ApplyMatrix: VMULAx, VMADDAy, VMADDAz, VMADDw.
    pub fn apply(m: &Mat4, v: &Vec4) -> Vec4 {
        let mut out = [0.0f32; 4];
        for (i, o) in out.iter_mut().enumerate() {
            *o = A::add(A::add(A::add(A::mul(m[0][i], v[0]), A::mul(m[1][i], v[1])), A::mul(m[2][i], v[2])), A::mul(m[3][i], v[3]));
        }
        out
    }

    // sceVu0MulMatrix(out, a, b): each row of b through a.
    pub fn multiply(a: &Mat4, b: &Mat4) -> Mat4 {
        let mut out = [[0.0f32; 4]; 4];
        for (o, row) in out.iter_mut().zip(b) {
            *o = Self::apply(a, row);
        }
        out
    }

    pub fn rotate_x(m: &Mat4, angle: i32) -> Mat4 {
        let c = A::cos16(angle);
        let s = A::sin16(angle);
        Self::multiply(m, &[[1.0, 0.0, 0.0, 0.0], [0.0, c, s, 0.0], [0.0, -s, c, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }

    pub fn rotate_y(m: &Mat4, angle: i32) -> Mat4 {
        let c = A::cos16(angle);
        let s = A::sin16(angle);
        Self::multiply(m, &[[c, 0.0, -s, 0.0], [0.0, 1.0, 0.0, 0.0], [s, 0.0, c, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }

    pub fn rotate_z(m: &Mat4, angle: i32) -> Mat4 {
        let c = A::cos16(angle);
        let s = A::sin16(angle);
        Self::multiply(m, &[[c, s, 0.0, 0.0], [-s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }

    pub fn move_(m: &Mat4, x: f32, y: f32, z: f32) -> Mat4 {
        [m[0], m[1], m[2], Self::apply(m, &[x, y, z, 1.0])]
    }

    pub fn normalize(v: &Vec3) -> Vec3 {
        let length = A::sqrt(A::add(A::add(A::mul(v[0], v[0]), A::mul(v[1], v[1])), A::mul(v[2], v[2])));
        let inverse = A::div(1.0, length);
        [A::mul(v[0], inverse), A::mul(v[1], inverse), A::mul(v[2], inverse)]
    }

    pub fn dot(a: &Vec3, b: &Vec3) -> f32 {
        A::add(A::add(A::mul(a[0], b[0]), A::mul(a[1], b[1])), A::mul(a[2], b[2]))
    }

    // sceVu0ViewScreenMatrix.
    #[allow(clippy::too_many_arguments)]
    pub fn view_screen(scrz: f32, ax: f32, ay: f32, cx: f32, cy: f32, zmin: f32, zmax: f32, nearz: f32, farz: f32) -> Mat4 {
        let range = A::add(-zmin, zmax);
        let depth = A::add(-nearz, farz);
        let az = A::div(A::mul(A::mul(farz, nearz), range), depth);
        let cz = A::div(A::add(A::mul(-zmax, nearz), A::mul(zmin, farz)), depth);
        let m = [[scrz, 0.0, 0.0, 0.0], [0.0, scrz, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0]];
        let mt = [[ax, 0.0, 0.0, 0.0], [0.0, ay, 0.0, 0.0], [0.0, 0.0, az, 0.0], [cx, cy, cz, 1.0]];
        Self::multiply(&mt, &m)
    }

    // The view matrix builder (HDD module_clock_238DC0): rotation X, Y, Z in radians, the three
    // vectors through it, then sceVu0CameraMatrix.
    pub fn view_matrix(position: &Vec4, direction: &Vec4, up: &Vec4, rotation: &Vec4) -> Mat4 {
        let mut m = Self::identity();
        m = Self::camera_rotate_z(&Self::camera_rotate_y(&Self::camera_rotate_x(&m, rotation[0]), rotation[1]), rotation[2]);
        let zd = Self::apply(&m, direction);
        let yd = Self::apply(&m, up);
        let p = Self::apply(&m, position);
        let x = Self::camera_normalize(&Self::outer(&yd, &zd));
        let z = Self::camera_normalize(&zd);
        let y = Self::outer(&z, &x);
        let t = [A::add(0.0, p[0]), A::add(0.0, p[1]), A::add(0.0, p[2]), 1.0];
        let mut out = [[0.0f32; 4]; 4];
        for (i, row) in out.iter_mut().take(3).enumerate() {
            *row = [x[i], y[i], z[i], 0.0];
        }
        #[allow(clippy::needless_range_loop)]
        for i in 0..3 {
            out[3][i] = A::sub(0.0, A::add(A::add(A::mul(out[0][i], t[0]), A::mul(out[1][i], t[1])), A::mul(out[2][i], t[2])));
        }
        out[3][3] = t[3];
        out
    }

    fn camera_rotate_x(m: &Mat4, radians: f32) -> Mat4 {
        let (s, c) = A::sine_cosine(radians);
        Self::multiply(&[[1.0, 0.0, 0.0, 0.0], [0.0, c, s, 0.0], [0.0, A::sub(0.0, s), c, 0.0], [0.0, 0.0, 0.0, 1.0]], m)
    }

    fn camera_rotate_y(m: &Mat4, radians: f32) -> Mat4 {
        let (s, c) = A::sine_cosine(radians);
        Self::multiply(&[[c, 0.0, A::sub(0.0, s), 0.0], [0.0, 1.0, 0.0, 0.0], [s, 0.0, c, 0.0], [0.0, 0.0, 0.0, 1.0]], m)
    }

    fn camera_rotate_z(m: &Mat4, radians: f32) -> Mat4 {
        let (s, c) = A::sine_cosine(radians);
        Self::multiply(&[[c, s, 0.0, 0.0], [A::sub(0.0, s), c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]], m)
    }

    fn outer(a: &Vec4, b: &Vec4) -> Vec4 {
        [A::sub(A::mul(a[1], b[2]), A::mul(b[1], a[2])), A::sub(A::mul(a[2], b[0]), A::mul(b[2], a[0])), A::sub(A::mul(a[0], b[1]), A::mul(b[0], a[1])), 0.0]
    }

    fn camera_normalize(v: &Vec4) -> Vec4 {
        let length = A::add(0.0, A::vu0_root(A::add(A::add(A::mul(v[0], v[0]), A::mul(v[1], v[1])), A::mul(v[2], v[2]))));
        let q = A::vu0_quotient(1.0, length);
        [A::mul(v[0], q), A::mul(v[1], q), A::mul(v[2], q), 0.0]
    }
}
