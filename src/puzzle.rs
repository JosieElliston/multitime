//! 3x3x3 as a permutation group on 54 stickers. Knows nothing about time.

use std::sync::LazyLock;

pub type V3 = [i32; 3];

/// Sticker places: (cubie position, outward normal).
pub static STICKERS: LazyLock<Vec<(V3, V3)>> = LazyLock::new(|| {
    let mut out = vec![];
    for axis in 0..3 {
        for sign in [1, -1] {
            for a in -1..=1 {
                for b in -1..=1 {
                    let mut pos = [0; 3];
                    let mut normal = [0; 3];
                    pos[axis] = sign;
                    normal[axis] = sign;
                    pos[(axis + 1) % 3] = a;
                    pos[(axis + 2) % 3] = b;
                    out.push((pos, normal));
                }
            }
        }
    }
    out
});

/// `p[i]` is the place that the contents of place `i` came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Perm(pub [u8; 54]);

impl Perm {
    pub const ID: Self = {
        let mut p = [0; 54];
        let mut i = 0;
        while i < 54 {
            p[i] = i as u8;
            i += 1;
        }
        Self(p)
    };

    /// `self`, then `next`.
    pub fn then(&self, next: &Self) -> Self {
        Self(next.0.map(|i| self.0[i as usize]))
    }

    pub fn inv(&self) -> Self {
        let mut out = [0; 54];
        for (i, &src) in self.0.iter().enumerate() {
            out[src as usize] = i as u8;
        }
        Self(out)
    }

    pub fn pow(&self, n: i32) -> Self {
        let mut base = if n < 0 { self.inv() } else { *self };
        let mut n = n.unsigned_abs();
        let mut out = Self::ID;
        while n > 0 {
            if n & 1 == 1 {
                out = out.then(&base);
            }
            base = base.then(&base);
            n >>= 1;
        }
        out
    }

    /// Solved up to whole-cube rotation.
    pub fn is_solved(&self) -> bool {
        self.0.chunks(9).all(|f| f.iter().all(|&s| s / 9 == f[0] / 9))
    }
}

pub fn face_normal(face: char) -> Option<V3> {
    Some(match face {
        'R' => [1, 0, 0],
        'L' => [-1, 0, 0],
        'U' => [0, 1, 0],
        'D' => [0, -1, 0],
        'F' => [0, 0, 1],
        'B' => [0, 0, -1],
        _ => return None,
    })
}

pub fn face_of_normal(n: V3) -> char {
    "RLUDFB".chars().find(|&c| face_normal(c) == Some(n)).expect("unit normal")
}

fn dot(a: V3, b: V3) -> i32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Rotates 90 degrees clockwise as seen looking at the face with normal `n`.
fn rotate_cw(n: V3, v: V3) -> V3 {
    let d = dot(n, v);
    let cross = |i: usize| {
        let (j, k) = ((i + 1) % 3, (i + 2) % 3);
        n[j] * v[k] - n[k] * v[j]
    };
    [0, 1, 2].map(|i| n[i] * d - cross(i))
}

/// Clockwise turn of a face's layers `lo..=hi` (1 is the outermost).
pub fn turn(face: char, lo: u32, hi: u32) -> Option<Perm> {
    if lo < 1 || hi > 3 || lo > hi {
        return None;
    }
    let n = face_normal(face)?;
    let mut out = Perm::ID;
    for (i, &(pos, normal)) in STICKERS.iter().enumerate() {
        let layer = (2 - dot(n, pos)) as u32;
        if (lo..=hi).contains(&layer) {
            let dest = (rotate_cw(n, pos), rotate_cw(n, normal));
            let j = STICKERS.iter().position(|s| *s == dest)?;
            out.0[j] = i as u8;
        }
    }
    Some(out)
}
