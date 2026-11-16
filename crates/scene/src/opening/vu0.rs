use std::marker::PhantomData;

use crate::arithmetic::Arithmetic;
use crate::matrix::{Mat4, Vec4};

// libvu0 as the opening calls it (facts/opening.md 4.6): References/model/opening-lib.mjs with the exact add, sub,
// quotient and root of its _v2 verifiers; rows as the EE holds them.
pub struct Vu0<A>(PhantomData<A>);

impl<A: Arithmetic> Vu0<A> {
    pub fn unit_matrix() -> Mat4 {
        [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
    }

    // opening-lib combine: rows[0] * v.x + rows[1] * v.y + rows[2] * v.z + rows[3] * v.w.
    pub fn apply(rows: &Mat4, v: &Vec4) -> Vec4 {
        let mut out = [0.0f32; 4];
        for (i, o) in out.iter_mut().enumerate() {
            *o = A::add_exact(
                A::add_exact(A::add_exact(A::mul(rows[0][i], v[0]), A::mul(rows[1][i], v[1])), A::mul(rows[2][i], v[2])),
                A::mul(rows[3][i], v[3]),
            );
        }
        out
    }

    // sceVu0MulMatrix 0x0027AE48: each row of b through a.
    pub fn mul_matrix(a: &Mat4, b: &Mat4) -> Mat4 {
        let mut out = [[0.0f32; 4]; 4];
        for (o, row) in out.iter_mut().zip(b) {
            *o = Self::apply(a, row);
        }
        out
    }

    // sceVu0RotMatrix 0x0027B3D8: Z, then Y, then X (angles x, y, z), each through _sceVu0ecossin.
    pub fn rot_matrix(m: &Mat4, angles: &Vec4) -> Mat4 {
        Self::rot_x(&Self::rot_y(&Self::rot_z(m, angles[2]), angles[1]), angles[0])
    }

    pub fn rot_matrix_unit(angles: &Vec4) -> Mat4 {
        Self::rot_matrix(&Self::unit_matrix(), angles)
    }

    // sceVu0TransMatrix 0x0027B098: rows 0..2 copied, row 3 xyz plus t xyz, w kept.
    pub fn trans_matrix(m: &Mat4, t: &Vec4) -> Mat4 {
        [m[0], m[1], m[2], [A::add_exact(m[3][0], t[0]), A::add_exact(m[3][1], t[1]), A::add_exact(m[3][2], t[2]), m[3][3]]]
    }

    // sceVu0SubVector 0x0027B050.
    pub fn sub_vector(a: &Vec4, b: &Vec4) -> Vec4 {
        [A::sub_exact(a[0], b[0]), A::sub_exact(a[1], b[1]), A::sub_exact(a[2], b[2]), A::sub_exact(a[3], b[3])]
    }

    // sceVu0CameraMatrix 0x0027B450 (opening-lib cameraMatrix(p, zd, yd)).
    pub fn camera_matrix(position: &Vec4, direction: &Vec4, up: &Vec4) -> Mat4 {
        let x = Self::normalize(&Self::outer(up, direction));
        let z = Self::normalize(direction);
        let y = Self::outer(&z, &x);
        let t = [position[0], position[1], position[2], 1.0f32];
        let mut out = [[0.0f32; 4]; 4];
        for (i, row) in out.iter_mut().take(3).enumerate() {
            *row = [x[i], y[i], z[i], 0.0];
        }
        for i in 0..3 {
            out[3][i] = A::sub_exact(0.0, A::add_exact(A::add_exact(A::mul(out[0][i], t[0]), A::mul(out[1][i], t[1])), A::mul(out[2][i], t[2])));
        }
        out[3][3] = t[3];
        out
    }

    // sceVu0ViewScreenMatrix 0x0027B628 (opening-lib viewScreenMatrix).
    pub fn view_screen_matrix(distance: f32, ax: f32, ay: f32, cx: f32, cy: f32, zmin: f32, zmax: f32, nearz: f32, farz: f32) -> Mat4 {
        let depth = A::add_exact(-nearz, farz);
        let cz = A::quotient_exact(A::add_exact(A::mul(-zmax, nearz), A::mul(zmin, farz)), depth);
        let az = A::quotient_exact(A::mul(A::mul(farz, nearz), A::add_exact(-zmin, zmax)), depth);
        let mut first = Self::unit_matrix();
        first[0][0] = distance;
        first[1][1] = distance;
        first[2][2] = 0.0;
        first[3][3] = 0.0;
        first[2][3] = 1.0;
        first[3][2] = 1.0;
        let mut second = Self::unit_matrix();
        second[0][0] = ax;
        second[1][1] = ay;
        second[2][2] = az;
        second[3][0] = cx;
        second[3][1] = cy;
        second[3][2] = cz;
        Self::mul_matrix(&second, &first)
    }

    // sceVu0NormalLightMatrix (verify_opening_inputs.mjs normalLight): the three light directions negated and
    // normalised become the columns of the matrix; the fourth column is (0, 0, 0, 1).
    pub fn normal_light_matrix(l0: &Vec4, l1: &Vec4, l2: &Vec4) -> Mat4 {
        let rows = [Self::normalize(&Self::negate(l0)), Self::normalize(&Self::negate(l1)), Self::normalize(&Self::negate(l2)), [0.0, 0.0, 0.0, 1.0]];
        let mut out = [[0.0f32; 4]; 4];
        for (r, row) in out.iter_mut().enumerate() {
            for (c, x) in row.iter_mut().enumerate() {
                *x = rows[c][r];
            }
        }
        out
    }

    // sceVu0ClipAll 0x0027BA70 (verify_opening_lights_v2.mjs clipAll): true when no vertex is strictly inside
    // min..max in x and y (scaled by w) and in w.
    pub fn clip_all(min: &Vec4, max: &Vec4, m: &Mat4, vertices: &[Vec4]) -> bool {
        for v in vertices {
            let p = Self::apply(m, v);
            let d = [
                A::sub_exact(p[0], A::mul(min[0], p[3])),
                A::sub_exact(p[1], A::mul(min[1], p[3])),
                A::sub_exact(p[3], min[3]),
                A::sub_exact(A::mul(max[0], p[3]), p[0]),
                A::sub_exact(A::mul(max[1], p[3]), p[1]),
                A::sub_exact(max[3], p[3]),
            ];
            let mut inside = true;
            for x in d {
                inside = inside && x > 0.0;
            }
            if inside {
                return false;
            }
        }
        true
    }

    fn rot_x(m: &Mat4, a: f32) -> Mat4 {
        let (s, c) = A::sine_cosine(a);
        Self::mul_matrix(&[[1.0, 0.0, 0.0, 0.0], [0.0, c, s, 0.0], [0.0, Self::minus(s), c, 0.0], [0.0, 0.0, 0.0, 1.0]], m)
    }

    fn rot_y(m: &Mat4, a: f32) -> Mat4 {
        let (s, c) = A::sine_cosine(a);
        Self::mul_matrix(&[[c, 0.0, Self::minus(s), 0.0], [0.0, 1.0, 0.0, 0.0], [s, 0.0, c, 0.0], [0.0, 0.0, 0.0, 1.0]], m)
    }

    fn rot_z(m: &Mat4, a: f32) -> Mat4 {
        let (s, c) = A::sine_cosine(a);
        Self::mul_matrix(&[[c, s, 0.0, 0.0], [Self::minus(s), c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]], m)
    }

    fn minus(s: f32) -> f32 {
        A::cut(0.0 - f64::from(s))
    }

    fn negate(l: &Vec4) -> Vec4 {
        [A::mul(l[0], -1.0), A::mul(l[1], -1.0), A::mul(l[2], -1.0), A::mul(l[3], -1.0)]
    }

    fn outer(a: &Vec4, b: &Vec4) -> Vec4 {
        [
            A::sub_exact(A::mul(a[1], b[2]), A::mul(b[1], a[2])),
            A::sub_exact(A::mul(a[2], b[0]), A::mul(b[2], a[0])),
            A::sub_exact(A::mul(a[0], b[1]), A::mul(b[0], a[1])),
            0.0,
        ]
    }

    fn normalize(v: &Vec4) -> Vec4 {
        let length = A::root_exact(A::add_exact(A::add_exact(A::mul(v[0], v[0]), A::mul(v[1], v[1])), A::mul(v[2], v[2])));
        let q = A::quotient_exact(1.0, length);
        [A::mul(v[0], q), A::mul(v[1], q), A::mul(v[2], q), 0.0]
    }
}
