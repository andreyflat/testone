//! Reactive self-balancing bipeds: a controller that keeps a jointed, fully simulated body on
//! its feet using nothing but its own muscles (joint motors) and the floor, the way a person
//! does, and animates it procedurally as it goes.
//!
//! Every physics step it measures the body (centre of mass, its velocity, which feet are on the
//! floor) and works out:
//! - **Where it needs to stand.** From the centre of mass and its velocity comes the capture
//!   point (`com + velocity / ω`, the linear inverted pendulum's divergent component of motion):
//!   where a foot would have to be to bring the body to rest. While that stays within reach of
//!   its feet, it balances in place, pulling its weight back over them from the ankles and hips
//!   (with an integral term, so its weight leaning on its muscles doesn't leave it leaning).
//!   Once the capture point strays beyond its soles, it takes a step, the foot aimed at where
//!   the capture point will be when it lands, re-aimed every physics step as the body moves,
//!   never crossing the other leg (pushed sideways, it shuffles: outer foot out, inner foot
//!   after). Settled, it tidies its feet back into its stance, shifting its weight off the foot
//!   first, as lifting a foot with its weight between them would topple it. Walking is the same
//!   stepping done on purpose: each step aimed short of the capture point by as much as keeps
//!   the body falling on at its pace, speeding up and turning gradually.
//! - **A pose.** Stance legs reach down by IK from where its hips should be to where its feet
//!   are planted, holding the pelvis at standing height over where its weight should be. The
//!   swing leg reaches from where its hip is to where its foot is going, lifting it mid-step.
//!   Feet are held flat. The upper body holds its posture, the head steadies itself against the
//!   body's wobbling, and the arms hang, swing against the legs, or are thrown out as it loses
//!   its balance.
//! - **Muscle targets.** Each joint motor pulls towards its part of the pose. The stance hips
//!   instead pull the pelvis upright in the world, whatever the legs are doing, pushing against
//!   the planted feet (as SIMBICON does), their damping steadying the pelvis in the world rather
//!   than against the leg turning beneath it, so knocks to the body are resisted through the
//!   legs. Motors are capped at what real muscles could manage: ankles can't hold more than the
//!   foot can push against without tipping onto its toes.
//!
//! Nothing holds it up but that: no hidden forces, no anchors. Knocked hard enough, it steps,
//! staggers or falls. How hard it tries, and in what posture, is up to its `Drive`.
//!
//! `examples/biped_sim.rs` runs one headless, shoving it about, for tuning; `tests/biped.rs`
//! checks it stands, steps, walks and falls as it should.

pub mod body;
pub mod ik;

use std::f32::consts::PI;

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use game_core::GRAVITY;
use game_core::math::smoothstep;

use crate::body::{
    ANKLE_HEIGHT, FEET, FOREARM_LENGTH, FOREARMS, JointKind, PART_COUNT, Part, SHIN_LENGTH, SHINS, SIDE_SIGNS, THIGH_LENGTH,
    THIGHS, UPPER_ARM_LENGTH, UPPER_ARMS,
};
use crate::ik::{axis_angles, from_axis_angles, two_bone, yaw, yaw_rotation};

/// How far a standing biped's stance legs are extended, as a fraction of their length: just
/// short of straight, so the knees stay soft and can take up height changes.
const STANCE_EXTENSION: f32 = 0.985;
/// How far apart its feet are when it stands, from the middle of one sole to the other.
const STANCE_WIDTH: f32 = 0.2;

/// Holding its balance in place, it pulls its centre of mass back over where it wants it, as a
/// shift of where its hips should be per metre the centre of mass is off it...
const BALANCE_POSITION_GAIN: f32 = 1.6;
/// ...and against how fast it is moving, per metre per second...
const BALANCE_VELOCITY_GAIN: f32 = 0.6;
/// ...and, to make up for its weight leaning on its muscles (which pull back only in proportion
/// to how far it is off), by however far it has been off for a while, building up at this rate
/// per second, up to this far, in metres...
const BALANCE_INTEGRAL_GAIN: f32 = 1.0;
const BALANCE_MAX_INTEGRAL: f32 = 0.15;
/// ...all told, up to this far, in metres: beyond that, it would rather step.
const BALANCE_MAX_SHIFT: f32 = 0.2;
/// Stepping, it only brakes a little from the stance ankle, against how fast it is moving, per
/// metre per second: the step is what catches it, aimed for a body that carries on as it is.
const STEP_BRAKE_GAIN: f32 = 0.15;
/// What it has built up leaning against its weight fades away while it steps, with this time
/// constant in seconds: it will be standing somewhere else.
const LEAN_IN_FADE: f32 = 0.2;
/// It speeds up and slows down by at most this much, in metres per second squared, and turns at
/// most this fast, in radians per second.
const WALK_ACCELERATION: f32 = 0.6;
const MAX_TURN_RATE: f32 = 0.8;
/// Walking, it presses on from the stance ankle to keep up its speed, per metre per second it is
/// short of it.
const WALK_PUSH_GAIN: f32 = 0.08;

/// It steps once its capture point strays this far beyond the feet it is standing on, in metres,
/// forwards or back and to the sides: about the edges of its soles.
const SUPPORT_MARGIN: Vec2 = Vec2::new(0.12, 0.06);
/// How long a step takes at full strength, in seconds, standing and walking.
const STEP_DURATION: f32 = 0.3;
const WALK_STEP_DURATION: f32 = 0.35;
/// Steps are quicker the further it is off balance: this many times quicker again for every
/// `SUPPORT_MARGIN` its capture point is beyond its feet, up to two margins' worth.
const STEP_URGENCY: f32 = 0.3;
/// How high it lifts its foot mid-step, in metres, standing and walking.
const STEP_LIFT: f32 = 0.1;
const WALK_STEP_LIFT: f32 = 0.09;
/// Furthest it reaches with a step at full strength, from beneath its hip, in metres.
const MAX_STEP: f32 = 0.65;
/// A step never puts a foot closer than this to the other's side, in metres between the
/// middles of the soles, so its legs never cross.
const MIN_STEP_WIDTH: f32 = 0.14;
/// A step lands once the foot touches the floor this far through it, if it is near where it was
/// aiming (this close, in metres: otherwise it has just stubbed its toe), and anywhere once this
/// far through; failing that, it is cut short, foot down wherever it is, by this far through.
const STEP_LAND_FROM: f32 = 0.5;
const STEP_LAND_NEAR: f32 = 0.08;
const STEP_LAND_ANYWHERE: f32 = 0.9;
const STEP_LAND_BY: f32 = 1.3;
/// A swinging foot gets over where it is going by this far through the step, then comes down
/// onto it, rather than skimming the floor on its way and landing short.
const SWING_REACH_BY: f32 = 0.75;
/// Once still, it tidies its feet back into its stance: if they are this far, in metres, from
/// where they belong to each other, or turned this far, in radians, from the way it faces...
const TIDY_DISTANCE: f32 = 0.1;
const TIDY_TURN: f32 = 0.35;
/// ...and it is moving slower than this, in metres per second.
const TIDY_SPEED: f32 = 0.2;
/// Before an unhurried step it shifts its weight onto the other foot, as lifting a foot with its
/// weight between them would topple it that way. It eases its weight over for this long, in
/// seconds, then steps once its capture point is this close to over the other foot sideways, in
/// metres, and it is moving sideways slower than this, in metres per second, or anyway after
/// this long, in seconds...
const SHIFT_EASE: f32 = 0.8;
const SHIFT_DONE: f32 = 0.03;
const SHIFT_SETTLED: f32 = 0.06;
const SHIFT_TIME: f32 = 1.5;
/// ...and steps this many times more slowly than to catch itself, its weight kept over the
/// other foot.
const TIDY_SLOWER: f32 = 1.5;

/// It has fallen over once its pelvis tips this far from upright, as the cosine of its tilt,
/// or its centre of mass is this low over its feet, in metres, and gives up balancing: stepping,
/// and holding its body upright, are no use any more.
const FALLEN_TILT: f32 = 0.5;
const FALLEN_HEIGHT: f32 = 0.55;

/// Fallen over, its legs lie out with the knees bent this far, in radians, at this fraction of
/// their strength: barely holding the pose, so they flop as they land.
const FALLEN_KNEE_BEND: f32 = 0.2;
const FALLEN_LEG_STRENGTH: f32 = 0.15;
/// Kneeling, it only counts as fallen over once its centre of mass is this low over the floor...
const KNEELING_FALLEN_HEIGHT: f32 = 0.3;
/// ...holding its legs folded under it as the `axis_angles` of its hips, knees and ankles: thighs
/// sloping forwards from the pelvis to the knees and spread apart this far each, so it doesn't
/// topple sideways, knees folded right up so it sits back on its heels, its torso over its
/// shins, toes pointed, at this fraction of its legs' strength.
const KNEEL_HIP: f32 = -0.6;
const KNEEL_SPREAD: f32 = 0.35;
const KNEEL_KNEE: f32 = 2.1;
const KNEEL_ANKLE: f32 = 0.6;
const KNEEL_STRENGTH: f32 = 1.0;
const KNEEL_ANKLE_STRENGTH: f32 = 0.1;
/// Kneeling, it keeps its weight this far behind its knees, in metres, over its shins, bending
/// its knees further, in radians, by this much per metre its weight is ahead of there and per
/// metre per second it is heading forwards, and straightening them as much the other way, by up
/// to this much.
const KNEEL_SIT_BACK: f32 = 0.35;
const KNEEL_BALANCE_GAIN: f32 = 2.0;
const KNEEL_BALANCE_DAMPING: f32 = 0.5;
const KNEEL_MAX_FOLD: f32 = 0.3;
/// Its shins count as lying along the floor, for holding itself upright on its knees, once they
/// are no more upright than this, as the cosine of their tilt, fully once flat; and once they
/// have, they go on counting so until it gets up.
const KNEEL_SHIN_DOWN: f32 = 0.6;
/// Until then, dropping onto its knees, it holds its thighs this far forwards of straight down in
/// the world, so its hips fall straight down onto its knees, its shins folding back under it,
/// rather than sitting back, and its pelvis pitched this far forwards, in radians, by this much,
/// as a fraction, as hard as it is trying to balance, so its weight goes over its knees rather
/// than its hips sinking back over its heels and tipping it over backwards.
const KNEEL_DROP_THIGH: f32 = -0.1;
const KNEEL_DROP_LEAN: f32 = 0.35;
const KNEEL_DROP_UPRIGHT: f32 = 0.8;
const KNEEL_DROP_KNEE: f32 = 1.6;

/// How far its arms hang out from its sides, in radians, and how much further they are thrown out
/// per metre its capture point is off to the side, and swing per metre forwards or back.
const ARM_HANG: f32 = 0.16;
const ARM_SPREAD_PER_METRE: f32 = 2.0;
const ARM_SWING_PER_METRE: f32 = 1.2;
/// Furthest it throws its arms out to the sides, and swings them forwards or back, to catch its
/// balance, in radians.
const ARM_MAX_SPREAD: f32 = 1.1;
const ARM_MAX_SWING: f32 = 0.7;
/// How far its arms swing against its legs as it walks, per radian of the leg's swing.
const ARM_WALK_SWING: f32 = 0.8;
/// How much its arms react to its balance while it walks, as a fraction.
const WALK_ARM_REFLEX: f32 = 0.2;
/// How bent it keeps its elbows, in radians, and how much more when throwing its arms out.
const ELBOW_BEND: f32 = 0.15;
const ELBOW_BRACE: f32 = 0.35;
/// How far it leans its body into the way it is walking, in radians per metre per second.
const WALK_LEAN: f32 = 0.06;
/// How fully its head steadies itself in the world rather than following the body, from 0 to 1.
const HEAD_STEADYING: f32 = 0.7;
/// Its muscles' targets keep this fraction inside the joints' limits.
const JOINT_LIMIT_MARGIN: f32 = 0.9;

/// A sloppy biped's balance, at `Drive::sloppiness` 1: it misjudges where its capture point is
/// by up to this far, in metres; its balance gains, and how far off balance it lets itself get
/// before stepping, wander by up to this fraction; its steps take up to this fraction longer or
/// shorter, and land up to this far, in metres, from where it means; and its muscles' targets
/// wobble this much more, as a fraction of `WOBBLE`. All as waves of these frequencies, in
/// radians per second, slow enough to look like misjudgement rather than jitter.
const SLOPPY_CAPTURE: f32 = 0.12;
const SLOPPY_GAIN: f32 = 0.6;
const SLOPPY_STEP_TIME: f32 = 0.5;
const SLOPPY_FOOTING: f32 = 0.12;
const SLOPPY_WOBBLE: f32 = 1.5;
const SLOPPY_RATES: (f32, f32) = (1.1, 2.9);
const SLOPPY_CHANNELS: usize = 8;

/// A languid biped, at `Drive::languor` 1: senses how it is moving this late, as a time
/// constant in seconds, for deciding what to do (its legs keep holding it up from how it really
/// moves); eases its muscles' targets, but for those of the legs it stands on, towards where
/// they're aiming with this time constant, in seconds; has its muscles damped this many times more heavily again; rests this
/// long, in seconds, after each step before it can take another; and fidgets this much more
/// slowly, as a fraction.
const LANGUOR_LAG: f32 = 0.35;
/// `Drive::languor` can go past 1, more languid still, up to this.
const MAX_LANGUOR: f32 = 2.0;
const LANGUOR_EASE: f32 = 0.3;
/// Taking over a body from elsewhere, or changing what it is doing all at once (see
/// `Biped::ease_in`), its muscles come round to their new targets gently: from where its body is
/// then, easing over this long, in seconds, its upper body and the legs it stands on.
const HANDOFF_EASE: (f32, f32) = (0.22, 0.08);
const LANGUOR_DAMPING: f32 = 1.5;
const LANGUOR_STEP_REST: f32 = 0.8;
const LANGUOR_SLOW_FIDGET: f32 = 0.75;

/// The upper body's and arms' muscles are damped this many times more heavily than the strengths
/// below give, so they settle instead of springing about. Not the legs': damped more, they would
/// swing too sluggishly to catch it.
const UPPER_BODY_DAMPING: f32 = 1.6;

/// Muscle strengths at full strength, as (stiffness in newton-metres per radian, damping in
/// newton-metres per radian per second, most torque in newton-metres).
const HIP_MUSCLES: (f32, f32, f32) = (1100.0, 70.0, 500.0);
const KNEE_MUSCLES: (f32, f32, f32) = (1100.0, 50.0, 450.0);
/// A swinging leg carries only its own weight, so it swings with this fraction of the leg's
/// strength: at full strength, it would kick the pelvis about as it swings.
const SWING_STRENGTH: f32 = 1.0;
/// Ankles are capped below what would tip a foot up on its toes or heel.
const ANKLE_MUSCLES: (f32, f32, f32) = (450.0, 20.0, 110.0);
const SPINE_MUSCLES: (f32, f32, f32) = (800.0, 45.0, 350.0);
const NECK_MUSCLES: (f32, f32, f32) = (350.0, 16.0, 120.0);
const HEAD_MUSCLES: (f32, f32, f32) = (160.0, 7.0, 60.0);
const SHOULDER_MUSCLES: (f32, f32, f32) = (140.0, 7.0, 70.0);
const ELBOW_MUSCLES: (f32, f32, f32) = (70.0, 3.5, 35.0);

/// The muscles' targets wobble about where they aim, so it never stands statue still: up to
/// this far, in radians, at a `Drive::wobble` of 1, as the sum of waves of these frequencies, in
/// radians per second, the second at half the size.
const WOBBLE: f32 = 0.1;
const WOBBLE_RATES: (f32, f32) = (1.7, 4.3);

/// Drawing a knee up, the `axis_angles` of its hip and knee, at this fraction of its legs'
/// strength.
const TUCK_HIP: Vec3 = Vec3::new(-1.05, 0.0, 0.0);
const TUCK_KNEE: Vec3 = Vec3::new(2.0, 0.0, 0.0);
const TUCK_STRENGTH: f32 = 0.7;
/// Writhing, its joints bend and twist about as far as this, in radians, with waves at these
/// rates, in radians per second; and its legs, however slack, at least at this fraction of their
/// strength. Convulsing, much quicker and not so far.
const WRITHE_ANGLE: f32 = 0.9;
const WRITHE_RATES: (f32, f32) = (2.1, 4.7);
const WRITHE_LEG_STRENGTH: f32 = 0.5;
const CONVULSE_ANGLE: f32 = 0.5;
const CONVULSE_RATES: (f32, f32) = (23.0, 37.0);
/// Time step for working out how fast writhing and convulsing turn its joints, in seconds.
const WIGGLE_DT: f32 = 0.005;

/// The curled-up pose, as the `axis_angles` of the joint holding each part: knees drawn up,
/// back and neck bent forwards, head tucked, arms drawn in with elbows bent.
const CURL_POSE: [(Part, Vec3); 11] = [
    (Part::LeftThigh, Vec3::new(-1.0, 0.0, 0.0)),
    (Part::RightThigh, Vec3::new(-1.0, 0.0, 0.0)),
    (Part::LeftShin, Vec3::new(1.9, 0.0, 0.0)),
    (Part::RightShin, Vec3::new(1.9, 0.0, 0.0)),
    (Part::Chest, Vec3::new(0.35, 0.0, 0.0)),
    (Part::Neck, Vec3::new(0.25, 0.0, 0.0)),
    (Part::Head, Vec3::new(0.35, 0.0, 0.0)),
    (Part::LeftUpperArm, Vec3::new(-1.2, 0.0, 0.3)),
    (Part::RightUpperArm, Vec3::new(-1.2, 0.0, -0.3)),
    (Part::LeftForearm, Vec3::new(-1.9, 0.0, 0.0)),
    (Part::RightForearm, Vec3::new(-1.9, 0.0, 0.0)),
];

/// Runs the bipeds' balance every frame, ahead of the physics step in `PostUpdate`, after
/// whatever sets their `Drive`s.
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub struct BipedSystems;

pub struct BipedPlugin;

impl Plugin for BipedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, balance.in_set(BipedSystems));
    }
}

/// What a biped is trying to do, and how hard: set by whatever controls it, read every step.
#[derive(Component, Clone, Debug)]
pub struct Drive {
    /// Strength of all its muscles, from 0 (limp) to 1.
    pub strength: f32,
    /// Further strength of its legs, neck and head, and arms, as fractions.
    pub legs: f32,
    pub neck: f32,
    pub arms: f32,
    /// How firmly its upper body and arms hold their pose, as a fraction of their stiffness:
    /// low, it springs back sluggishly from knocks.
    pub tone: f32,
    /// How hard it tries to stay on its feet, from 0 to 1: holding itself upright from the hips
    /// and pulling its weight back over its feet. Its legs carry on holding their pose regardless.
    pub balance: f32,
    /// Whether it steps to catch itself.
    pub steps: bool,
    /// How far it can reach with a step, and how long a step takes, as fractions of its best.
    pub reach: f32,
    pub step_time: f32,
    /// How fast, and which way, it wants to walk, horizontally in world space.
    pub velocity: Vec3,
    /// Which way it wants to face, as an angle about the vertical, if it cares.
    pub heading: Option<f32>,
    /// Where it holds its weight, relative to between its feet, horizontally in world space: off
    /// to one side, it leans that way and sways, stepping to catch itself if it's too far.
    pub sway: Vec3,
    /// How far it tips its pelvis from upright, as a rotation vector in world space.
    pub lean: Vec3,
    /// How far it sinks at the knees, in metres.
    pub crouch: f32,
    /// How far it slumps its chest and hangs its head forwards, and twists its chest round on
    /// its pelvis, in radians.
    pub slump: f32,
    pub droop: f32,
    pub twist: f32,
    /// How far it draws itself into a curled-up pose over everything else, as a fraction.
    pub curl: f32,
    /// How far it has dropped onto its knees, from 0 (on its feet) to 1 (kneeling, sat back on
    /// its heels, holding its pelvis upright from the hips): its legs fold into that pose instead
    /// of standing or stepping, and down there it doesn't count as fallen over. It should stop
    /// stepping meanwhile.
    pub kneel: f32,
    /// How far it draws each leg's knee up towards its chest, left then right, over everything
    /// else its legs are doing, as a fraction: on its front, that brings its knees under it.
    pub tuck: [f32; 2],
    /// How hard it writhes, twisting and bending all over, slowly, and convulses, jerking all over
    /// in quick spasms, as fractions.
    pub writhe: f32,
    pub convulse: f32,
    /// How much its arms react to its balance, as a fraction.
    pub arm_reflex: f32,
    /// Where it wants its hands, if anywhere in particular, left then right.
    pub hands: [Option<HandTarget>; 2],
    /// How much its muscles' targets wobble about, as a fraction of `WOBBLE`.
    pub wobble: f32,
    /// Most torque its muscles can put out, as a fraction of their usual most: capping how
    /// sharply they can change how its body is moving, however hard they pull.
    pub force: f32,
    /// How imprecise its balance is, from 0 (as precise as it gets) to 1: its sense of where its
    /// weight is going, how hard it corrects for it, when it decides to step, how quickly and
    /// where it puts its foot, all wander about at random. See `SLOPPY_CAPTURE` and on.
    pub sloppiness: f32,
    /// How unresponsive it is, from 0 (as quick as it gets) to 1, or further, up to 2: its sense of how it is moving
    /// lags behind, its muscles ease slowly towards what it means to do and are heavily damped,
    /// it rests between steps, and it fidgets more slowly. See `LANGUOR_LAG` and on.
    pub languor: f32,
}

impl Default for Drive {
    fn default() -> Self {
        Self {
            strength: 1.0,
            legs: 1.0,
            neck: 1.0,
            arms: 1.0,
            tone: 1.0,
            balance: 1.0,
            steps: true,
            reach: 1.0,
            step_time: 1.0,
            velocity: Vec3::ZERO,
            heading: None,
            sway: Vec3::ZERO,
            lean: Vec3::ZERO,
            crouch: 0.0,
            slump: 0.0,
            droop: 0.0,
            twist: 0.0,
            curl: 0.0,
            kneel: 0.0,
            tuck: [0.0; 2],
            writhe: 0.0,
            convulse: 0.0,
            arm_reflex: 1.0,
            hands: [None, None],
            wobble: 0.15,
            force: 1.0,
            sloppiness: 0.0,
            languor: 0.0,
        }
    }
}

/// Where a biped wants one of its hands.
#[derive(Clone, Copy, Debug)]
pub struct HandTarget {
    /// Where the fingertips go, in world space.
    pub point: Vec3,
    /// Which way the elbow points, in the chest's space.
    pub elbow: Vec3,
    /// How far towards it the arm goes over its usual pose, from 0 to 1.
    pub weight: f32,
    /// Strength of the arm while reaching, as a fraction of its usual strength.
    pub strength: f32,
}

/// Each body part's own strength, as a fraction: lowered by injury.
#[derive(Component, Clone, Copy, Debug)]
pub struct Muscle(pub f32);

impl Default for Muscle {
    fn default() -> Self {
        Self(1.0)
    }
}

/// A self-balancing biped: the dummy whose body parts these are, children of it.
#[derive(Component, Debug)]
pub struct Biped {
    /// Its body parts, in `Part` order.
    pub parts: [Entity; PART_COUNT],
    gait: Gait,
    /// Which way it faces, once it has settled on it.
    heading: Option<f32>,
    /// Height of the floor beneath it.
    ground: f32,
    /// The foot that stepped last, to walk with the other one next.
    last_step: usize,
    /// Fallen over, so not trying to step or hold itself up.
    fallen: bool,
    /// How long it has been going, for its wobbling.
    time: f32,
    /// Where each muscle's wobble starts in its waves, in `Part` order.
    wobble_phase: [Vec3; PART_COUNT],
    /// Where each of its balance's sloppiness noises starts in its waves; see `Drive::sloppiness`.
    sloppy_phase: [f32; SLOPPY_CHANNELS],
    /// How far its capture point was beyond its feet last step, as a fraction of `SUPPORT_MARGIN`.
    off_balance: f32,
    /// Where it senses its centre of mass is and how fast it is moving, lagging behind the truth
    /// as it grows languid; see `Drive::languor`.
    sensed: Option<(Vec3, Vec3)>,
    /// Its muscles' targets as they ease towards where they're aiming, in `Part` order.
    eased: [Option<Quat>; PART_COUNT],
    /// How much longer it rests after a step before it can take another, in seconds.
    rest: f32,
    /// Kneeling, whether it has got down onto its knees yet.
    knelt: bool,
    /// The clock its fidgeting runs on, slowing as it grows languid.
    fidget_time: f32,
    /// Its muscles have been let go.
    limp: bool,
    /// How fast, and which way, it is walking, working up to what its `Drive` asks.
    pace: Vec3,
    /// How far it has built up leaning against its weight while holding its balance in place;
    /// see `BALANCE_INTEGRAL_GAIN`.
    lean_in: Vec3,
    /// How much longer its muscles ease gently into their targets, in seconds, and whether to
    /// start them from where its body is, on the next step.
    handoff: f32,
    seed: bool,
}

impl Biped {
    pub fn new(parts: [Entity; PART_COUNT]) -> Self {
        Self {
            parts,
            gait: Gait::Standing,
            heading: None,
            ground: 0.0,
            last_step: 1,
            fallen: false,
            time: 0.0,
            wobble_phase: std::array::from_fn(|_| Vec3::from_array(std::array::from_fn(|_| 2.0 * PI * fastrand::f32()))),
            sloppy_phase: std::array::from_fn(|_| 2.0 * PI * fastrand::f32()),
            off_balance: 0.0,
            sensed: None,
            eased: [None; PART_COUNT],
            rest: 0.0,
            knelt: false,
            fidget_time: 0.0,
            limp: false,
            pace: Vec3::ZERO,
            lean_in: Vec3::ZERO,
            handoff: 0.0,
            seed: false,
        }
    }

    pub fn part(&self, part: Part) -> Entity {
        self.parts[part.index()]
    }

    /// Which way a body part with the given rotation faces, as an angle about the vertical.
    pub fn facing(rotation: &Quat) -> f32 {
        yaw(*rotation)
    }

    /// Whether it has fallen over: tipped right over or down on the floor.
    pub fn fallen(&self) -> bool {
        self.fallen
    }

    /// Whether it is taking a step.
    pub fn stepping(&self) -> bool {
        matches!(self.gait, Gait::Stepping(_))
    }

    /// Has its muscles ease gently into whatever they aim for over the next `seconds`, rather than
    /// pulling straight there; starting `from_pose`, from wherever its body is, as when taking
    /// over a body something else was posing.
    pub fn ease_in(&mut self, seconds: f32, from_pose: bool) {
        self.handoff = self.handoff.max(seconds);
        self.seed |= from_pose;
    }

    /// Starts it afresh, standing still, as when it is stood back up.
    pub fn reset(&mut self) {
        self.gait = Gait::Standing;
        self.heading = None;
        self.fallen = false;
        self.limp = false;
        self.pace = Vec3::ZERO;
        self.lean_in = Vec3::ZERO;
        self.off_balance = 0.0;
        self.sensed = None;
        self.eased = [None; PART_COUNT];
        self.rest = 0.0;
        self.knelt = false;
    }

    /// How far off balance it is: how far its capture point is beyond the feet it stands on, as
    /// a fraction of what they can hold it over. Past 1, it would need to step to catch itself.
    pub fn off_balance(&self) -> f32 {
        self.off_balance
    }

    /// Smooth noise from -1 to 1 for one of its balance's sloppiness channels.
    fn sloppy(&self, channel: usize) -> f32 {
        let (slow, fast) = SLOPPY_RATES;
        let phase = self.sloppy_phase[channel];
        let t = self.fidget_time;
        ((t * slow + phase).sin() + (t * fast + phase * 1.618).sin() * 0.5) / 1.5
    }

    fn wobble(&self, part: Part) -> Vec3 {
        self.noise(part, WOBBLE_RATES, self.fidget_time)
    }

    /// Smooth noise from -1 to 1 about each axis of one part's joint, mixing waves at the given
    /// rates, in radians per second, at time `time`.
    fn noise(&self, part: Part, (slow, fast): (f32, f32), time: f32) -> Vec3 {
        let phase = self.wobble_phase[part.index()];
        let wave = |rate: f32, offset: Vec3| (Vec3::splat(time * rate) + offset).to_array().map(f32::sin);
        let (a, b) = (Vec3::from_array(wave(slow, phase)), Vec3::from_array(wave(fast, phase.zxy() * 1.618)));
        (a + b * 0.5) / 1.5
    }
}

#[derive(Clone, Copy, Debug)]
enum Gait {
    /// On both feet, or whichever are down.
    Standing,
    /// Shifting its weight onto one foot, to step with the other (0 for the left, 1 for the
    /// right) once it can lift it without toppling that way, for this long so far, in seconds.
    Shifting { foot: usize, time: f32 },
    Stepping(Step),
}

#[derive(Clone, Copy, Debug)]
struct Step {
    /// The swinging foot: 0 for the left, 1 for the right.
    foot: usize,
    /// Where its sole set off from, on the floor.
    from: Vec3,
    /// Where it is aiming, on the floor, and which way it will face.
    to: Vec3,
    yaw: f32,
    /// How far through the step it is, from 0, and how long the step takes, in seconds.
    progress: f32,
    duration: f32,
    lift: f32,
    /// A step taken just to tidy the feet, which doesn't chase the capture point.
    tidy: bool,
}

/// A body part's state, from the physics.
#[derive(Clone, Copy)]
struct PartState {
    position: Vec3,
    rotation: Quat,
    velocity: Vec3,
    spin: Vec3,
}

impl PartState {
    fn point(&self, local: Vec3) -> Vec3 {
        self.position + self.rotation * local
    }
}

/// Where on a foot its sole's middle is, in the foot's own space.
fn sole_offset(part: Part) -> Vec3 {
    Vec3::NEG_Y * part.size().y / 2.0
}

/// Where a part's joint to its parent is, in the part's own space and in the parent's.
fn joint_offsets(part: Part) -> (Vec3, Vec3) {
    let joint = part.joint().expect("names a jointed part");
    (joint.point - part.center(), joint.point - joint.parent.center())
}

/// Muscles of the joint holding the given part.
fn muscles(part: Part) -> (f32, f32, f32) {
    match part {
        Part::LeftThigh | Part::RightThigh => HIP_MUSCLES,
        Part::LeftShin | Part::RightShin => KNEE_MUSCLES,
        Part::LeftFoot | Part::RightFoot => ANKLE_MUSCLES,
        Part::Chest => SPINE_MUSCLES,
        Part::Neck => NECK_MUSCLES,
        Part::Head => HEAD_MUSCLES,
        Part::LeftUpperArm | Part::RightUpperArm => SHOULDER_MUSCLES,
        Part::LeftForearm | Part::RightForearm => ELBOW_MUSCLES,
        Part::Guts => (0.0, 0.0, 0.0),
    }
}

/// How far to turn from angle `from` to angle `to`, the short way round, in radians.
fn angle_between(from: f32, to: f32) -> f32 {
    (to - from + PI).rem_euclid(2.0 * PI) - PI
}

/// Balances every biped and poses it as its `Drive` asks, by setting its joint motors.
fn balance(
    time: Res<Time>,
    rapier: ReadRapierContext,
    mut bipeds: Query<(&mut Biped, &Drive)>,
    parts: Query<(&GlobalTransform, &Velocity, Option<&Muscle>, Option<&Children>)>,
    mut joints: Query<&mut ImpulseJoint>,
) -> Result {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return Ok(());
    }
    let rapier = rapier.single()?;
    for (mut biped, drive) in &mut bipeds {
        let mut states =
            [PartState { position: Vec3::ZERO, rotation: Quat::IDENTITY, velocity: Vec3::ZERO, spin: Vec3::ZERO }; PART_COUNT];
        let mut strengths = [1.0; PART_COUNT];
        let mut complete = true;
        for (index, &entity) in biped.parts.iter().enumerate() {
            let Ok((transform, velocity, muscle, _)) = parts.get(entity) else {
                complete = false;
                break;
            };
            let (_, rotation, position) = transform.to_scale_rotation_translation();
            states[index] = PartState { position, rotation, velocity: velocity.linear, spin: velocity.angular };
            strengths[index] = muscle.map_or(1.0, |muscle| muscle.0.clamp(0.0, 1.0));
        }
        if !complete {
            continue;
        }
        // Taking over, its muscles start from where its body is.
        if std::mem::take(&mut biped.seed) {
            for part in Part::ALL {
                if let Some(joint) = part.joint() {
                    let parent = states[joint.parent.index()].rotation;
                    biped.eased[part.index()] = Some(parent.inverse() * states[part.index()].rotation);
                }
            }
        }

        if drive.strength <= 0.0 {
            if !biped.limp {
                for &entity in &biped.parts {
                    if let Ok(mut joint) = joints.get_mut(entity) {
                        relax(&mut joint);
                    }
                }
                biped.limp = true;
                biped.gait = Gait::Standing;
            }
            continue;
        }
        biped.limp = false;
        biped.time += dt;

        // Its own colliders, which don't count as the floor.
        let own: Vec<Entity> = biped
            .parts
            .iter()
            .flat_map(|&part| {
                let armor = parts.get(part).ok().and_then(|(.., children)| children).into_iter().flatten().copied();
                std::iter::once(part).chain(armor)
            })
            .collect();
        let on_floor = |foot: Entity| {
            rapier.contact_pairs_with(foot).any(|pair| {
                let other = if pair.collider1() == Some(foot) { pair.collider2() } else { pair.collider1() };
                pair.has_any_active_contact() && other.is_some_and(|other| !own.contains(&other))
            })
        };
        let contact = FEET.map(|foot| on_floor(biped.part(foot)));
        let (targets, gains, spins) = plan(&mut biped, drive, &states, contact, dt);

        for part in Part::ALL {
            let Some(target) = targets[part.index()] else {
                continue;
            };
            let Ok(mut joint) = joints.get_mut(biped.part(part)) else {
                continue;
            };
            let group = if part.is_leg() {
                drive.legs
            } else if part.is_arm() {
                drive.arms
            } else if matches!(part, Part::Neck | Part::Head) {
                drive.neck
            } else {
                1.0
            };
            let reaching = match part {
                Part::LeftUpperArm | Part::LeftForearm => drive.hands[0],
                Part::RightUpperArm | Part::RightForearm => drive.hands[1],
                _ => None,
            };
            let reach = reaching.map_or(1.0, |hand| 1.0.lerp(hand.strength, hand.weight.clamp(0.0, 1.0)));
            let tone = if part.is_leg() { 1.0 } else { drive.tone };
            let strength = drive.strength * group * strengths[part.index()] * reach * gains[part.index()];
            let (stiffness, damping, torque) = muscles(part);
            let settling = if part.is_leg() { 1.0 } else { UPPER_BODY_DAMPING };
            let heaviness = settling * (1.0 + LANGUOR_DAMPING * drive.languor.clamp(0.0, MAX_LANGUOR));
            let muscles = (stiffness, damping * heaviness, torque * drive.force.clamp(0.0, 1.0));
            set_muscles(&mut joint, part, target, spins[part.index()], muscles, strength, tone);
        }
    }
    Ok(())
}

/// Works out where the biped should step and the pose it should pull itself towards, as the
/// target rotation of each part's joint relative to its parent, in `Part` order.
fn plan(
    biped: &mut Biped,
    drive: &Drive,
    states: &[PartState; PART_COUNT],
    contact: [bool; 2],
    dt: f32,
) -> ([Option<Quat>; PART_COUNT], [f32; PART_COUNT], [Vec3; PART_COUNT]) {
    let state = |part: Part| states[part.index()];
    let pelvis = state(Part::Guts);

    let (mut mass, mut com, mut momentum) = (0.0, Vec3::ZERO, Vec3::ZERO);
    for part in Part::ALL {
        let state = state(part);
        mass += part.mass();
        com += state.position * part.mass();
        momentum += state.velocity * part.mass();
    }
    let (com, velocity) = (com / mass, momentum / mass);
    // Its legs hold it up from how it is really moving; but languid, it senses that late for
    // deciding what to do about it (stepping, where to, throwing its arms out), and reacts late.
    let (true_com, true_velocity) = (com, velocity.with_y(0.0));
    let languor = drive.languor.clamp(0.0, MAX_LANGUOR);
    biped.fidget_time += dt * (1.0 - LANGUOR_SLOW_FIDGET * languor);
    let (com, velocity) = {
        let lag = LANGUOR_LAG * languor;
        let follow = if lag > 0.0 { 1.0 - (-dt / lag).exp() } else { 1.0 };
        let (sensed_com, sensed_velocity) = biped.sensed.get_or_insert((com, velocity));
        *sensed_com += (com - *sensed_com) * follow;
        *sensed_velocity += (velocity - *sensed_velocity) * follow;
        (*sensed_com, *sensed_velocity)
    };
    let flat_velocity = velocity.with_y(0.0);

    let soles = FEET.map(|foot| state(foot).point(sole_offset(foot)));
    if contact.iter().any(|&down| down) {
        let lowest = (0..2).filter(|&i| contact[i]).map(|i| soles[i].y).fold(f32::INFINITY, f32::min);
        biped.ground = lowest;
    }
    let ground = biped.ground;

    let pelvis_up = pelvis.rotation * Vec3::Y;
    let kneel = drive.kneel.clamp(0.0, 1.0);
    biped.fallen = pelvis_up.y < FALLEN_TILT || com.y - ground < FALLEN_HEIGHT.lerp(KNEELING_FALLEN_HEIGHT, kneel);
    let balancing = drive.balance * if biped.fallen { 0.0 } else { 1.0 };
    let steps = drive.steps;

    // It turns towards where it wants to face, and gets up to the speed it wants, gradually.
    // Kneeling, it can't turn, and faces whichever way it ended up.
    if kneel > 0.0 {
        biped.heading = None;
    } else {
        biped.knelt = false;
    }
    let heading = *biped.heading.get_or_insert(yaw(pelvis.rotation));
    let turn = angle_between(heading, drive.heading.unwrap_or(heading));
    let heading = heading + turn.clamp(-MAX_TURN_RATE * dt, MAX_TURN_RATE * dt);
    biped.heading = Some(heading);
    let wanted_pace = if steps { drive.velocity.with_y(0.0) } else { Vec3::ZERO };
    biped.pace += (wanted_pace - biped.pace).clamp_length_max(WALK_ACCELERATION * dt);
    let pace = biped.pace;
    let facing = yaw_rotation(heading);
    let (forward, right) = (facing * Vec3::Z, facing * Vec3::X);

    // The capture point, on the floor.
    let omega = (GRAVITY / (com.y - ground).max(0.3)).sqrt();
    // Sloppy, it misjudges where its weight is going, how hard to correct for it, and how far
    // off balance it can let itself get before stepping.
    let sloppy = drive.sloppiness.clamp(0.0, 1.0);
    let noises: [f32; SLOPPY_CHANNELS] = std::array::from_fn(|channel| biped.sloppy(channel) * sloppy);
    let noise = |channel: usize| noises[channel];
    let misjudged = Vec3::new(noise(0), 0.0, noise(1)) * SLOPPY_CAPTURE;
    let capture = (com + flat_velocity / omega).with_y(ground) + misjudged;
    let gain = |channel: usize| (1.0 + noise(channel) * SLOPPY_GAIN).max(0.2);
    let margin = SUPPORT_MARGIN * gain(2);
    let walking = pace.length() > 0.05 && balancing > 0.5 && steps;
    let between = (soles[0] + soles[1]) / 2.0;
    let wanted = between.with_y(ground) + drive.sway;
    let off_balance = capture - wanted;
    // How far the capture point is beyond what the feet on the floor can hold it over, as a
    // fraction of `SUPPORT_MARGIN`: past 1, only a step will catch it.
    let planted: Vec<Vec3> = (0..2).filter(|&i| contact[i]).map(|i| soles[i].with_y(ground)).collect();
    let beyond = match planted.as_slice() {
        [a, b] => {
            let along = b - *a;
            let t = ((capture - *a).dot(along) / along.length_squared().max(1e-6)).clamp(0.0, 1.0);
            capture - (*a + along * t)
        }
        [a] => capture - *a,
        _ => Vec3::ZERO,
    } - drive.sway;
    let outside = (beyond.dot(forward) / margin.x).hypot(beyond.dot(right) / margin.y);
    biped.off_balance = outside;
    let needs_step = outside > 1.0;

    // Shifting its weight, it steps once its weight is over the other foot. Knocked off balance
    // meanwhile, it gives up on that and steps to catch itself instead.
    if let Gait::Shifting { foot, time } = &mut biped.gait {
        *time += dt;
        let (foot, time) = (*foot, *time);
        let stance = soles[1 - foot].with_y(ground);
        let over = time > SHIFT_EASE
            && (capture - stance).dot(right).abs() < SHIFT_DONE
            && flat_velocity.dot(right).abs() < SHIFT_SETTLED;
        if needs_step || !steps || biped.fallen {
            biped.gait = Gait::Standing;
        } else if over || time > SHIFT_TIME {
            biped.gait = Gait::Stepping(Step {
                foot,
                from: soles[foot],
                to: soles[foot],
                yaw: heading,
                progress: 0.0,
                duration: STEP_DURATION * TIDY_SLOWER * drive.step_time.max(0.25),
                lift: STEP_LIFT * 0.7,
                tidy: true,
            });
        }
    }

    // Carry on any step under way; it lands once it touches down late enough in its swing.
    if let Gait::Stepping(step) = &mut biped.gait {
        step.progress += dt / step.duration;
        let near = soles[step.foot].with_y(0.0).distance(step.to.with_y(0.0)) < STEP_LAND_NEAR;
        let landed = step.progress >= STEP_LAND_BY
            || (contact[step.foot] && (step.progress >= STEP_LAND_ANYWHERE || (step.progress >= STEP_LAND_FROM && near)));
        if landed || !steps || biped.fallen {
            biped.last_step = step.foot;
            biped.rest = LANGUOR_STEP_REST * languor;
            biped.gait = Gait::Standing;
        }
    }
    // Then decide whether to take another.
    // Languid, it rests a while after each step before it can bring itself to take another.
    biped.rest = (biped.rest - dt).max(0.0);
    if matches!(biped.gait, Gait::Standing) && steps && biped.rest <= 0.0 && !biped.fallen && contact.iter().any(|&down| down) {
        let urgency = (outside - 1.0).clamp(0.0, 2.0);
        let duration = |walk: bool| if walk { WALK_STEP_DURATION } else { STEP_DURATION } * drive.step_time.max(0.25);
        let start = |foot: usize, tidy: bool, walk: bool| Step {
            foot,
            from: soles[foot],
            to: soles[foot],
            yaw: heading,
            progress: 0.0,
            // The further off balance, the quicker it gets its foot down.
            duration: duration(walk) / (1.0 + urgency * STEP_URGENCY) * (1.0 + noise(3) * SLOPPY_STEP_TIME).max(0.3),
            lift: if walk { WALK_STEP_LIFT } else { STEP_LIFT },
            tidy,
        };
        if walking {
            biped.gait = Gait::Stepping(start(1 - biped.last_step, false, true));
        } else if needs_step {
            // Off to one side, only the foot on that side can step out without crossing the
            // other; once it has, the other follows it in, and so on, shuffling sideways.
            let lateral = off_balance.dot(right);
            let outer = if lateral > 0.0 { 1 } else { 0 };
            let spread = (soles[1] - soles[0]).dot(right);
            let foot = if lateral.abs() > SUPPORT_MARGIN.y {
                if biped.last_step == outer && spread > STANCE_WIDTH * 1.4 { 1 - outer } else { outer }
            } else if soles[0].distance(capture) > soles[1].distance(capture) {
                0
            } else {
                1
            };
            biped.gait = Gait::Stepping(start(foot, false, false));
        } else if flat_velocity.length() < TIDY_SPEED && contact.iter().all(|&down| down) {
            // Settled, it brings its feet back into its stance, under it and facing its way.
            let untidy = |foot: usize| {
                let other = 1 - foot;
                let belongs = soles[other] + right * (SIDE_SIGNS[foot] - SIDE_SIGNS[other]) / 2.0 * STANCE_WIDTH;
                let turned = angle_between(heading, yaw(state(FEET[foot]).rotation));
                soles[foot].with_y(0.0).distance(belongs.with_y(0.0)).max(turned.abs() * TIDY_DISTANCE / TIDY_TURN)
            };
            let foot = if untidy(0) > untidy(1) { 0 } else { 1 };
            if untidy(foot) > TIDY_DISTANCE {
                biped.gait = Gait::Shifting { foot, time: 0.0 };
            }
        }
    }

    // Where the stepping foot is aiming: where the capture point will be when it lands, short
    // of it by as much as keeps it walking at the speed it wants, out to its side.
    let mut swing = None;
    if let Gait::Stepping(step) = &mut biped.gait {
        let foot = step.foot;
        let stance = soles[1 - foot];
        let side = right * SIDE_SIGNS[foot];
        let hip = pelvis.point(joint_offsets(THIGHS[foot]).1);
        let target = if step.tidy {
            stance + side * STANCE_WIDTH
        } else {
            let left = (1.0 - step.progress).max(0.0) * step.duration;
            let growth = (omega * left).exp().min(3.0);
            let landing = stance.with_y(ground) + (capture - stance.with_y(ground)) * growth;
            let walk_period = WALK_STEP_DURATION * drive.step_time.max(0.25);
            // Walking steadily on, at the speed it is going, up to the speed it wants: falling on
            // past the next foot as far as it fell past this one.
            let going = if walking {
                flat_velocity.project_onto(pace.normalize_or_zero()).clamp_length_max(pace.length())
            } else {
                Vec3::ZERO
            };
            let short = going * walk_period / ((omega * walk_period).exp() - 1.0);
            landing - short + side * STANCE_WIDTH / 2.0
        };
        // Never across the other foot, and no further than it can reach.
        let mut target = target.with_y(ground);
        let across = (MIN_STEP_WIDTH - (target - stance).dot(side)).max(0.0);
        target += side * across;
        let beneath = hip.with_y(ground);
        let target = beneath + (target - beneath).clamp_length_max(MAX_STEP * drive.reach.max(0.1));
        // Sloppy, it puts its foot down not quite where it means to.
        step.to = target + Vec3::new(noise(4), 0.0, noise(5)) * SLOPPY_FOOTING;
        step.yaw = heading;
        // The foot gets over where it is going early, then comes down onto it.
        let progress = step.progress;
        let along = smoothstep(progress / SWING_REACH_BY);
        let lift = step.lift * (PI * (progress / SWING_REACH_BY).min(1.0)).sin().max(0.0).powf(0.7);
        // Past the end of its swing without landing, it reaches down for the floor.
        let reach_down = if progress > 1.0 { 0.05 } else { 0.0 };
        let sole = step.from.lerp(step.to, along).with_y(ground + lift - reach_down);
        // Turning the foot round to face the way it is stepping, the short way round.
        let from_yaw = yaw(state(FEET[foot]).rotation);
        let foot_yaw = yaw_rotation(from_yaw + angle_between(from_yaw, step.yaw) * along);
        swing = Some((foot, sole, foot_yaw));
    }

    // Where the pelvis should be. Standing, it pulls its weight back over its feet; stepping, it
    // just brakes, leaving the step to catch it; walking, it presses on.
    let shift = if walking {
        (pace - true_velocity) * WALK_PUSH_GAIN
    } else if let Gait::Stepping(Step { tidy: true, foot, .. }) = biped.gait {
        // Stepping unhurried, it keeps its weight over the stance foot.
        let error = (soles[1 - foot].with_y(ground) - true_com).with_y(0.0);
        error * BALANCE_POSITION_GAIN * gain(6) + biped.lean_in - true_velocity * BALANCE_VELOCITY_GAIN * gain(7)
    } else if swing.is_some() {
        biped.lean_in *= (-dt / LEAN_IN_FADE).exp();
        -true_velocity * STEP_BRAKE_GAIN
    } else {
        // Shifting its weight, it eases it over onto the stance foot.
        let wanted = match biped.gait {
            Gait::Shifting { foot, time } => wanted.lerp(soles[1 - foot].with_y(ground), smoothstep(time / SHIFT_EASE)),
            _ => wanted,
        };
        let error = (wanted - true_com).with_y(0.0);
        biped.lean_in = (biped.lean_in + error * BALANCE_INTEGRAL_GAIN * dt).clamp_length_max(BALANCE_MAX_INTEGRAL);
        error * BALANCE_POSITION_GAIN * gain(6) + biped.lean_in - true_velocity * BALANCE_VELOCITY_GAIN * gain(7)
    };
    let shift = shift.clamp_length_max(BALANCE_MAX_SHIFT) * balancing;
    let walk_lean = Vec3::Y.cross(pace) * WALK_LEAN * if walking { 1.0 } else { 0.0 };
    let pelvis_rotation = Quat::from_scaled_axis(drive.lean + walk_lean) * facing;
    let hip_height = ANKLE_HEIGHT + (THIGH_LENGTH + SHIN_LENGTH) * STANCE_EXTENSION - drive.crouch;
    let hip_offset = joint_offsets(Part::LeftThigh).1;
    let pelvis_position = (pelvis.position + shift).with_y(ground + hip_height - hip_offset.y);

    let mut targets: [Option<Quat>; PART_COUNT] = [None; PART_COUNT];
    let mut gains = [1.0; PART_COUNT];
    // How fast each joint should be turning, as for `set_muscles`.
    let mut spins = [Vec3::ZERO; PART_COUNT];
    let relative = |parent: Quat, child: Quat| parent.inverse() * child;

    // Legs.
    for foot in 0..2 {
        let (thigh, shin, foot_part) = (THIGHS[foot], SHINS[foot], FEET[foot]);
        let ankle_local = joint_offsets(foot_part).0;
        let ankle_from_sole = ankle_local - sole_offset(foot_part);
        let foot_state = state(foot_part);
        let foot_yaw = yaw_rotation(yaw(foot_state.rotation));
        match swing {
            Some((swinging, sole, foot_rotation)) if swinging == foot => {
                // Swinging: from where its hip is, reaching for where the foot is going.
                let hip = pelvis.point(joint_offsets(thigh).1);
                let ankle = sole + foot_rotation * ankle_from_sole;
                let (thigh_rotation, shin_rotation) =
                    two_bone(hip, ankle, THIGH_LENGTH, SHIN_LENGTH, foot_rotation * Vec3::Z, true);
                targets[thigh.index()] = Some(relative(pelvis.rotation, thigh_rotation));
                targets[shin.index()] = Some(relative(thigh_rotation, shin_rotation));
                targets[foot_part.index()] = Some(relative(state(shin).rotation, foot_rotation));
                // Carrying only the leg's own weight, not the body's.
                for part in [thigh, shin, foot_part] {
                    gains[part.index()] = SWING_STRENGTH;
                }
            }
            _ => {
                // Standing: from where its hip should be, down to where the foot is planted.
                let hip = pelvis_position + pelvis_rotation * joint_offsets(thigh).1;
                let ankle = foot_state.point(ankle_local);
                let (thigh_rotation, shin_rotation) =
                    two_bone(hip, ankle, THIGH_LENGTH, SHIN_LENGTH, foot_yaw * Vec3::Z, true);
                // The hip holds the pelvis upright in the world against the leg as it is,
                // pushing against the floor, as hard as it is trying to balance.
                let upright = relative(pelvis_rotation, state(thigh).rotation);
                let posed = relative(pelvis_rotation, thigh_rotation);
                // Standing on one foot mid-step, that is the one to push against, whether or not
                // it is touching just now.
                let supporting = contact[foot] || swing.is_some();
                let hip_target = if supporting { posed.slerp(upright, balancing) } else { posed };
                targets[thigh.index()] = Some(hip_target);
                // Its damping should steady the pelvis in the world, not against the leg, which
                // turns as the body passes over the foot: so the hip should turn as fast as the
                // leg does, the pelvis not at all.
                if supporting {
                    let still = pelvis.rotation.inverse() * state(thigh).spin;
                    spins[thigh.index()] = still * balancing;
                }
                targets[shin.index()] = Some(relative(thigh_rotation, shin_rotation));
                targets[foot_part.index()] = Some(relative(shin_rotation, foot_yaw));
            }
        }
    }

    // Fallen over, there is nothing to stand on: its legs give up reaching for the floor, and
    // just lie out nearly straight, slackly, however it landed.
    if biped.fallen {
        for foot in 0..2 {
            targets[THIGHS[foot].index()] = Some(Quat::IDENTITY);
            targets[SHINS[foot].index()] = Some(Quat::from_rotation_x(FALLEN_KNEE_BEND));
            targets[FEET[foot].index()] = Some(Quat::IDENTITY);
            for part in [THIGHS[foot], SHINS[foot], FEET[foot]] {
                gains[part.index()] = FALLEN_LEG_STRENGTH;
                spins[part.index()] = Vec3::ZERO;
            }
        }
    }

    // Kneeling, its legs fold under it, whether or not it has fallen over, the ankles slack so the
    // feet don't hold it up, letting it drop onto its knees. Once its shins lie along the floor,
    // the hips pitch the pelvis upright in the world against the thighs as they are, resting on
    // the knees, as hard as it is trying to balance; otherwise they hold the thighs straight
    // under it, so the shins fold out behind.
    if kneel > 0.0 {
        let hold = drive.balance * if pelvis_up.y < FALLEN_TILT { 0.0 } else { 1.0 };
        // Balancing on its knees, it sits further back on its heels as its weight gets ahead of
        // them, or rises off them as it gets too far behind.
        let knees = SHINS.map(|shin| state(shin).point(joint_offsets(shin).0));
        let ahead = (true_com - (knees[0] + knees[1]) / 2.0).dot(forward) + KNEEL_SIT_BACK;
        let fold = (ahead * KNEEL_BALANCE_GAIN + true_velocity.dot(forward) * KNEEL_BALANCE_DAMPING)
            .clamp(-KNEEL_MAX_FOLD, KNEEL_MAX_FOLD);
        for foot in 0..2 {
            let (thigh, shin, foot_part) = (THIGHS[foot], SHINS[foot], FEET[foot]);
            let lying = if biped.knelt { 1.0 } else { smoothstep((KNEEL_SHIN_DOWN - (state(shin).rotation * Vec3::Y).y.abs()) / KNEEL_SHIN_DOWN) };
            if lying > 0.9 {
                biped.knelt = true;
            }
            let spread = SIDE_SIGNS[foot] * KNEEL_SPREAD;
            let upright = axis_angles(relative(pelvis_rotation, state(thigh).rotation)).x;
            let resting = from_axis_angles(Vec3::new(KNEEL_HIP.lerp(upright, hold), 0.0, spread));
            let pitched = Quat::from_scaled_axis(Vec3::Y.cross(forward) * KNEEL_DROP_LEAN) * pelvis_rotation;
            let pelvis_dropping = pelvis.rotation.slerp(pitched, KNEEL_DROP_UPRIGHT * hold);
            let dropping = relative(pelvis_dropping, facing * from_axis_angles(Vec3::new(KNEEL_DROP_THIGH, 0.0, spread)));
            let hip = dropping.slerp(resting, lying);
            // Dropping, its knees only fold square, swinging its shins back flat as its knees come
            // down beneath its hips; down on them, they fold right up as it sits back.
            let knee = Quat::from_rotation_x(KNEEL_DROP_KNEE.lerp(KNEEL_KNEE + fold * hold, lying));
            let pose = [(thigh, hip), (shin, knee), (foot_part, Quat::from_rotation_x(KNEEL_ANKLE))];
            for (part, kneeling) in pose {
                let index = part.index();
                targets[index] = Some(targets[index].unwrap_or(Quat::IDENTITY).slerp(kneeling, kneel));
                let strength = if part == foot_part { KNEEL_ANKLE_STRENGTH } else { KNEEL_STRENGTH };
                gains[index] = gains[index].lerp(strength, kneel);
                spins[index] *= 1.0 - kneel;
            }
        }
    }

    // Upper body: the chest holds its posture on the pelvis, the head steadies itself.
    targets[Part::Chest.index()] = Some(from_axis_angles(Vec3::new(drive.slump, drive.twist, 0.0)));
    targets[Part::Neck.index()] = Some(from_axis_angles(Vec3::X * drive.droop * 0.4));
    let head_posed = from_axis_angles(Vec3::X * drive.droop * 0.6);
    let head_steady = relative(state(Part::Neck).rotation, facing * Quat::from_rotation_x(drive.droop));
    targets[Part::Head.index()] = Some(head_posed.slerp(head_steady, HEAD_STEADYING * balancing));

    // Arms: hanging, swinging against the legs, thrown out to catch its balance, or reaching for
    // somewhere in particular.
    let chest = state(Part::Chest);
    // Walking, its weight is always ahead of its feet; that is no reason to throw its arms out.
    let reflex = drive.arm_reflex * balancing * if walking { WALK_ARM_REFLEX } else { 1.0 };
    let aside = off_balance.dot(right);
    let ahead = off_balance.dot(forward);
    for side in 0..2 {
        let sign = SIDE_SIGNS[side];
        let (upper, lower) = (UPPER_ARMS[side], FOREARMS[side]);
        let leg_swing = axis_angles(relative(pelvis.rotation, state(THIGHS[side]).rotation)).x;
        let out = ARM_HANG + (ARM_SPREAD_PER_METRE * aside.abs()).min(ARM_MAX_SPREAD) * reflex;
        let catch = (ARM_SWING_PER_METRE * ahead).clamp(-ARM_MAX_SWING, ARM_MAX_SWING) * reflex;
        let swing = -ARM_WALK_SWING * leg_swing * if walking { 1.0 } else { 0.3 } + catch;
        let mut upper_target = from_axis_angles(Vec3::new(swing, 0.0, sign * out));
        let brace = ELBOW_BEND + ELBOW_BRACE * (off_balance.length() * 4.0).min(1.0) * reflex;
        let mut lower_target = from_axis_angles(Vec3::X * -brace);
        if let Some(hand) = drive.hands[side] {
            let shoulder = chest.point(joint_offsets(upper).1);
            let to_hand = chest.rotation.inverse() * (hand.point - shoulder);
            let (upper_rotation, lower_rotation) =
                two_bone(Vec3::ZERO, to_hand, UPPER_ARM_LENGTH, FOREARM_LENGTH, hand.elbow, false);
            let weight = hand.weight.clamp(0.0, 1.0);
            upper_target = upper_target.slerp(upper_rotation, weight);
            lower_target = lower_target.slerp(relative(upper_rotation, lower_rotation), weight);
        }
        targets[upper.index()] = Some(upper_target);
        targets[lower.index()] = Some(lower_target);
    }

    // Curling up over all that, and wobbling about it. Lying on its front, drawing its knees up
    // would only lift its feet into the air, so its legs stay out.
    let prone = biped.fallen && (state(Part::Chest).rotation * Vec3::Z).y < -0.5;
    for (part, angles) in CURL_POSE {
        if prone && part.is_leg() {
            continue;
        }
        if let Some(target) = &mut targets[part.index()] {
            *target = target.slerp(from_axis_angles(angles), drive.curl.clamp(0.0, 1.0));
        }
    }
    // Drawing its knees up, whatever else its legs were doing, firmly enough to move them even
    // lying slack on the floor.
    for foot in 0..2 {
        let tuck = drive.tuck[foot].clamp(0.0, 1.0);
        if tuck <= 0.0 {
            continue;
        }
        for (part, angles) in [(THIGHS[foot], TUCK_HIP), (SHINS[foot], TUCK_KNEE), (FEET[foot], Vec3::ZERO)] {
            let index = part.index();
            let target = targets[index].unwrap_or(Quat::IDENTITY);
            targets[index] = Some(target.slerp(from_axis_angles(angles), tuck));
            gains[index] = gains[index].lerp(TUCK_STRENGTH, tuck);
            spins[index] *= 1.0 - tuck;
        }
    }
    let writhe = drive.writhe.clamp(0.0, 1.0);
    let convulse = drive.convulse.clamp(0.0, 1.0);
    for part in Part::ALL {
        let index = part.index();
        if let Some(target) = &mut targets[index] {
            // Not the legs while they hold it up, which would shuffle its feet about.
            let wobble = if part.is_leg() { 0.2 } else { 1.0 };
            *target *= from_axis_angles(biped.wobble(part) * WOBBLE * (drive.wobble + sloppy * SLOPPY_WOBBLE) * wobble);
            // Writhing and convulsing, all over, its legs too, however slack they lay.
            if writhe > 0.0 || convulse > 0.0 {
                let wiggle = |time: f32| {
                    biped.noise(part, WRITHE_RATES, time) * WRITHE_ANGLE * writhe
                        + biped.noise(part, CONVULSE_RATES, time) * CONVULSE_ANGLE * convulse
                };
                *target *= from_axis_angles(wiggle(biped.time));
                // Turning as fast as the wiggling does, so its muscles' damping follows it
                // rather than smothering it.
                spins[index] += (wiggle(biped.time + WIGGLE_DT) - wiggle(biped.time)) / WIGGLE_DT;
                if part.is_leg() {
                    gains[index] = gains[index].max(WRITHE_LEG_STRENGTH * writhe.max(convulse));
                }
            }
        }
    }
    // Languid, its muscles only slowly come round to what it means to do.
    // Not the legs it stands on, which have to keep holding it up as best they can.
    let ease = LANGUOR_EASE * languor;
    let follow = if ease > 0.0 { 1.0 - (-dt / ease).exp() } else { 1.0 };
    let swinging = swing.map(|(foot, ..)| foot);
    // Easing gently in after a handoff.
    let handing_over = biped.handoff > 0.0;
    biped.handoff = (biped.handoff - dt).max(0.0);
    let gently = |ease: f32| 1.0 - (-dt / ease).exp();
    for (index, (target, eased)) in targets.iter_mut().zip(biped.eased.iter_mut()).enumerate() {
        let Some(target) = target else {
            *eased = None;
            continue;
        };
        let part = Part::from_index(index);
        let standing_on = (0..2).any(|foot| Some(foot) != swinging && [THIGHS[foot], SHINS[foot], FEET[foot]].contains(&part));
        let follow = if standing_on { 1.0 } else { follow };
        let follow = if handing_over {
            follow.min(gently(if standing_on { HANDOFF_EASE.1 } else { HANDOFF_EASE.0 }))
        } else {
            follow
        };
        let now = eased.map_or(*target, |eased| eased.slerp(*target, follow));
        *eased = Some(now);
        *target = now;
    }
    (targets, gains, spins)
}

/// Sets a joint's motors to pull its part towards `target`, its rotation relative to the part
/// it hangs from, turning at `spin` (its angular velocity relative to that part, in that part's
/// space), with the given muscles at `strength` (from 0, limp, to 1), and their stiffness
/// further scaled by `tone`.
fn set_muscles(
    joint: &mut ImpulseJoint,
    part: Part,
    target: Quat,
    spin: Vec3,
    (stiffness, damping, torque): (f32, f32, f32),
    strength: f32,
    tone: f32,
) {
    if strength <= 0.0 {
        relax(joint);
        return;
    }
    let body_joint = part.joint().expect("names a jointed part");
    let angles = body_joint.clamp(axis_angles(target), JOINT_LIMIT_MARGIN);
    // Damping weakens only with the square root of the strength, keeping pace with how fast a
    // weaker spring swings: weakened in proportion, the joint would be ever springier.
    let (stiffness, damping, torque) = (stiffness * strength * tone, damping * strength.sqrt(), torque * strength);
    match (&mut joint.data, body_joint.kind) {
        (TypedJoint::SphericalJoint(ball), JointKind::Ball(_)) => {
            for (axis, angle) in [(JointAxis::AngX, angles.x), (JointAxis::AngY, angles.y), (JointAxis::AngZ, angles.z)] {
                let spin = match axis {
                    JointAxis::AngX => spin.x,
                    JointAxis::AngY => spin.y,
                    _ => spin.z,
                };
                ball.set_motor(axis, angle, spin, stiffness, damping).set_motor_max_force(axis, torque);
            }
        }
        (TypedJoint::RevoluteJoint(hinge), JointKind::Hinge(_)) => {
            hinge.set_motor(angles.x, spin.x, stiffness, damping).set_motor_max_force(torque);
        }
        _ => {}
    }
}

/// Lets a joint's motors go. A motor with no stiffness or damping isn't slack but rigid: Rapier
/// then holds the joint still with unlimited force. Limp muscles need no force at all.
fn relax(joint: &mut ImpulseJoint) {
    match &mut joint.data {
        TypedJoint::SphericalJoint(ball) => {
            for axis in [JointAxis::AngX, JointAxis::AngY, JointAxis::AngZ] {
                ball.set_motor_position(axis, 0.0, 0.0, 0.0).set_motor_max_force(axis, 0.0);
            }
        }
        TypedJoint::RevoluteJoint(hinge) => {
            hinge.set_motor_position(0.0, 0.0, 0.0).set_motor_max_force(0.0);
        }
        _ => {}
    }
}
