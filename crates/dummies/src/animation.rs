//! Animated dummies: while nothing calls for their balance, a dummy's body parts are kinematic,
//! posed every physics step by an animation clip: standing idly, breathing and shifting a
//! little, or for the walker, a walk cycle round its circle. Light hits make it flinch on top of
//! that, each joint from the part hit up to the pelvis knocked about and springing back.
//!
//! Once it is knocked about too hard, starts bleeding out or dies, the physics takes over: its
//! parts turn dynamic, carrying on at the speed the clip had them moving, and its self-balancing
//! biped (see `biped`) catches it from there. Once it has settled again, it blends back into the
//! clip from wherever the physics left it, standing where it ended up.
//!
//! A standing dummy can also be steered (see `Steer`): it turns to face the way it is told and
//! walks or runs at the speed it is told, easing from its idle stance into the walk cycle, and
//! from there into a run, as it speeds up. It can walk one way while facing another. Armed, it
//! holds a gun in both hands, low in front of it or raised to its shoulder aiming (`gun_pose`
//! says where).

use std::f32::consts::PI;

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use biped::body::{FOREARMS, FOREARM_LENGTH, JointKind, PART_COUNT, Part, UPPER_ARMS, UPPER_ARM_LENGTH, standing_pose};
use biped::ik::{axis_angles, two_bone, yaw};
use game_core::math::smoothstep;

/// Walking speed in metres per second, and the distance covered by one full step cycle, left
/// foot and right.
pub const WALK_SPEED: f32 = 1.2;
const STRIDE_LENGTH: f32 = 1.4;
/// Steered, how fast it turns, in radians per second, and speeds up or slows down, in metres per
/// second squared. It only walks the way it faces: this far off the way it wants to face, in
/// radians, it stops and turns on the spot first.
const TURN_RATE: f32 = 7.0;
const WALK_ACCELERATION: f32 = 9.0;
const WALK_TURN_FIRST: f32 = 1.0;
/// Steered, it keeps to the floor it is told of, closing this fraction of the way to its height
/// each second.
const FLOOR_FOLLOW: f32 = 12.0;
/// Running at this speed, in metres per second, it takes its longest strides, this long, swinging
/// its legs and arms this much further, as fractions, and leans forwards this far, in radians.
pub const RUN_SPEED: f32 = 4.0;
const RUN_STRIDE_LENGTH: f32 = 2.3;
const RUN_SWING: f32 = 0.6;
const RUN_LEAN: f32 = 0.14;
/// Armed, it holds the gun's grip this far in front of its chest, and this far below its
/// shoulders, when not aiming, tipped down this far, in radians...
const LOW_READY_GRIP: Vec3 = Vec3::new(0.06, -0.32, 0.32);
const LOW_READY_PITCH: f32 = -0.8;
/// ...and aiming, this far in front of its shoulders, a little off to the right; raising it
/// or lowering it at this rate, in fractions per second.
const AIM_GRIP: Vec3 = Vec3::new(0.05, -0.04, 0.44);
const AIM_RATE: f32 = 6.0;
/// Leaning, it tips over at this rate, in radians per second: its pelvis from the hips, up to
/// this far, its chest from the waist for the rest, up to this far, so neither strains a joint
/// that the physics would have to snap back if it took over. Its hips and waist are this high.
const LEAN_RATE: f32 = 3.0;
const LEAN_PELVIS: f32 = 0.15;
const LEAN_CHEST: f32 = 0.33;
const HIP_HEIGHT: f32 = 0.9;
const WAIST_HEIGHT: f32 = 1.2;
/// Signalling by hand, it lets go of the gun with its off hand and points out ahead of it with it,
/// this far from its shoulder, in its chest's space, chopping up and down this far, in metres,
/// this fast, in radians per second, for this long, in seconds.
const SIGNAL_POINT: Vec3 = Vec3::new(0.06, 0.14, 0.48);
const SIGNAL_CHOP: (f32, f32) = (0.05, 13.0);
const SIGNAL_TIME: f32 = 1.3;
/// Where the off hand holds the gun, relative to the grip, in the gun's space.
const SUPPORT_HAND: Vec3 = Vec3::new(-0.03, -0.01, -0.06);
/// The gun's grip, in its own space, which it is held by.
pub const GUN_GRIP: Vec3 = Vec3::new(0.0, -0.07, 0.07);
/// How far the legs swing forwards and back from the hip, and the most the knee bends while the
/// leg swings forwards, in radians.
const LEG_SWING: f32 = 0.45;
const KNEE_BEND: f32 = 0.7;
/// How far the arms swing from the shoulder, and how bent the elbows are while walking, in
/// radians.
const ARM_SWING: f32 = 0.35;
const ELBOW_BEND: f32 = 0.3;
/// Hip to sole, which the body drops by as the legs spread so the feet stay on the ground.
const LEG_LENGTH: f32 = 0.9;
/// Standing idly, it breathes, its chest rising and falling this far, in radians, this fast, in
/// radians per second, and its arms swaying with it, this far...
const BREATH: f32 = 0.02;
const BREATH_RATE: f32 = 1.6;
const ARM_SWAY: f32 = 0.03;
/// ...and slowly shifts its weight, swaying from the ankles this far, in radians, this fast.
const IDLE_SWAY: f32 = 0.012;
const IDLE_SWAY_RATE: f32 = 0.5;

/// Hits make an animated dummy flinch: the joint holding the part hit is knocked about, and the
/// joints above it in turn less and less, each on a spring that brings it back. The springs, as
/// (stiffness per radian off pose, damping per radian per second) for an unhurt part: damped to
/// just short of critically, so it swings back without bouncing...
const FLINCH_SPRING: (f32, f32) = (250.0, 26.0);
/// ...weakening as the part is hurt, down to this fraction of their strength, so a battered limb
/// swings further and returns sluggishly.
const MIN_FLINCH_CONTROL: f32 = 0.2;
/// How fast a hit at full impact sets the joint holding the part hit turning, in radians per
/// second per metre of leverage the shot has about it.
const FLINCH_KICK: f32 = 18.0;
/// Each joint further up the body from the part hit is kicked this fraction as hard as the one
/// below it.
const FLINCH_FALLOFF: f32 = 0.4;
/// Furthest a flinch turns a joint, in radians, and the fraction of how far the joint can turn
/// it keeps within, so the physics never has to snap a joint back into range when it takes over.
const MAX_FLINCH: f32 = 0.9;
const JOINT_LIMIT_MARGIN: f32 = 0.85;

/// Taking over, the physics carries on with this fraction of how fast the clip had each part
/// moving: the hit that set it off stops it short, and a body carrying on at full walking pace
/// would be more than its balance could catch.
const HANDOFF_MOMENTUM: f32 = 0.5;

/// Blending back into the clip from wherever the physics left the body takes this long, in
/// seconds.
const BLEND_TIME: f32 = 0.6;

/// A dummy's animation, and whether it is playing or the physics has the body.
#[derive(Component)]
pub struct Animation {
    /// Playing, its body parts kinematic and posed by the clip.
    pub active: bool,
    /// Walking round a circle of this radius about the dummy's origin, rather than standing.
    walk_radius: Option<f32>,
    /// Where it stands, in the dummy's space, if it doesn't walk.
    root: Transform,
    /// How far round its circle it has walked.
    walked: f32,
    time: f32,
    /// Each part's pose last step, and how fast the clip is moving it, in the dummy's space.
    last: Option<[Transform; PART_COUNT]>,
    velocities: [Velocity; PART_COUNT],
    /// Blending in from where the physics left the body: each part's pose then, and how far
    /// through the blend it is, in seconds.
    blend: Option<([Transform; PART_COUNT], f32)>,
    /// How far each joint has been knocked by flinching, as a rotation vector in the dummy's
    /// space, and how fast it is turning, in `Part` order of the part it holds.
    flinch: [(Vec3, Vec3); PART_COUNT],
    /// Where it is being steered, standing rather than walking its circle.
    pub steer: Steer,
    /// How fast it is walking where it is steered just now, in metres per second.
    pace: f32,
    /// How far through its stride it is, in radians, steered.
    stride: f32,
    /// Which way it is walking just now, as an angle about the vertical, steered.
    heading_walked: f32,
    /// Holding a gun, and how far it has raised it to aim, from 0 to 1, and how far up.
    pub armed: bool,
    aiming: f32,
    aim_pitch: f32,
    /// How far it is leaning, in radians, positive towards its -X side.
    lean: f32,
    /// How much longer it is signalling by hand for, in seconds.
    signalling: f32,
    /// Where it held the gun last step, in the dummy's space.
    gun: Transform,
}

/// Where a standing dummy's animation is told to take it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Steer {
    /// How fast to walk forwards, in metres per second.
    pub speed: f32,
    /// Which way to face, as an angle about the vertical in the dummy's space, if anywhere in
    /// particular.
    pub heading: Option<f32>,
    /// How high the floor under it is, in the dummy's space, if known: it rises and sinks to stay
    /// on it.
    pub floor: Option<f32>,
    /// Which way to walk, as an angle about the vertical in the dummy's space, if not the way it
    /// faces: it walks that way without turning to it.
    pub travel: Option<f32>,
    /// Armed, how far up to aim, in radians, if it is aiming.
    pub aim: Option<f32>,
    /// How far to lean its upper body over to the side from the hips, in radians: positive towards
    /// its -X side.
    pub lean: f32,
}

impl Animation {
    /// Standing at `root`, in the dummy's space.
    pub fn standing(root: Transform) -> Self {
        Self::new(None, root)
    }

    /// Walking a circle of `radius` about the dummy's origin, from its +X side.
    pub fn walking(radius: f32) -> Self {
        Self::new(Some(radius), Transform::IDENTITY)
    }

    fn new(walk_radius: Option<f32>, root: Transform) -> Self {
        Self {
            active: true,
            walk_radius,
            root,
            walked: 0.0,
            time: fastrand::f32() * 100.0,
            last: None,
            velocities: [Velocity::zero(); PART_COUNT],
            blend: None,
            flinch: [(Vec3::ZERO, Vec3::ZERO); PART_COUNT],
            steer: Steer::default(),
            pace: 0.0,
            stride: 0.0,
            heading_walked: 0.0,
            armed: false,
            aiming: 0.0,
            aim_pitch: 0.0,
            lean: 0.0,
            signalling: 0.0,
            gun: Transform::IDENTITY,
        }
    }

    /// Where it stands, in the dummy's space, and which way it faces, as an angle about the
    /// vertical.
    pub fn position(&self) -> Vec3 {
        self.root.translation
    }

    pub fn facing(&self) -> f32 {
        yaw(self.root.rotation)
    }

    /// Moves where it stands by `offset`, in the dummy's space, at once: for whatever keeps it
    /// out of walls and off other dummies, as nothing stops an animated body.
    pub fn nudge(&mut self, offset: Vec3) {
        self.root.translation += offset;
    }

    /// How fast it is walking where it is steered, in metres per second, and how far through its
    /// stride it is, in radians: each step is half a turn.
    pub fn pace(&self) -> f32 {
        self.pace
    }

    pub fn stride(&self) -> f32 {
        self.stride
    }

    /// Signals to others by hand: pointing out ahead with its off hand for a moment.
    pub fn signal(&mut self) {
        self.signalling = SIGNAL_TIME;
    }

    /// How far it has raised its gun to aim, from 0 to 1.
    pub fn aiming(&self) -> f32 {
        self.aiming
    }

    /// Where it holds its gun, in the dummy's space, as the gun's own origin, pointing along its
    /// -Z: as of the last step played.
    pub fn gun_pose(&self) -> Transform {
        self.gun
    }

    /// Where the clip has the given part, with the body standing where the clip is up to.
    fn pose(&self) -> [Transform; PART_COUNT] {
        let mut poses = match self.walk_radius {
            Some(radius) => {
                let angle = self.walked / radius;
                let phase = self.walked / STRIDE_LENGTH * 2.0 * PI;
                // Lower the body as the legs spread, so the planted foot stays on the ground.
                let drop = LEG_LENGTH * (1.0 - walk_angle(Part::LeftThigh, phase).cos());
                // Facing round the circle.
                let frame = Transform::from_xyz(radius * angle.cos(), -drop, radius * angle.sin())
                    .with_rotation(Quat::from_rotation_y(-angle));
                Part::ALL.map(|part| frame * chain_pose(part, |part| walk_angle(part, phase)))
            }
            None => {
                let t = self.time;
                // Steered, it eases from its idle stance into the walk cycle as it speeds up.
                let (walking, swing) = self.gait();
                let phase = self.stride;
                // It walks the way it is going, which may not be the way it faces: its legs swing
                // that way, its body turned from them.
                let off = wrap_angle(self.heading_walked - self.facing());
                let (legs_turn, off) = if off.abs() > PI / 2.0 { (off - PI.copysign(off), -1.0) } else { (off, 1.0) };
                let frame = self.body_frame();
                let idling = 1.0 - walking;
                let mut local = Part::ALL.map(|part| {
                    chain_pose(part, |part| {
                        let walk = walk_angle(part, phase) * if part.is_leg() { off } else { 1.0 };
                        swing * walk + idling * idle_angle(part, t)
                    })
                });
                // Its legs turned the way it walks.
                let hips = Part::Guts.center();
                for part in Part::ALL.into_iter().filter(|part| part.is_leg()) {
                    local[part.index()].rotate_around(hips.with_y(0.0), Quat::from_rotation_y(legs_turn * walking));
                }
                // Leaning over from the hips.
                for part in Part::ALL.into_iter().filter(|part| !part.is_leg()) {
                    self.lean_over(part, &mut local[part.index()]);
                }
                if self.armed {
                    self.hold_gun(&mut local);
                }
                local.map(|local| frame * local)
            }
        };
        self.apply_flinch(&mut poses);
        let floor = if self.walk_radius.is_some() { 0.0 } else { self.root.translation.y };
        lift_clear_of_floor(&mut poses, floor);
        poses
    }

    /// How far it has eased from standing into walking, from 0 to 1, and how far its limbs swing
    /// as it goes, as a fraction of walking.
    fn gait(&self) -> (f32, f32) {
        let walking = (self.pace / WALK_SPEED).clamp(0.0, 1.0);
        (walking, walking * (1.0 + RUN_SWING * self.running()))
    }

    /// Where its body is, steered, in the dummy's space: sinking as its legs spread, leaning into
    /// a run, swaying while it stands.
    fn body_frame(&self) -> Transform {
        let (walking, swing) = self.gait();
        let drop = LEG_LENGTH * (1.0 - (swing * walk_angle(Part::LeftThigh, self.stride)).cos());
        let lean = Quat::from_rotation_x(RUN_LEAN * self.running() * walking);
        let idling = 1.0 - walking;
        let t = self.time;
        let sway = Quat::from_rotation_x(idling * IDLE_SWAY * (t * IDLE_SWAY_RATE).sin())
            * Quat::from_rotation_z(idling * IDLE_SWAY * 0.6 * (t * IDLE_SWAY_RATE * 0.73).sin());
        let pivot = self.root.translation;
        Transform::from_translation(pivot)
            * Transform::from_rotation(sway)
            * Transform::from_translation(-pivot)
            * self.root
            * Transform::from_xyz(0.0, -drop, 0.0).with_rotation(lean)
    }

    /// Tips `part` of its upper body, posed at `pose` in its body's own space, over as far as it
    /// is leaning: its chest and all above it from the waist, then with its pelvis from the hips.
    fn lean_over(&self, part: Part, pose: &mut Transform) {
        if self.lean == 0.0 {
            return;
        }
        let pelvis = self.lean.clamp(-LEAN_PELVIS, LEAN_PELVIS);
        let chest = (self.lean - pelvis).clamp(-LEAN_CHEST, LEAN_CHEST);
        if part != Part::Guts {
            pose.rotate_around(Vec3::Y * WAIST_HEIGHT, Quat::from_rotation_z(chest));
        }
        pose.rotate_around(Vec3::Y * HIP_HEIGHT, Quat::from_rotation_z(pelvis));
    }

    /// How far from walking into a full run it is, from 0 to 1.
    fn running(&self) -> f32 {
        ((self.pace - WALK_SPEED) / (RUN_SPEED - WALK_SPEED)).clamp(0.0, 1.0)
    }

    /// Where it holds the gun, in its body's own space, given its chest posed at `chest`.
    fn gun_in_body(&self, chest: &Transform) -> Transform {
        let shoulders = chest.transform_point(Vec3::new(0.0, 1.48, 0.0) - Part::Chest.center());
        let grip = shoulders + chest.rotation * LOW_READY_GRIP.lerp(AIM_GRIP, self.aiming);
        let pitch = LOW_READY_PITCH.lerp(self.aim_pitch, self.aiming);
        // Pointing forwards (+Z), tipped up by `pitch`.
        let rotation = Quat::from_rotation_x(-pitch) * Quat::from_rotation_y(PI);
        Transform::from_translation(grip - rotation * GUN_GRIP).with_rotation(rotation)
    }

    /// Puts both its hands on the gun, in its body's own space, bending its arms to reach.
    fn hold_gun(&self, local: &mut [Transform; PART_COUNT]) {
        let gun = self.gun_in_body(&local[Part::Chest.index()]);
        let mut hands = [gun.transform_point(GUN_GRIP + SUPPORT_HAND), gun.transform_point(GUN_GRIP)];
        // Signalling, its off hand leaves the gun to point, chopping, and comes back.
        if self.signalling > 0.0
            && let Some(shoulder) = joint_point(UPPER_ARMS[0], local)
        {
            let t = 1.0 - self.signalling / SIGNAL_TIME;
            let reach = (PI * t).sin().powf(0.5);
            let chest = local[Part::Chest.index()].rotation;
            let chop = Vec3::Y * SIGNAL_CHOP.0 * (SIGNAL_CHOP.1 * t * SIGNAL_TIME).sin();
            let point = shoulder + chest * (SIGNAL_POINT * Vec3::new(-1.0, 1.0, 1.0) + chop);
            hands[0] = hands[0].lerp(point, reach);
        }
        for ((upper, lower), hand) in UPPER_ARMS.into_iter().zip(FOREARMS).zip(hands) {
            let (Some(shoulder), Some(shoulder_rest), Some(elbow_rest)) =
                (joint_point(upper, local), upper.joint().map(|joint| joint.point), lower.joint().map(|joint| joint.point))
            else {
                continue;
            };
            let side = upper.center().x.signum();
            // The elbow pushing out to the side, down and back: a direction from the shoulder.
            let pole = Vec3::new(side * 0.35, -0.35, -0.15);
            let (upper_turn, lower_turn) = two_bone(shoulder, hand, UPPER_ARM_LENGTH, FOREARM_LENGTH, pole, false);
            let elbow = shoulder + upper_turn * (elbow_rest - shoulder_rest);
            local[upper.index()] =
                Transform::from_translation(shoulder + upper_turn * (upper.center() - shoulder_rest)).with_rotation(upper_turn);
            local[lower.index()] =
                Transform::from_translation(elbow + lower_turn * (lower.center() - elbow_rest)).with_rotation(lower_turn);
        }
    }

    /// Turns each part about its joint and all those above it by how far flinching has knocked
    /// them.
    fn apply_flinch(&self, poses: &mut [Transform; PART_COUNT]) {
        let points: [Option<Vec3>; PART_COUNT] = Part::ALL.map(|part| joint_point(part, poses));
        let unflinched = *poses;
        for part in Part::ALL {
            let mut pose = unflinched[part.index()];
            let mut joint = Some(part);
            while let Some(holding) = joint
                && let Some(point) = points[holding.index()]
            {
                let (angle, _) = self.flinch[holding.index()];
                if angle != Vec3::ZERO {
                    pose.rotate_around(point, Quat::from_scaled_axis(angle));
                }
                joint = holding.parent();
            }
            poses[part.index()] = pose;
        }
    }

    /// Makes it flinch from a hit on `part` at `point`, travelling `direction`, in the dummy's
    /// space, delivering `impact` of the shot's momentum: the joint holding the part (or for the
    /// pelvis, the spine above it) is set turning by the shot's leverage about it, and each joint
    /// above it in turn less and less.
    pub fn flinch(&mut self, part: Part, point: Vec3, direction: Vec3, impact: f32) {
        let poses = self.last.unwrap_or_else(|| self.pose());
        let mut joint = if part == Part::Guts { Some(Part::Chest) } else { Some(part) };
        let mut kick = FLINCH_KICK * impact;
        while let Some(holding) = joint
            && let Some(at) = joint_point(holding, &poses)
        {
            self.flinch[holding.index()].1 += (point - at).cross(direction) * kick;
            kick *= FLINCH_FALLOFF;
            joint = holding.parent();
        }
    }

    /// Hands the body over to the physics, its parts carrying on much as the clip had them
    /// moving (see `HANDOFF_MOMENTUM`).
    pub fn stop(&mut self, parts: &[Entity; PART_COUNT], bodies: &mut Query<(&mut RigidBody, &mut Velocity)>) {
        if !self.active {
            return;
        }
        self.active = false;
        for (index, &entity) in parts.iter().enumerate() {
            if let Ok((mut body, mut velocity)) = bodies.get_mut(entity) {
                *body = RigidBody::Dynamic;
                let moving = self.velocities[index];
                *velocity = Velocity { linear: moving.linear * HANDOFF_MOMENTUM, angular: moving.angular * HANDOFF_MOMENTUM };
            }
        }
    }

    /// Takes the body back from the physics, blending from `poses`, where the physics left its
    /// parts in the dummy's space, into the clip, standing where it ended up or, for the walker,
    /// carrying on round its circle from the nearest point on it.
    pub fn resume(&mut self, poses: [Transform; PART_COUNT], parts: &[Entity; PART_COUNT], bodies: &mut Query<(&mut RigidBody, &mut Velocity)>) {
        let pelvis = poses[Part::Guts.index()];
        match self.walk_radius {
            Some(radius) => self.walked = pelvis.translation.z.atan2(pelvis.translation.x) * radius,
            None => {
                self.root = Transform::from_translation(pelvis.translation.with_y(floor_under(&poses)))
                    .with_rotation(Quat::from_rotation_y(yaw(pelvis.rotation)));
            }
        }
        self.start(parts, bodies);
        self.blend = Some((poses, 0.0));
    }

    /// Stands the body straight back up in the clip at `root`, or for the walker where `root`
    /// is on its circle, with no blending: for when it gets back up.
    pub fn restart(&mut self, root: Transform, parts: &[Entity; PART_COUNT], bodies: &mut Query<(&mut RigidBody, &mut Velocity)>) {
        match self.walk_radius {
            Some(radius) => self.walked = root.translation.z.atan2(root.translation.x) * radius,
            None => self.root = root,
        }
        self.start(parts, bodies);
        self.blend = None;
    }

    fn start(&mut self, parts: &[Entity; PART_COUNT], bodies: &mut Query<(&mut RigidBody, &mut Velocity)>) {
        self.active = true;
        self.last = None;
        self.pace = 0.0;
        self.heading_walked = self.facing();
        self.flinch = [(Vec3::ZERO, Vec3::ZERO); PART_COUNT];
        for &entity in parts {
            if let Ok((mut body, mut velocity)) = bodies.get_mut(entity) {
                *body = RigidBody::KinematicPositionBased;
                *velocity = Velocity::zero();
            }
        }
    }

    /// Plays the clip on by `dt`: springing flinches back, with `control` the strength of each
    /// joint's spring as a fraction, and returning every part's pose in the dummy's space.
    fn play(&mut self, dt: f32, control: [f32; PART_COUNT]) -> [Transform; PART_COUNT] {
        self.time += dt;
        if self.walk_radius.is_some() {
            self.walked += WALK_SPEED * dt;
        } else {
            self.walk_steered(dt);
        }
        let base = {
            let flinch = std::mem::replace(&mut self.flinch, [(Vec3::ZERO, Vec3::ZERO); PART_COUNT]);
            let base = self.pose();
            self.flinch = flinch;
            base
        };
        for part in Part::ALL {
            let Some(joint) = part.joint() else {
                continue;
            };
            let (angle, velocity) = &mut self.flinch[part.index()];
            let (stiffness, damping) = FLINCH_SPRING;
            let control = control[part.index()].max(MIN_FLINCH_CONTROL);
            spring(angle, velocity, (stiffness * control, damping * control.sqrt()), dt);
            *angle = angle.clamp_length_max(MAX_FLINCH);
            // Keep the joint within what it allows, from where the clip has it.
            let parent = base[joint.parent.index()].rotation;
            let bent = axis_angles(parent.inverse() * base[part.index()].rotation);
            match joint.kind {
                JointKind::Ball(limits) => {
                    let room = (limits.min_element() * JOINT_LIMIT_MARGIN - bent.length()).max(0.0);
                    *angle = angle.clamp_length_max(room);
                }
                JointKind::Hinge([low, high]) => {
                    let axis = parent * Vec3::X;
                    let bend = angle.dot(axis).max(low * JOINT_LIMIT_MARGIN - bent.x).min(high * JOINT_LIMIT_MARGIN - bent.x);
                    *angle = axis * bend;
                    *velocity = axis * velocity.dot(axis);
                }
            }
        }
        let mut poses = self.pose();
        if self.armed && self.walk_radius.is_none() {
            let idling = 1.0 - self.gait().0;
            let mut chest = chain_pose(Part::Chest, |part| idling * idle_angle(part, self.time));
            self.lean_over(Part::Chest, &mut chest);
            self.gun = self.body_frame() * self.gun_in_body(&chest);
        }
        if let Some((from, elapsed)) = &mut self.blend {
            *elapsed += dt;
            let t = smoothstep(*elapsed / BLEND_TIME);
            for (pose, from) in poses.iter_mut().zip(from.iter()) {
                pose.translation = from.translation.lerp(pose.translation, t);
                pose.rotation = from.rotation.slerp(pose.rotation, t);
            }
            if *elapsed >= BLEND_TIME {
                self.blend = None;
            }
        }
        if let Some(last) = self.last {
            for ((velocity, pose), last) in self.velocities.iter_mut().zip(&poses).zip(&last) {
                *velocity = velocity_between(last, pose, dt);
            }
        }
        self.last = Some(poses);
        poses
    }

    /// Turns and walks it on by `dt` where it is steered.
    fn walk_steered(&mut self, dt: f32) {
        let facing = self.facing();
        let off = self.steer.heading.map_or(0.0, |heading| wrap_angle(heading - facing));
        let turn = off.clamp(-TURN_RATE * dt, TURN_RATE * dt);
        self.root.rotation = Quat::from_rotation_y(facing + turn);
        // Walking the way it faces, slower the further it has to turn, and not at all until it
        // roughly faces the right way; or the way it is told, facing whichever way.
        let (way, lined_up) = match self.steer.travel {
            Some(travel) => (travel, 1.0),
            None => (facing + turn, (1.0 - (off.abs() / WALK_TURN_FIRST)).max(0.0)),
        };
        let wanted = self.steer.speed.max(0.0) * lined_up;
        let change = WALK_ACCELERATION * dt;
        self.pace = wanted.clamp(self.pace - change, self.pace + change);
        let walked_off = wrap_angle(way - self.heading_walked);
        self.heading_walked += walked_off.clamp(-TURN_RATE * dt, TURN_RATE * dt);
        let forward = Quat::from_rotation_y(self.heading_walked) * Vec3::Z;
        self.root.translation += forward * self.pace * dt;
        self.walked += self.pace * dt;
        let stride = STRIDE_LENGTH.lerp(RUN_STRIDE_LENGTH, self.running());
        self.stride = (self.stride + self.pace * dt / stride * 2.0 * PI) % (2.0 * PI);
        // Raising its gun to aim, or lowering it.
        let raised = if self.steer.aim.is_some() { 1.0 } else { 0.0 };
        self.aiming += (raised - self.aiming).clamp(-AIM_RATE * dt, AIM_RATE * dt);
        self.lean += (self.steer.lean - self.lean).clamp(-LEAN_RATE * dt, LEAN_RATE * dt);
        self.signalling = (self.signalling - dt).max(0.0);
        if let Some(pitch) = self.steer.aim {
            self.aim_pitch += (pitch - self.aim_pitch) * (AIM_RATE * 2.0 * dt).min(1.0);
        }
        if let Some(floor) = self.steer.floor {
            let y = &mut self.root.translation.y;
            *y += (floor - *y) * (FLOOR_FOLLOW * dt).min(1.0);
        }
    }
}

/// `angle`, in radians, brought within half a turn either way of zero.
pub fn wrap_angle(angle: f32) -> f32 {
    (angle + PI).rem_euclid(2.0 * PI) - PI
}

/// How fast a part moving from `from` to `to` over `dt` is going.
fn velocity_between(from: &Transform, to: &Transform, dt: f32) -> Velocity {
    let turn = to.rotation * from.rotation.inverse();
    let turn = if turn.w < 0.0 { -turn } else { turn };
    Velocity { linear: (to.translation - from.translation) / dt, angular: turn.to_scaled_axis() / dt }
}

/// Plays every animated dummy's clip, posing its kinematic body parts. Each part's flinch spring
/// is weaker the more it has been hurt and the more the dummy is bleeding, as `control` gives.
pub(super) fn animate(
    time: Res<Time>,
    mut dummies: Query<(&mut Animation, &biped::Biped, &super::Bleeding)>,
    mut parts: Query<(&mut Transform, &super::LimbDamage)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (mut animation, biped, bleeding) in &mut dummies {
        if !animation.active {
            continue;
        }
        let control = Part::ALL.map(|part| {
            let damage = parts.get(biped.part(part)).map_or(0.0, |(_, damage)| damage.0);
            (1.0 - damage / super::LIMB_CRIPPLE_DAMAGE) / bleeding.weakness()
        });
        let poses = animation.play(dt, control);
        for (part, pose) in Part::ALL.into_iter().zip(poses) {
            if let Ok((mut transform, _)) = parts.get_mut(biped.part(part)) {
                *transform = pose;
            }
        }
    }
}

/// A part's pose with every joint from it up to the pelvis turned about the sideways axis by
/// `angle`, positive turning the lower end backwards, carrying everything below it along.
fn chain_pose(part: Part, angle: impl Fn(Part) -> f32) -> Transform {
    let mut pose = standing_pose(part, Transform::IDENTITY);
    let mut joint = part.joint();
    while let Some(holding) = joint {
        pose.rotate_around(holding.point, Quat::from_rotation_x(angle(holding.part)));
        joint = holding.parent.joint();
    }
    pose
}

/// Where the joint holding `part` to its parent is, with the body posed as `poses`.
fn joint_point(part: Part, poses: &[Transform; PART_COUNT]) -> Option<Vec3> {
    let joint = part.joint()?;
    Some(poses[joint.parent.index()].transform_point(joint.point - joint.parent.center()))
}

/// How far the named part is turned about its joint at `phase` through a step cycle, in radians
/// about the sideways axis. The right side is half a cycle behind the left, each arm swings
/// against the leg on its side, and each foot is kept level.
fn walk_angle(part: Part, phase: f32) -> f32 {
    let right = matches!(part, Part::RightThigh | Part::RightShin | Part::RightFoot | Part::RightUpperArm | Part::RightForearm);
    let phase = if right { phase + PI } else { phase };
    let thigh = -LEG_SWING * phase.sin();
    // Bent while the leg swings forwards, straight while it pushes back.
    let knee = KNEE_BEND * phase.cos().max(0.0);
    match part {
        Part::LeftThigh | Part::RightThigh => thigh,
        Part::LeftShin | Part::RightShin => knee,
        Part::LeftFoot | Part::RightFoot => -(thigh + knee),
        Part::LeftUpperArm | Part::RightUpperArm => ARM_SWING * phase.sin(),
        Part::LeftForearm | Part::RightForearm => -ELBOW_BEND,
        _ => 0.0,
    }
}

/// How far the named part is turned about its joint standing idly at time `t`: breathing.
fn idle_angle(part: Part, t: f32) -> f32 {
    let breath = (t * BREATH_RATE).sin();
    match part {
        Part::Chest => -BREATH * breath,
        Part::Head => BREATH * 0.5 * breath,
        Part::LeftUpperArm | Part::RightUpperArm => ARM_SWAY * (t * BREATH_RATE - 1.0).sin(),
        Part::LeftForearm | Part::RightForearm => -0.08,
        _ => 0.0,
    }
}

/// How high the floor is under a body posed as `poses`, in `Part` order and the dummy's space,
/// standing or lying on it: level with the lowest corner of any part.
pub fn floor_under(poses: &[Transform; PART_COUNT]) -> f32 {
    lowest_corner(poses, &Part::ALL)
}

/// The lowest corner of any of `parts`, posed as `poses`.
fn lowest_corner(poses: &[Transform; PART_COUNT], parts: &[Part]) -> f32 {
    parts
        .iter()
        .flat_map(|&part| {
            let pose = poses[part.index()];
            let half = part.size() / 2.0;
            [-1.0, 1.0].into_iter().flat_map(move |x| {
                [-1.0, 1.0]
                    .into_iter()
                    .flat_map(move |z| [-1.0, 1.0].map(|y| pose.transform_point(half * Vec3::new(x, y, z)).y))
            })
        })
        .fold(f32::INFINITY, f32::min)
}

/// Lifts a posed body, in `Part` order and the dummy's space, clear of the floor at `floor`,
/// which is level with its soles as it stands: corners dipping below it are harmless while
/// posed, but the moment the physics takes over it would push them out, popping the body up.
fn lift_clear_of_floor(poses: &mut [Transform; PART_COUNT], floor: f32) {
    let lowest = lowest_corner(poses, &[Part::LeftFoot, Part::RightFoot, Part::LeftShin, Part::RightShin]);
    if lowest < floor {
        for pose in poses.iter_mut() {
            pose.translation.y += floor - lowest;
        }
    }
}

/// Moves a damped spring at `position`, moving at `velocity`, towards zero over `dt`, in small
/// slices so a long step can't make it blow up.
fn spring(position: &mut Vec3, velocity: &mut Vec3, (stiffness, damping): (f32, f32), dt: f32) {
    const MAX_STEP: f32 = 1.0 / 240.0;
    let steps = (dt / MAX_STEP).ceil().max(1.0);
    let step = dt / steps;
    for _ in 0..steps as usize {
        *velocity -= (*position * stiffness + *velocity * damping) * step;
        *position += *velocity * step;
    }
    if position.length() < 1e-4 && velocity.length() < 1e-3 {
        *position = Vec3::ZERO;
        *velocity = Vec3::ZERO;
    }
}
