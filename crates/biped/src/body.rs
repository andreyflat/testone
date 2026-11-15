//! A dummy's physical body: the boxes it is built from, the joints holding them together, and
//! spawning them as jointed rigid bodies whose joint motors are its muscles. Everything here is
//! in the dummy's own space, with the soles at the origin and the front facing +Z.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use bevy_rapier3d::rapier::dynamics::MotorModel;
use game_core::collision::environment_groups;
use game_core::combat::BodyPartKind;

/// The body parts, as indices into `BODY`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Head,
    Neck,
    Chest,
    Guts,
    LeftUpperArm,
    RightUpperArm,
    LeftForearm,
    RightForearm,
    LeftThigh,
    RightThigh,
    LeftShin,
    RightShin,
    LeftFoot,
    RightFoot,
}

pub const PART_COUNT: usize = 14;

impl Part {
    pub const ALL: [Part; PART_COUNT] = [
        Part::Head,
        Part::Neck,
        Part::Chest,
        Part::Guts,
        Part::LeftUpperArm,
        Part::RightUpperArm,
        Part::LeftForearm,
        Part::RightForearm,
        Part::LeftThigh,
        Part::RightThigh,
        Part::LeftShin,
        Part::RightShin,
        Part::LeftFoot,
        Part::RightFoot,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL[index]
    }

    pub fn name(self) -> &'static str {
        BODY[self.index()].0
    }

    pub fn kind(self) -> BodyPartKind {
        BODY[self.index()].1
    }

    /// Where the part's centre is when standing.
    pub fn center(self) -> Vec3 {
        BODY[self.index()].2
    }

    pub fn size(self) -> Vec3 {
        BODY[self.index()].3
    }

    pub fn mass(self) -> f32 {
        let size = self.size();
        size.x * size.y * size.z * density(self)
    }

    /// The joint holding this part to the part it hangs from, if it isn't the pelvis.
    pub fn joint(self) -> Option<&'static BodyJoint> {
        JOINTS.iter().find(|joint| joint.part == self)
    }

    /// The part this one hangs from.
    pub fn parent(self) -> Option<Part> {
        self.joint().map(|joint| joint.parent)
    }

    pub fn is_leg(self) -> bool {
        matches!(
            self,
            Part::LeftThigh | Part::RightThigh | Part::LeftShin | Part::RightShin | Part::LeftFoot | Part::RightFoot
        )
    }

    pub fn is_arm(self) -> bool {
        matches!(self, Part::LeftUpperArm | Part::RightUpperArm | Part::LeftForearm | Part::RightForearm)
    }
}

/// A leg's or arm's parts, from the left side (index 0) or the right (index 1).
pub const THIGHS: [Part; 2] = [Part::LeftThigh, Part::RightThigh];
pub const SHINS: [Part; 2] = [Part::LeftShin, Part::RightShin];
pub const FEET: [Part; 2] = [Part::LeftFoot, Part::RightFoot];
pub const UPPER_ARMS: [Part; 2] = [Part::LeftUpperArm, Part::RightUpperArm];
pub const FOREARMS: [Part; 2] = [Part::LeftForearm, Part::RightForearm];
/// Which way each side lies along the body's sideways axis (+X): left, then right.
pub const SIDE_SIGNS: [f32; 2] = [-1.0, 1.0];

/// The boxes a dummy's body is made of, as (name, kind, centre, size), in `Part` order.
const BODY: [(&str, BodyPartKind, Vec3, Vec3); PART_COUNT] = [
    ("Head", BodyPartKind::Head, Vec3::new(0.0, 1.68, 0.0), Vec3::new(0.2, 0.24, 0.22)),
    ("Neck", BodyPartKind::Neck, Vec3::new(0.0, 1.53, 0.0), Vec3::new(0.1, 0.06, 0.1)),
    ("Chest", BodyPartKind::Chest, Vec3::new(0.0, 1.35, 0.0), Vec3::new(0.4, 0.3, 0.22)),
    ("Guts", BodyPartKind::Guts, Vec3::new(0.0, 1.05, 0.0), Vec3::new(0.36, 0.3, 0.2)),
    ("Left upper arm", BodyPartKind::Arm, Vec3::new(-0.26, 1.35, 0.0), Vec3::new(0.1, 0.3, 0.1)),
    ("Right upper arm", BodyPartKind::Arm, Vec3::new(0.26, 1.35, 0.0), Vec3::new(0.1, 0.3, 0.1)),
    ("Left forearm", BodyPartKind::Arm, Vec3::new(-0.26, 1.05, 0.0), Vec3::new(0.09, 0.3, 0.09)),
    ("Right forearm", BodyPartKind::Arm, Vec3::new(0.26, 1.05, 0.0), Vec3::new(0.09, 0.3, 0.09)),
    ("Left thigh", BodyPartKind::Leg, Vec3::new(-0.1, 0.675, 0.0), Vec3::new(0.16, 0.45, 0.16)),
    ("Right thigh", BodyPartKind::Leg, Vec3::new(0.1, 0.675, 0.0), Vec3::new(0.16, 0.45, 0.16)),
    ("Left shin", BodyPartKind::Leg, Vec3::new(-0.1, 0.265, 0.0), Vec3::new(0.14, 0.37, 0.14)),
    ("Right shin", BodyPartKind::Leg, Vec3::new(0.1, 0.265, 0.0), Vec3::new(0.14, 0.37, 0.14)),
    // Feet reach further forwards than back from the ankle, which sits over the heel.
    ("Left foot", BodyPartKind::Leg, Vec3::new(-0.1, 0.04, 0.05), Vec3::new(0.11, 0.08, 0.26)),
    ("Right foot", BodyPartKind::Leg, Vec3::new(0.1, 0.04, 0.05), Vec3::new(0.11, 0.08, 0.26)),
];

/// Density of the body parts in kg/m³, making the whole body about 70 kg.
const DENSITY: f32 = 600.0;
/// The neck is short and would be light between the heavy head and chest, a mismatch that sets
/// the joints chattering; weighting it closer to them steadies it. Feet are weighted for the
/// same reason against the shins, and so they sit firmly on the floor.
const NECK_DENSITY: f32 = DENSITY * 5.0;
const FOOT_DENSITY: f32 = DENSITY * 1.7;
/// Grip of the soles on the floor.
const FOOT_FRICTION: f32 = 1.2;

fn density(part: Part) -> f32 {
    match part {
        Part::Neck => NECK_DENSITY,
        Part::LeftFoot | Part::RightFoot => FOOT_DENSITY,
        _ => DENSITY,
    }
}

/// A body is many light parts held together by stiff muscles, which take more solver
/// iterations than Rapier's default to hold as firmly as they should.
const SOLVER_ITERATIONS: usize = 20;

/// Hip to knee, knee to ankle, and ankle to sole.
pub const THIGH_LENGTH: f32 = 0.45;
pub const SHIN_LENGTH: f32 = 0.37;
pub const ANKLE_HEIGHT: f32 = 0.08;
/// Shoulder to elbow and elbow to fingertips.
pub const UPPER_ARM_LENGTH: f32 = 0.28;
pub const FOREARM_LENGTH: f32 = 0.3;

/// How a body part is joined to the part it hangs from.
#[derive(Clone, Copy, Debug)]
pub enum JointKind {
    /// Turns up to this many radians either way about each of its axes: X bends forwards and
    /// back, Y twists about the part's length, Z bends sideways.
    Ball(Vec3),
    /// Bends only about the sideways axis, within these limits in radians. Positive bends the
    /// lower part backwards.
    Hinge([f32; 2]),
}

/// A joint between two body parts.
pub struct BodyJoint {
    pub part: Part,
    pub parent: Part,
    /// Where it is, in the dummy's space as it stands.
    pub point: Vec3,
    pub kind: JointKind,
}

impl BodyJoint {
    /// Clamps a target for the joint, as angles about its axes, to within its limits less
    /// `margin`, a fraction, so its muscles never pull it against them.
    pub fn clamp(&self, angles: Vec3, margin: f32) -> Vec3 {
        match self.kind {
            JointKind::Ball(limits) => angles.clamp(-limits * margin, limits * margin),
            JointKind::Hinge([low, high]) => Vec3::new(angles.x.clamp(low * margin, high * margin), 0.0, 0.0),
        }
    }
}

const fn ball(swing: f32, twist: f32) -> JointKind {
    JointKind::Ball(Vec3::new(swing, twist, swing))
}

/// The joints holding a body together.
pub const JOINTS: [BodyJoint; PART_COUNT - 1] = [
    BodyJoint { part: Part::Head, parent: Part::Neck, point: Vec3::new(0.0, 1.56, 0.0), kind: ball(0.4, 0.4) },
    BodyJoint { part: Part::Neck, parent: Part::Chest, point: Vec3::new(0.0, 1.5, 0.0), kind: ball(0.3, 0.3) },
    BodyJoint { part: Part::Chest, parent: Part::Guts, point: Vec3::new(0.0, 1.2, 0.0), kind: ball(0.4, 0.4) },
    // Shoulders twist further, so a hand can be brought across the front of the body with the
    // elbow at the side.
    BodyJoint { part: Part::LeftUpperArm, parent: Part::Chest, point: Vec3::new(-0.26, 1.48, 0.0), kind: ball(1.6, 1.0) },
    BodyJoint { part: Part::RightUpperArm, parent: Part::Chest, point: Vec3::new(0.26, 1.48, 0.0), kind: ball(1.6, 1.0) },
    // Elbows bend the forearm forwards, knees bend the shin backwards.
    BodyJoint { part: Part::LeftForearm, parent: Part::LeftUpperArm, point: Vec3::new(-0.26, 1.2, 0.0), kind: JointKind::Hinge([-2.4, 0.0]) },
    BodyJoint { part: Part::RightForearm, parent: Part::RightUpperArm, point: Vec3::new(0.26, 1.2, 0.0), kind: JointKind::Hinge([-2.4, 0.0]) },
    BodyJoint { part: Part::LeftThigh, parent: Part::Guts, point: Vec3::new(-0.1, 0.9, 0.0), kind: ball(1.2, 0.5) },
    BodyJoint { part: Part::RightThigh, parent: Part::Guts, point: Vec3::new(0.1, 0.9, 0.0), kind: ball(1.2, 0.5) },
    BodyJoint { part: Part::LeftShin, parent: Part::LeftThigh, point: Vec3::new(-0.1, 0.45, 0.0), kind: JointKind::Hinge([0.0, 2.4]) },
    BodyJoint { part: Part::RightShin, parent: Part::RightThigh, point: Vec3::new(0.1, 0.45, 0.0), kind: JointKind::Hinge([0.0, 2.4]) },
    // Ankles tip the foot up and down and roll it a little, but barely twist.
    BodyJoint { part: Part::LeftFoot, parent: Part::LeftShin, point: Vec3::new(-0.1, ANKLE_HEIGHT, 0.0), kind: JointKind::Ball(Vec3::new(0.7, 0.2, 0.35)) },
    BodyJoint { part: Part::RightFoot, parent: Part::RightShin, point: Vec3::new(0.1, ANKLE_HEIGHT, 0.0), kind: JointKind::Ball(Vec3::new(0.7, 0.2, 0.35)) },
];

/// Where the given part stands, in the dummy's space, for a body standing at `placement`.
pub fn standing_pose(part: Part, placement: Transform) -> Transform {
    placement * Transform::from_translation(part.center())
}

/// Spawns a body's parts as children of `dummy`, standing at `placement` in the dummy's space,
/// each a dynamic rigid body jointed to the part it hangs from, with force-based joint motors for
/// muscles. Returns them in `Part` order. Anything else a part needs (what it is when shot, how
/// it looks) is up to the caller.
pub fn spawn_body(commands: &mut Commands, dummy: Entity, placement: Transform) -> [Entity; PART_COUNT] {
    let parts = Part::ALL.map(|part| {
        let size = part.size();
        let mut entity = commands.spawn((
            Name::new(part.name()),
            standing_pose(part, placement),
            RigidBody::Dynamic,
            Velocity::zero(),
            Collider::cuboid(size.x / 2.0, size.y / 2.0, size.z / 2.0),
            ColliderMassProperties::Density(density(part)),
            environment_groups(),
            Damping { linear_damping: 0.1, angular_damping: 0.3 },
            // Muscles are set afresh every step, which a sleeping body wouldn't notice.
            Sleeping::disabled(),
            AdditionalSolverIterations(SOLVER_ITERATIONS),
            ExternalImpulse::default(),
            ChildOf(dummy),
        ));
        if matches!(part, Part::LeftFoot | Part::RightFoot) {
            entity.insert(Friction { coefficient: FOOT_FRICTION, combine_rule: CoefficientCombineRule::Max });
        }
        entity.id()
    });

    for joint in &JOINTS {
        let anchor1 = joint.point - joint.parent.center();
        let anchor2 = joint.point - joint.part.center();
        // Force-based muscles, so their strength is a torque that can carry the weight of the
        // body beyond the joint, not just the two parts it joins. Neighbours overlap at the
        // joint, so only parts not jointed together collide.
        let data: TypedJoint = match joint.kind {
            JointKind::Ball(limits) => SphericalJointBuilder::new()
                .contacts_enabled(false)
                .motor_model(JointAxis::AngX, MotorModel::ForceBased)
                .motor_model(JointAxis::AngY, MotorModel::ForceBased)
                .motor_model(JointAxis::AngZ, MotorModel::ForceBased)
                .local_anchor1(anchor1)
                .local_anchor2(anchor2)
                .limits(JointAxis::AngX, [-limits.x, limits.x])
                .limits(JointAxis::AngY, [-limits.y, limits.y])
                .limits(JointAxis::AngZ, [-limits.z, limits.z])
                .into(),
            JointKind::Hinge(limits) => RevoluteJointBuilder::new(Vec3::X)
                .contacts_enabled(false)
                .motor_model(MotorModel::ForceBased)
                .local_anchor1(anchor1)
                .local_anchor2(anchor2)
                .limits(limits)
                .into(),
        };
        commands.entity(parts[joint.part.index()]).insert(ImpulseJoint::new(parts[joint.parent.index()], data));
    }
    parts
}
