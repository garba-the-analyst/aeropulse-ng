//! Tactical intercept geometry.
//!
//! Solves the constant-speed, constant-velocity-target intercept problem:
//! given an interceptor at rest or in cruise and a target flying a steady
//! course, computes the required true heading, minimum ground speed and
//! time-to-intercept such that both meet at the same point.
//!
//! The solution iterates on the classical pursuit-curve fixed point:
//! heading = bearing to target's position projected `t` ahead, with `t`
//! refined by closing-speed integration. Convergence is monotonic for all
//! physically realisable inputs.

use crate::kinematics::dead_reckoning::{KT_TO_MS, NM_TO_M};
use crate::models::InterceptSolution;

#[derive(Debug, Clone, Copy)]
pub struct InterceptInput {
    /// Interceptor state (ENU metres / m/s relative to site).
    pub ix_m: f64,
    pub iy_m: f64,
    pub ivx_ms: f64,
    pub ivy_ms: f64,
    /// Target state.
    pub tx_m: f64,
    pub ty_m: f64,
    pub tvx_ms: f64,
    pub tvy_ms: f64,
    /// Maximum sustainable interceptor ground speed, knots TAS.
    pub interceptor_speed_kt: f64,
    /// Abandon search beyond this time, seconds.
    pub max_time_s: f64,
}

impl Default for InterceptInput {
    fn default() -> Self {
        Self {
            ix_m: 0.0,
            iy_m: 0.0,
            ivx_ms: 0.0,
            ivy_ms: 0.0,
            tx_m: 50_000.0,
            ty_m: 0.0,
            tvx_ms: -150.0,
            tvy_ms: 0.0,
            interceptor_speed_kt: 480.0,
            max_time_s: 900.0,
        }
    }
}

/// Iterates the pursuit fixed point until the residual range is under
/// `tolerance_m` or iterations exhaust.
pub fn solve_intercept(input: &InterceptInput) -> InterceptSolution {
    let v_i = input.interceptor_speed_kt * KT_TO_MS;
    let rel_x = input.tx_m - input.ix_m;
    let rel_y = input.ty_m - input.iy_m;
    let initial_range_nm = (rel_x * rel_x + rel_y * rel_y).sqrt() / NM_TO_M;

    // Initial guess: direct bearing, naive closing estimate.
    let mut t = initial_range_nm * NM_TO_M / (v_i.max(1.0));
    let mut heading_rad = rel_y.atan2(rel_x);

    const MAX_ITERS: usize = 48;
    const TOLERANCE_M: f64 = 15.0;

    let mut feasible = false;
    let mut t_final = f64::INFINITY;

    for _ in 0..MAX_ITERS {
        // Relative geometry at candidate time t (interceptor drift included).
        let px = input.tx_m + input.tvx_ms * t - input.ivx_ms * t - input.ix_m;
        let py = input.ty_m + input.tvy_ms * t - input.ivy_ms * t - input.iy_m;

        // Bearing measured FROM NORTH, matching engine convention:
        // heading unit vector = (sin h, cos h) on (east, north).
        heading_rad = px.atan2(py);
        let required = (px * px + py * py).sqrt();

        if v_i <= 1.0 {
            break;
        }
        let t_new = required / v_i;

        if (t_new - t).abs() < TOLERANCE_M / v_i {
            t = t_new;
            feasible = t <= input.max_time_s;
            t_final = t;
            break;
        }
        // Damped update keeps oscillation bounded on head-on geometries.
        t += 0.65 * (t_new - t);
        if t > input.max_time_s * 4.0 {
            break;
        }
    }

    // Verify by forward simulation of the final solution.
    let hx = heading_rad.sin();
    let hy = heading_rad.cos();
    let meet_ix = input.ix_m + input.ivx_ms * t_final + hx * v_i * t_final;
    let meet_iy = input.iy_m + input.ivy_ms * t_final + hy * v_i * t_final;
    let meet_tx = input.tx_m + input.tvx_ms * t_final;
    let meet_ty = input.ty_m + input.tvy_ms * t_final;
    let residual = ((meet_ix - meet_tx).powi(2) + (meet_iy - meet_ty).powi(2)).sqrt();
    feasible = feasible && residual < 250.0 && t_final.is_finite() && t_final <= input.max_time_s;

    let heading_deg = heading_rad.to_degrees().rem_euclid(360.0);

    InterceptSolution {
        target_icao24: String::new(),
        interceptor_icao24: String::new(),
        intercept_heading_deg: heading_deg,
        required_speed_kt: input.interceptor_speed_kt,
        time_to_intercept_s: if feasible { t_final } else { f64::INFINITY },
        initial_bearing_deg: rel_x.atan2(rel_y).to_degrees().rem_euclid(360.0),
        range_nm: initial_range_nm,
        feasible,
    }
}

/// Minimum speed that still achieves intercept within `max_time_s` using
/// bisection over the feasibility predicate.
pub fn minimum_feasible_speed_kt(input: &InterceptInput) -> Option<f64> {
    let mut lo = 60.0_f64;
    let mut hi = input.interceptor_speed_kt.max(120.0);

    let fast = solve_intercept(input);
    if !fast.feasible {
        return None;
    }

    for _ in 0..22 {
        let mid = 0.5 * (lo + hi);
        let probe = solve_intercept(&InterceptInput {
            interceptor_speed_kt: mid,
            ..*input
        });
        if probe.feasible {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Some(hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_on_intercept_is_straight_line() {
        // Target inbound on the x-axis; interceptor due east of site.
        let sol = solve_intercept(&InterceptInput {
            ix_m: 0.0,
            iy_m: 0.0,
            ivx_ms: 0.0,
            ivy_ms: 0.0,
            tx_m: 100_000.0,
            ty_m: 0.0,
            tvx_ms: -180.0,
            tvy_ms: 0.0,
            interceptor_speed_kt: 500.0,
            max_time_s: 600.0,
        });
        assert!(sol.feasible);
        assert!(
            (sol.intercept_heading_deg - 90.0).abs() < 0.5,
            "heading {}",
            sol.intercept_heading_deg
        );
        // Closing 257+180 m/s over 100 km -> ~228 s.
        assert!((sol.time_to_intercept_s - 227.0).abs() < 12.0);
    }

    #[test]
    fn crossing_target_requires_lead_angle() {
        // Eastbound crossing traffic NE of the interceptor: predicted point
        // rotates clockwise, so the solution heading must lead the
        // instantaneous bearing toward east.
        let sol = solve_intercept(&InterceptInput {
            ix_m: 0.0,
            iy_m: 0.0,
            ivx_ms: 0.0,
            ivy_ms: 0.0,
            tx_m: 45_000.0,
            ty_m: 22_000.0,
            tvx_ms: 80.0,
            tvy_ms: 0.0,
            interceptor_speed_kt: 450.0,
            max_time_s: 600.0,
        });
        assert!(sol.feasible, "crossing geometry must be catchable");
        assert!(
            sol.intercept_heading_deg > sol.initial_bearing_deg + 2.0,
            "lead {} vs initial {}",
            sol.intercept_heading_deg,
            sol.initial_bearing_deg
        );
    }

    #[test]
    fn receding_target_infeasible_when_slower() {
        let sol = solve_intercept(&InterceptInput {
            tx_m: 10_000.0,
            ty_m: 0.0,
            tvx_ms: 300.0,
            interceptor_speed_kt: 200.0,
            max_time_s: 300.0,
            ..Default::default()
        });
        assert!(!sol.feasible);
    }

    #[test]
    fn moving_interceptor_shortens_time_to_contact() {
        let static_base = solve_intercept(&InterceptInput {
            tx_m: 80_000.0,
            ty_m: 20_000.0,
            tvx_ms: -100.0,
            tvy_ms: 0.0,
            interceptor_speed_kt: 400.0,
            max_time_s: 900.0,
            ..Default::default()
        });
        let already_running = solve_intercept(&InterceptInput {
            ix_m: 0.0,
            iy_m: 0.0,
            ivx_ms: 150.0,
            ivy_ms: 40.0,
            tx_m: 80_000.0,
            ty_m: 20_000.0,
            tvx_ms: -100.0,
            tvy_ms: 0.0,
            interceptor_speed_kt: 400.0,
            max_time_s: 900.0,
        });
        assert!(static_base.feasible && already_running.feasible);
        assert!(already_running.time_to_intercept_s < static_base.time_to_intercept_s);
    }

    #[test]
    fn min_speed_bisection_bounded_by_fast_solution() {
        let input = InterceptInput {
            tx_m: 70_000.0,
            ty_m: 25_000.0,
            tvx_ms: -140.0,
            tvy_ms: 30.0,
            interceptor_speed_kt: 520.0,
            max_time_s: 420.0,
            ..Default::default()
        };
        let vmin = minimum_feasible_speed_kt(&input).expect("feasible envelope");
        assert!(vmin <= 520.0);
        let check = solve_intercept(&InterceptInput {
            interceptor_speed_kt: vmin + 5.0,
            ..input
        });
        assert!(check.feasible);
    }
}
