//! Rotation helpers for posing a body: two-bone inverse kinematics for arms and legs, and
//! conversion between rotations and the per-axis angles Rapier's joint motors work in.

use bevy::prelude::*;

/// A rotation's angles about its own X, Y and Z axes the way Rapier measures them for a joint's
/// motors and limits: twice the arcsine of each of its vector components, taking the shorter
/// way round.
pub fn axis_angles(rotation: Quat) -> Vec3 {
    let rotation = if rotation.w < 0.0 { -rotation } else { rotation };
    let angle = |component: f32| 2.0 * component.clamp(-1.0, 1.0).asin();
    Vec3::new(angle(rotation.x), angle(rotation.y), angle(rotation.z))
}

/// The rotation with the given `axis_angles`, as near as there is one: angles too large
/// together are scaled down until there is.
pub fn from_axis_angles(angles: Vec3) -> Quat {
    let imaginary = Vec3::new((angles.x / 2.0).sin(), (angles.y / 2.0).sin(), (angles.z / 2.0).sin());
    let length = imaginary.length();
    let imaginary = if length > 1.0 { imaginary / length } else { imaginary };
    Quat::from_xyzw(imaginary.x, imaginary.y, imaginary.z, (1.0 - imaginary.length_squared()).max(0.0).sqrt())
}

/// How far a rotation turns about the vertical: its twist about Y, however it is tilted.
pub fn yaw(rotation: Quat) -> f32 {
    2.0 * rotation.y.atan2(rotation.w)
}

/// Turning about the vertical by `yaw`.
pub fn yaw_rotation(yaw: f32) -> Quat {
    Quat::from_rotation_y(yaw)
}

/// Two-bone IK for a limb that hangs along -Y at rest, with its middle joint a hinge about the
/// bones' shared X axis. Returns the world rotations of the upper and lower bones that put the
/// limb's end at `end` from its root at `root`, with the middle joint pushed out towards `pole`.
/// A leg's knee pushes out ahead of it and bends the shin back; an arm's elbow, with `bends_back`
/// false, pushes out behind and bends the forearm forwards. Out of reach, the limb points
/// straight at the end; too close, it folds as far as makes sense.
pub fn two_bone(root: Vec3, end: Vec3, upper: f32, lower: f32, pole: Vec3, bends_back: bool) -> (Quat, Quat) {
    let offset = end - root;
    let reach = offset.length().clamp((upper - lower).abs() + 0.05, upper + lower - 1e-3);
    let towards = offset.normalize_or(Vec3::NEG_Y);
    // Towards the pole, square to the line from root to end; anything square if it's on it.
    let out = (pole - towards * pole.dot(towards)).try_normalize().unwrap_or_else(|| towards.any_orthonormal_vector());
    let along = (upper * upper - lower * lower + reach * reach) / (2.0 * reach);
    let middle = towards * along + out * (upper * upper - along * along).max(0.0).sqrt();
    let end = towards * reach;
    // The hinge axis: square to the plane the limb bends in, the same for both bones, and +X
    // for a leg standing straight with its knee pushing forwards (+Z).
    let axis = out.cross(towards).normalize_or(Vec3::X) * if bends_back { 1.0 } else { -1.0 };
    let bone = |along: Vec3| {
        let up = -along.normalize_or(Vec3::NEG_Y);
        let axis = (axis - up * axis.dot(up)).normalize_or(Vec3::X);
        Quat::from_mat3(&Mat3::from_cols(axis, up, axis.cross(up))).normalize()
    };
    (bone(middle), bone(end - middle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        a.distance(b) < 1e-3
    }

    #[test]
    fn axis_angles_round_trip() {
        let angles = Vec3::new(0.3, -0.2, 0.5);
        assert!(close(axis_angles(from_axis_angles(angles)), angles));
    }

    #[test]
    fn straight_leg_is_unturned() {
        let (thigh, shin) = two_bone(Vec3::ZERO, Vec3::NEG_Y * 0.82, 0.45, 0.37, Vec3::Z, true);
        assert!(thigh.angle_between(Quat::IDENTITY) < 0.1);
        assert!(shin.angle_between(Quat::IDENTITY) < 0.1);
    }

    #[test]
    fn bent_leg_reaches_and_bends_knee_back() {
        let end = Vec3::new(0.05, -0.7, 0.1);
        let (thigh, shin) = two_bone(Vec3::ZERO, end, 0.45, 0.37, Vec3::Z, true);
        let knee = thigh * Vec3::NEG_Y * 0.45;
        assert!(close(knee + shin * Vec3::NEG_Y * 0.37, end));
        assert!(knee.z > 0.0, "the knee pushes forwards");
        // The knee only bends about the thigh's X axis, backwards.
        let bend = thigh.inverse() * shin;
        let angles = axis_angles(bend);
        assert!(angles.x > 0.0 && angles.y.abs() < 1e-3 && angles.z.abs() < 1e-3);
    }

    #[test]
    fn arm_bends_forearm_forwards() {
        let end = Vec3::new(0.0, -0.3, 0.3);
        let (upper, lower) = two_bone(Vec3::ZERO, end, 0.28, 0.3, Vec3::NEG_Z, false);
        let elbow = upper * Vec3::NEG_Y * 0.28;
        assert!(close(elbow + lower * Vec3::NEG_Y * 0.3, end));
        let angles = axis_angles(upper.inverse() * lower);
        assert!(angles.x < 0.0 && angles.y.abs() < 1e-3 && angles.z.abs() < 1e-3);
        // Not twisted round: the upper arm's front still faces forwards.
        assert!((upper * Vec3::Z).z > 0.5);
    }
}
