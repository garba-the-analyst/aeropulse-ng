//! Fixed-size linear algebra kernels for the 6-state kinematic filters.
//!
//! Hand-rolled `[f64; N]` operations avoid external matrix crates so that the
//! air-gapped build has zero transitive native dependencies and bit-exact
//! reproducible arithmetic across workstations. All matrices are row-major.

pub const N: usize = 6;

/// Row-major 6x6 matrix.
pub type Mat6 = [f64; 36];

/// Identity matrix.
#[inline]
pub fn identity() -> Mat6 {
    let mut m = [0.0; 36];
    for i in 0..N {
        m[i * N + i] = 1.0;
    }
    m
}

#[inline]
pub fn mul(a: &Mat6, b: &Mat6) -> Mat6 {
    let mut o = [0.0; 36];
    for r in 0..N {
        for c in 0..N {
            let mut acc = 0.0;
            for k in 0..N {
                acc += a[r * N + k] * b[k * N + c];
            }
            o[r * N + c] = acc;
        }
    }
    o
}

/// Computes `a * b^T`.
#[inline]
pub fn mul_bt(a: &Mat6, b: &Mat6) -> Mat6 {
    let mut o = [0.0; 36];
    for r in 0..N {
        for c in 0..N {
            let mut acc = 0.0;
            for k in 0..N {
                acc += a[r * N + k] * b[c * N + k];
            }
            o[r * N + c] = acc;
        }
    }
    o
}

/// In-place addition.
#[inline]
pub fn add_assign(a: &mut Mat6, b: &Mat6) {
    for i in 0..36 {
        a[i] += b[i];
    }
}

/// Matrix-vector product.
#[inline]
pub fn mul_vec(a: &Mat6, v: &[f64; N]) -> [f64; N] {
    let mut o = [0.0; N];
    for r in 0..N {
        let base = r * N;
        o[r] = a[base] * v[0]
            + a[base + 1] * v[1]
            + a[base + 2] * v[2]
            + a[base + 3] * v[3]
            + a[base + 4] * v[4]
            + a[base + 5] * v[5];
    }
    o
}

/// Extracts a symmetric submatrix block `[r0..r0+d, c0..c0+d]` as dense rows.
pub fn block(a: &Mat6, r0: usize, c0: usize, d: usize) -> Vec<f64> {
    let mut o = vec![0.0; d * d];
    for r in 0..d {
        for c in 0..d {
            o[r * d + c] = a[(r0 + r) * N + (c0 + c)];
        }
    }
    o
}

/// Cholesky factorisation `L` such that `A = L L^T` for SPD `A`.
/// Returns `None` if a non-positive pivot is encountered.
pub fn cholesky(a: &[f64], d: usize) -> Option<Vec<f64>> {
    let mut l = vec![0.0; d * d];
    for i in 0..d {
        for j in 0..=i {
            let mut sum = a[i * d + j];
            for k in 0..j {
                sum -= l[i * d + k] * l[j * d + k];
            }
            if i == j {
                if sum <= 1e-12 {
                    return None;
                }
                l[i * d + i] = sum.sqrt();
            } else {
                l[i * d + j] = sum / l[j * d + j];
            }
        }
    }
    Some(l)
}

/// Solves `A x = b` for SPD `A` via Cholesky. `b` holds `d` right-hand-side
/// columns laid out column-major (`d * n_rhs`). Returns `x` in same layout.
pub fn chol_solve(a: &[f64], b: &[f64], d: usize, n_rhs: usize) -> Option<Vec<f64>> {
    let l = cholesky(a, d)?;
    let mut y = vec![0.0; d * n_rhs];
    // Forward substitution L y = b.
    for col in 0..n_rhs {
        for i in 0..d {
            let mut sum = b[col * d + i];
            for k in 0..i {
                sum -= l[i * d + k] * y[col * d + k];
            }
            y[col * d + i] = sum / l[i * d + i];
        }
    }
    let mut x = vec![0.0; d * n_rhs];
    // Back substitution L^T x = y.
    for col in 0..n_rhs {
        for i in (0..d).rev() {
            let mut sum = y[col * d + i];
            for k in (i + 1)..d {
                sum -= l[k * d + i] * x[col * d + k];
            }
            x[col * d + i] = sum / l[i * d + i];
        }
    }
    Some(x)
}

/// Numerically enforces covariance symmetry.
pub fn symmetrize(m: &mut Mat6) {
    for r in 0..N {
        for c in (r + 1)..N {
            let avg = 0.5 * (m[r * N + c] + m[c * N + r]);
            m[r * N + c] = avg;
            m[c * N + r] = avg;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_roundtrip() {
        let m: Mat6 = (0..36).map(|i| i as f64).collect::<Vec<_>>().try_into().unwrap();
        let o = mul(&identity(), &m);
        assert!((o[7] - m[7]).abs() < 1e-12);
    }

    #[test]
    fn cholesky_solves_spd_system() {
        let a = [4.0, 2.0, -2.0, 2.0, 10.0, 2.0, -2.0, 2.0, 5.0];
        let b = [2.0, -2.0, 4.0];
        let x = chol_solve(&a, &b, 3, 1).expect("spd");
        // Verified by Gaussian elimination: z=2, y=-1, x=2.
        let expect = [2.0, -1.0, 2.0];
        for i in 0..3 {
            assert!((x[i] - expect[i]).abs() < 1e-9);
        }
    }

    #[test]
    fn singular_matrix_rejected() {
        let a = [1.0, 1.0, 1.0, 1.0];
        assert!(cholesky(&a, 2).is_none());
    }
}
