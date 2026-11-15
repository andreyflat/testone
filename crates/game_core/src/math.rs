//! Small sums the crates share.

/// Eases from 0 to 1 as `t` goes from 0 to 1, slowly at either end; 0 before and 1 after.
pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
