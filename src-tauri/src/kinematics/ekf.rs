//! 6-State Extended Kalman Filter.
//!
//! State vector `X = [x, y, z, vx, vy, vz]^T` expressed in metres and
//! metres-per-second on the local East-North-Up tangent plane anchored at the
//! surveillance site. The filter smooths transponder position jitter and
//! provides the kinematic state used for dead-reckoning projection during RF
//! shadow zones.

use super::linalg::{self, Mat6};

/// Process noise: assumed 1-sigma target acceleration per axis, m/s^2.
/// Tuned between commercial transport (0.6 g lateral) and tactical jets.
pub const ACCEL_SIGMA_MS2: f64 = 3.5;

/// Default ADS-B horizontal measurement noise, 1-sigma in metres.
pub const POS_SIGMA_M: f64 = 45.0;
/// Barometric altitude noise, 1-sigma in feet (converted internally).
pub const ALT_SIGMA_FT: f64 = 55.0;
/// ADS-B velocity vector noise, 1-sigma in m/s.
pub const VEL_SIGMA_MS: f64 = 3.0;

const FT_TO_M: f64 = 0.3048;

#[derive(Debug, Clone)]
pub struct EkfConfig {
    pub accel_sigma: f64,
    pub pos_sigma_m: f64,
    pub alt_sigma_m: f64,
    pub vel_sigma_ms: f64,
    /// Mahalanobis gate squared for outlier rejection (chi2, dof=3 -> 16.27).
    pub pos_gate_chi2: f64,
}

impl Default for EkfConfig {
    fn default() -> Self {
        Self {
            accel_sigma: ACCEL_SIGMA_MS2,
            pos_sigma_m: POS_SIGMA_M,
            alt_sigma_m: ALT_SIGMA_FT * FT_TO_M,
            vel_sigma_ms: VEL_SIGMA_MS,
            pos_gate_chi2: 16.27,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Ekf {
    x: [f64; linalg::N],
    p: Mat6,
    cfg: EkfConfig,
    initialised: bool,
    last_update_s: f64,
    /// Accepted position fixes; gates engage only after acquisition.
    accepted_fixes: u32,
}

/// Number of accepted fixes before Mahalanobis gating activates. During
/// acquisition the velocity state is unknown and constant-velocity
/// prediction errors legitimately exceed steady-state bounds.
const ACQUISITION_FIXES: u32 = 12;

impl Ekf {
    pub fn new(cfg: EkfConfig) -> Self {
        let mut p = linalg::identity();
        // Generous initial uncertainty until first fixes converge.
        for i in 0..3 {
            p[i * 6 + i] = 250_000.0;
            p[(i + 3) * 6 + (i + 3)] = 900.0;
        }
        Self {
            x: [0.0; 6],
            p,
            cfg,
            initialised: false,
            last_update_s: 0.0,
            accepted_fixes: 0,
        }
    }

    #[inline]
    pub fn is_initialised(&self) -> bool {
        self.initialised
    }

    #[inline]
    pub fn last_update_s(&self) -> f64 {
        self.last_update_s
    }

    /// Read-only view of the state vector `[x, y, z, vx, vy, vz]`.
    #[inline]
    pub fn state(&self) -> &[f64; linalg::N] {
        &self.x
    }

    /// Controlled velocity injection (simulator seeding / mid-air handover).
    #[inline]
    pub fn set_velocity(&mut self, vx: f64, vy: f64, vz: f64) {
        self.x[3] = vx;
        self.x[4] = vy;
        self.x[5] = vz;
    }

    /// Horizontal 1-sigma position uncertainty from the covariance trace.
    #[inline]
    pub fn horizontal_sigma_m(&self) -> f64 {
        ((self.p[0] + self.p[7]) * 0.5).sqrt()
    }

    /// Time propagation step `x = F x`, `P = F P F^T + Q` for elapsed `dt_s`.
    pub fn predict(&mut self, dt_s: f64) {
        if dt_s <= 0.0 {
            return;
        }
        let dt = dt_s;
        self.x[0] += self.x[3] * dt;
        self.x[1] += self.x[4] * dt;
        self.x[2] += self.x[5] * dt;

        let mut f = linalg::identity();
        f[3] = dt; // x += vx*dt
        f[1 * 6 + 4] = dt; // y += vy*dt
        f[2 * 6 + 5] = dt; // z += vz*dt

        let q = process_noise(self.cfg.accel_sigma, dt);
        let fp = linalg::mul(&f, &self.p);
        self.p = linalg::mul_bt(&fp, &f);
        linalg::add_assign(&mut self.p, &q);
        linalg::symmetrize(&mut self.p);
    }

    /// Seeds the filter from a first position fix, zeroing velocities with
    /// speed uncertainty spanning the full ATC envelope (up to ~300 m/s).
    pub fn seed_position(&mut self, x_m: f64, y_m: f64, z_ft: f64, now_s: f64) {
        self.x = [x_m, y_m, z_ft * FT_TO_M, 0.0, 0.0, 0.0];
        for i in 0..3 {
            self.p[i * 6 + i] = (self.cfg.pos_sigma_m * 2.0).powi(2);
            self.p[(i + 3) * 6 + (i + 3)] = 150.0_f64.powi(2);
        }
        self.initialised = true;
        self.accepted_fixes = 0;
        self.last_update_s = now_s;
    }

    /// Position measurement update with Mahalanobis gating.
    /// Predicts internally to `now_s` first — for engine-driven loops that
    /// already propagate per tick, prefer [`Ekf::fuse_position`].
    pub fn update_position(
        &mut self,
        x_m: f64,
        y_m: f64,
        z_ft: f64,
        now_s: f64,
    ) -> bool {
        if !self.initialised {
            self.seed_position(x_m, y_m, z_ft, now_s);
            return true;
        }
        let dt = now_s - self.last_update_s;
        if dt > 0.0 {
            self.predict(dt);
        }
        self.fuse_position(x_m, y_m, z_ft, now_s)
    }

    /// Velocity update variant matching [`Ekf::update_position`].
    pub fn update_velocity(
        &mut self,
        ground_speed_kt: f64,
        course_deg: f64,
        vertical_rate_fpm: f64,
        now_s: f64,
    ) {
        if !self.initialised {
            return;
        }
        let dt = now_s - self.last_update_s;
        if dt > 0.0 {
            self.predict(dt);
        }
        self.fuse_velocity(ground_speed_kt, course_deg, vertical_rate_fpm, now_s);
    }
    /// Position fusion assuming the caller has already propagated the state
    /// to the measurement epoch (engine-driven loops predict once per tick).
    /// Returns `false` when the innovation was rejected by the gate.
    pub fn fuse_position(&mut self, x_m: f64, y_m: f64, z_ft: f64, now_s: f64) -> bool {
        if !self.initialised {
            self.seed_position(x_m, y_m, z_ft, now_s);
            return true;
        }
        self.last_update_s = now_s;

        let z = [x_m, y_m, z_ft * FT_TO_M];
        let hx = [self.x[0], self.x[1], self.x[2]];

        let ppp = linalg::block(&self.p, 0, 0, 3);
        let mut s = ppp.clone();
        s[0] += self.cfg.pos_sigma_m.powi(2);
        s[4] += self.cfg.pos_sigma_m.powi(2);
        s[8] += self.cfg.alt_sigma_m.powi(2);

        let innov = [z[0] - hx[0], z[1] - hx[1], z[2] - hx[2]];
        let d = chol_innov_distance(&s, &innov, 3);
        if self.accepted_fixes >= ACQUISITION_FIXES && d > self.cfg.pos_gate_chi2 {
            return false;
        }
        self.apply_gain(&ppp, &s, &innov, 0);
        self.accepted_fixes += 1;
        true
    }

    /// Velocity fusion with the same caller-predicts contract.
    pub fn fuse_velocity(
        &mut self,
        ground_speed_kt: f64,
        course_deg: f64,
        vertical_rate_fpm: f64,
        now_s: f64,
    ) {
        if !self.initialised {
            return;
        }
        self.last_update_s = now_s;

        let speed_ms = ground_speed_kt * 0.514444;
        let rad = course_deg.to_radians();
        let vx = speed_ms * rad.sin();
        let vy = speed_ms * rad.cos();
        let vz = vertical_rate_fpm * FT_TO_M / 60.0;

        let pvv = linalg::block(&self.p, 3, 3, 3);
        let mut s = pvv.clone();
        s[0] += self.cfg.vel_sigma_ms.powi(2);
        s[4] += self.cfg.vel_sigma_ms.powi(2);
        s[8] += (self.cfg.vel_sigma_ms * 4.0).powi(2);

        let innov = [vx - self.x[3], vy - self.x[4], vz - self.x[5]];
        let d = chol_innov_distance(&s, &innov, 3);
        if self.accepted_fixes >= ACQUISITION_FIXES && d > self.cfg.pos_gate_chi2 * 4.0 {
            return;
        }
        self.apply_gain_offset(&pvv, &s, &innov, 3);
    }

    /// Joseph-form covariance update `P = (I-KH) P (I-KH)^T + K R K^T` for a
    /// 3-dimensional measurement whose H rows select state indices starting
    /// at `state_offset`. `p_sel` must be `P[state_offset..+3, state_offset..+3]`.
    fn apply_gain_offset(&mut self, p_sel: &[f64], s: &[f64], innov: &[f64; 3], off: usize) {
        let k = kalman_gain(p_sel, s); // 3x3
        for j in 0..3 {
            self.x[off + j] +=
                k[j * 3] * innov[0] + k[j * 3 + 1] * innov[1] + k[j * 3 + 2] * innov[2];
        }

        // H is a pure selector (H = [0 I 0] band), so KH lives entirely in
        // the diagonal band [off..off+3) and equals K there: A = I - KH.
        let mut a = linalg::identity();
        for r in 0..3 {
            for c in 0..3 {
                a[(off + r) * 6 + (off + c)] -= k[r * 3 + c];
            }
        }
        let ap = linalg::mul(&a, &self.p);
        let mut new_p = linalg::mul_bt(&ap, &a);

        let r_diag = [
            self.meas_noise_for(off),
            self.meas_noise_for(off + 1),
            self.meas_noise_for(off + 2),
        ];
        for r in 0..3 {
            for c in 0..3 {
                let krk = k[r * 3] * k[c * 3] * r_diag[0]
                    + k[r * 3 + 1] * k[c * 3 + 1] * r_diag[1]
                    + k[r * 3 + 2] * k[c * 3 + 2] * r_diag[2];
                new_p[(off + r) * 6 + (off + c)] += krk;
            }
        }
        self.p = new_p;
        linalg::symmetrize(&mut self.p);
    }

    fn apply_gain(&mut self, p_sel: &[f64], s: &[f64], innov: &[f64; 3], _sel: usize) {
        self.apply_gain_offset(p_sel, s, innov, 0);
    }

    fn meas_noise_for(&self, idx: usize) -> f64 {
        match idx {
            0 | 1 => self.cfg.pos_sigma_m.powi(2),
            2 => self.cfg.alt_sigma_m.powi(2),
            3 | 4 => self.cfg.vel_sigma_ms.powi(2),
            5 => (self.cfg.vel_sigma_ms * 4.0).powi(2),
            _ => 1.0,
        }
    }
}

/// Standard Kalman gain `K = P S^-1` for symmetric 3x3 blocks.
fn kalman_gain(p_sel: &[f64], s: &[f64]) -> [f64; 9] {
    let rhs_colmajor = [
        p_sel[0], p_sel[3], p_sel[6], p_sel[1], p_sel[4], p_sel[7], p_sel[2], p_sel[5], p_sel[8],
    ];
    let sol = linalg::chol_solve(s, &rhs_colmajor, 3, 3).expect("innovation covariance SPD");
    let mut k = [0.0; 9];
    for r in 0..3 {
        for c in 0..3 {
            k[r * 3 + c] = sol[c * 3 + r];
        }
    }
    k
}

/// Squared Mahalanobis distance of innovation under Cholesky-factored `S`.
fn chol_innov_distance(s: &[f64], innov: &[f64; 3], d: usize) -> f64 {
    match linalg::chol_solve(s, &innov[..d], d, 1) {
        Some(y) => innov.iter().zip(y.iter()).map(|(v, w)| v * w).sum(),
        None => f64::INFINITY,
    }
}

/// Piecewise-constant white-noise-acceleration process model, one 2x2 block
/// per axis embedded into the full 6x6 Q matrix.
fn process_noise(sigma_a: f64, dt: f64) -> Mat6 {
    let s2 = sigma_a * sigma_a;
    let q_pos = s2 * dt.powi(4) / 4.0;
    let q_cross = s2 * dt.powi(3) / 2.0;
    let q_vel = s2 * dt * dt;
    let mut q = [0.0; 36];
    for axis in 0..3 {
        let pr = axis;
        let vr = axis + 3;
        q[pr * 6 + pr] = q_pos;
        q[pr * 6 + vr] = q_cross;
        q[vr * 6 + pr] = q_cross;
        q[vr * 6 + vr] = q_vel;
    }
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f64 = 25.0;

    #[test]
    fn converges_on_noisy_constant_velocity_target() {
        // Transport-grade process noise keeps the filter tight on a
        // constant-velocity airliner track.
        let cfg = EkfConfig {
            accel_sigma: 1.5,
            ..EkfConfig::default()
        };
        let mut ekf = Ekf::new(cfg);
        let truth_v = 220.0; // m/s eastward
        let mut t = 0.0;
        let mut x_true = 0.0;
        let mut seeded = false;
        let mut rng_state = 12345u64;
        let mut noise = || {
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (((rng_state >> 33) % 2001) as f64 - 1000.0) / 1000.0 * POS_SIGMA_M
        };
        for _ in 0..200 {
            t += 1.0;
            x_true += truth_v;
            let zx = x_true + noise();
            let zy = noise() * 0.3;
            if !seeded {
                ekf.seed_position(zx, zy, 30_000.0, t);
                seeded = true;
            } else {
                ekf.update_position(zx, zy, 30_000.0, t);
                ekf.update_velocity(truth_v / 0.514444, 90.0, 0.0, t);
            }
        }
        assert!(
            (ekf.x[3] - truth_v).abs() < 15.0,
            "vx est {} vs {}",
            ekf.x[3],
            truth_v
        );
        assert!(
            (ekf.x[0] - x_true).abs() < 250.0,
            "pos err {}",
            (ekf.x[0] - x_true).abs()
        );
    }

    #[test]
    fn predict_advances_position() {
        let mut ekf = Ekf::new(EkfConfig::default());
        ekf.seed_position(0.0, 0.0, 10_000.0, 0.0);
        ekf.x[3] = 100.0;
        ekf.predict(10.0);
        assert!((ekf.x[0] - 1000.0).abs() < TOL);
    }

    #[test]
    fn gate_rejects_wild_outlier() {
        let mut ekf = Ekf::new(EkfConfig::default());
        ekf.seed_position(0.0, 0.0, 20_000.0, 0.0);
        for i in 1..40 {
            ekf.update_position(i as f64 * 200.0, 0.0, 20_000.0, i as f64);
        }
        let accepted = ekf.update_position(500_000.0, -500_000.0, 20_000.0, 41.0);
        assert!(!accepted, "gross outlier must be gated out");
    }
}
