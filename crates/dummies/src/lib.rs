//! Enemies' bodies, built from boxes: unarmored, in a light vest, or in heavy armor (chest, waist,
//! thighs, shoulders, and a helmet with a visor over the face, leaving the neck exposed). Every
//! box is a separate collider, so a shot finds exactly which body part or armor piece it hit.
//! Ported from pistol_game's dummies, without their blood, voices, health bars and damage numbers.
//!
//! Each dummy is animated while nothing calls for anything more (see `animation`): standing
//! idly, or walking and running where it is steered, flinching at light hits on top of that.
//! Knocked about hard enough at once or killed, it hands over to the physics: a self-balancing
//! biped (see `biped`), jointed rigid bodies kept on their feet by nothing but joint motors,
//! stepping, stumbling and struggling as its balance controller works out, moment to moment.
//! This module decides what it is trying to do, as its `Drive`, and the physics does the rest.
//! Once it has settled, it blends back into its animation, standing where it ended up.
//!
//! The game decides what a hit does and reports it as `Damage` messages; this module applies
//! them to the dummy's health. A light hit makes it flinch: the part hit is knocked about its
//! joint, and less so the joints above it, springing back more sluggishly the more that part has
//! been hurt. A heavy one (see `REACT_KNOCKBACK`) really shoves the body part it hits, and the
//! dummy reels and steps to catch itself, or stumbles and falls if it can't, getting back up a
//! little later where it lies; while the physics has it, its muscles go slack about each hit, so
//! the push swings it about. A headshot that kills drops it limp; any other kill leaves it
//! fighting to stay on its feet as its strength gives out, ever weaker, slower and sloppier:
//! stepping to catch itself, throwing its arms out, clutching at its wound or putting its hands
//! out as it falls, pitching forwards onto its knees for a while or toppling. Down on the floor,
//! it finds a second wind and struggles on there: trying to get up, crawling, writhing,
//! convulsing, or lying still, one after another, until it is spent. Further damage hurries that
//! along, and a headshot finishes it off. Each body part keeps track of the damage it has taken,
//! which weakens its own muscle. Once dead it stays dead: clearing the body away is up to the game.

mod animation;
mod damage;
mod ragdoll;
mod spawn;
mod struggle;

pub use spawn::spawn_dummy;

pub use animation::{Animation, GUN_GRIP, RUN_SPEED, Steer, wrap_angle};

use std::f32::consts::PI;

use bevy::prelude::*;
use biped::{BipedPlugin, BipedSystems};
use biped::body::PART_COUNT;
use game_core::combat::{BodyPartKind, Damage};
use game_core::math::smoothstep;

use damage::{apply_damage, bleed, flinch};
use ragdoll::{falter, knock_down, recover_ragdolls, settle, take_over};
use struggle::struggle;

/// Handed over to the physics, or starting to struggle, its muscles ease into what they are
/// to do over this long, in seconds.
const HANDOFF_EASE: f32 = 0.8;
const STRUGGLE_EASE: f32 = 1.0;

/// A living dummy is animated (see `animation`), only flinching at hits, until it has been
/// knocked about this hard at once, in newton-seconds of shots, which wear off with this time
/// constant, in seconds; then the physics takes over and it reacts with its whole body,
/// balancing, stepping and stumbling to catch itself...
const REACT_KNOCKBACK: f32 = 6.5;
const KNOCKBACK_FADE: f32 = 0.4;
/// ...or once it is so far off balance, as a fraction of what its feet can hold it over, that
/// it would otherwise fall. It carries on reacting for this long, in seconds, after the last
/// time it had to, and until any step it is taking is done.
const REACT_OFF_BALANCE: f32 = 1.8;
const REACT_TIME: f32 = 1.5;
/// A hit leaves the muscle holding the part hit slack for a moment, so the shot swings it about
/// its joint before it pulls back, and the muscles further up the body in turn a fraction as
/// much each. Slackness, from 0 to 1, per hit at full impact (see `Damage::impact`)...
const FLINCH_SLACK: f32 = 0.85;
const FLINCH_FALLOFF: f32 = 0.45;
/// ...recovering at this rate per second, slowed by bleeding (see `Bleeding::weakness`)...
const FLINCH_RECOVERY: f32 = 2.5;
/// ...but a part's muscle never goes slacker than this from flinching alone.
const MAX_FLINCH_SLACK: f32 = 0.9;
/// Damage to a body part, before or after the kill, that leaves its muscle useless. Its
/// strength falls in proportion until then...
pub(crate) const LIMB_CRIPPLE_DAMAGE: f32 = 50.0;
/// ...though while the dummy lives, its muscles keep at least this much, so it stays on its feet.
const MIN_LIVING_MUSCLE: f32 = 0.35;

/// After a kill other than a headshot, the dummy struggles for longer the less damage the
/// killing shot did: this long, in seconds, from a shot doing no damage...
const STRUGGLE_LONGEST: f32 = 12.0;
/// ...down to this long from one doing `STRUGGLE_SHORTEST_DAMAGE` or more.
const STRUGGLE_SHORTEST: f32 = 1.0;
const STRUGGLE_SHORTEST_DAMAGE: f32 = 50.0;
/// Each struggle lasts up to this fraction longer or shorter, at random.
const STRUGGLE_VARIANCE: f32 = 0.2;
/// Fraction of the struggle spent at full strength, before it fades to nothing, quickly at
/// first and then ever more slowly.
const STRUGGLE_HOLD_FRACTION: f32 = 0.3;
/// Past the hold, its strength fades with what is left of the struggle raised to this power:
/// steeply, as its muscles start out several times stronger than needed to hold it up, so a
/// gentler fade would leave it standing firm until nearly the end.
const STRUGGLE_FADE_POWER: f32 = 1.5;
/// Once it has fallen over, there's nothing left to stand on, and its legs slacken over this
/// long, in seconds, for good, down to this fraction of their strength, so they still stir
/// weakly on the floor.
const LEG_FALLEN_SLACKEN: f32 = 0.3;
const LEG_FALLEN_STRENGTH: f32 = 0.15;
/// How much its muscles' targets wobble about while struggling, as a fraction of the biped's
/// `WOBBLE`, so it looks less robotic.
const STRUGGLE_WOBBLE: f32 = 1.0;

/// Health a stack of bleeding saps per second.
const BLEED_PER_SECOND: f32 = 2.625;
/// Bleeding saps health in ticks this far apart, in seconds, each showing a damage number.
const BLEED_TICK: f32 = 0.5;
/// Each stack of bleeding slows recovering from hits: with two stacks, to 1 / (1 + 2 × this)
/// of the usual pace.
const BLEED_WEAKNESS_PER_STACK: f32 = 0.25;

/// A standing dummy still bleeding with less health than this falters: it recovers from hits
/// with half its tone, sags at the knees, slumps and hangs its head, and sways, stepping to catch
/// itself. It eases into this over this long, in seconds, and back out of it once the bleeding
/// stops over this long.
const FALTER_HEALTH: f32 = 20.0;
const FALTER_FADE_IN: f32 = 4.0;
const FALTER_FADE_OUT: f32 = 8.0;
/// How far a faltering dummy sags at the knees, in metres, and slumps its chest and hangs its
/// head forwards, in radians.
const FALTER_CROUCH: f32 = 0.07;
const FALTER_SLUMP: f32 = 0.3;
const FALTER_DROOP: f32 = 0.35;
/// How far off between its feet a faltering dummy lets its weight sway, in metres, and how fast,
/// in radians per second, the direction it sways in wanders about at random.
const FALTER_SWAY: f32 = 0.09;
const FALTER_WANDER: f32 = 2.5;

/// Bleeding out, a dummy carries on faltering rather than fighting: it starts at this fraction of
/// a struggle's strength, in the posture it was faltering in, then gives way smoothly over the
/// whole struggle, sagging towards the fully weakened posture, swaying as it did.
const BLEED_OUT_HOLD: f32 = 0.85;
/// Chance that a dummy bleeding out staggers about for a while before it gives way: it sways
/// further, so it keeps stepping to catch itself, its legs holding up, not curling up or
/// toppling, until this fraction of the way through the struggle, when it sinks as any other.
const STAGGER_CHANCE: f32 = 0.5;
const STAGGER_UNTIL: f32 = 0.6;
const STAGGER_SWAY: f32 = 0.16;
/// Chance that a struggling dummy, as it weakens, pitches forwards and drops onto its knees to
/// catch itself, slumped there a while before it goes all the way down. Otherwise its balance
/// gives way towards a random direction, leaning its pelvis more and more over the struggle, up
/// to this far, in radians, at its end, so it topples over.
const KNEEL_CHANCE: f32 = 0.5;
const TOPPLE_LEAN: f32 = 0.8;
/// It drops to its knees once it is this far through going down and pitching forwards: its
/// chest tipped this far forwards, in radians, or its weight heading forwards this fast, in
/// metres per second; or going over any which way, its chest tipped this far, or sinking this
/// fast; or anyway, once this far through, as its legs give way under it. Not once it has
/// fallen over, nor while its weight is heading backwards faster than this, in metres per second.
const KNEEL_FROM: f32 = 0.2;
const KNEEL_BACKING: f32 = 0.1;
/// Nor while going faster than this, in metres per second, which would carry it on over its
/// knees: about to kneel, it stops staggering off this far short of `KNEEL_FROM`, to slow down.
const KNEEL_MAX_SPEED: f32 = 0.4;
const KNEEL_SETTLE: f32 = 0.1;
const KNEEL_TIP: f32 = 0.2;
const KNEEL_LUNGE: f32 = 0.3;
const KNEEL_TILT: f32 = 0.3;
const KNEEL_SINKING: f32 = 0.2;
const KNEEL_ANYWAY: f32 = 0.65;
/// It folds its legs under it over this long, in seconds...
const KNEEL_DROP: f32 = 0.3;
/// ...stays on its knees for between this long and this long, in seconds, at random, but gives way
/// no later than this fraction of the way through the struggle, so it is down before its end...
const KNEEL_HOLD: (f32, f32) = (1.5, 4.0);
const KNEEL_UNTIL: f32 = 0.85;
/// ...then gives way over this long, its legs unfolding as it tips over, up to this far, in
/// radians, forwards and up to this far off to one side or the other, at random.
const KNEEL_GIVE_WAY: f32 = 0.8;
const KNEEL_TIP_OVER: f32 = 1.0;
const KNEEL_TIP_ASIDE: f32 = 0.8;
/// Its balance gives way the more the later it gets, as how far it is going down raised to this
/// power, so it has time to stagger about first.
const TOPPLE_LATENESS: f32 = 2.5;
/// Chance that a toppling dummy also twists its chest round on its guts, one way or the other,
/// working up to this far, in radians, at the struggle's end.
const TOPPLE_TWIST_CHANCE: f32 = 0.5;
const TOPPLE_TWIST: f32 = 0.34;
/// Down on the floor for this long, in seconds, a struggling dummy starts doing something about
/// it, one thing after another, picking each at random with these weights, which depend on how
/// much effort it has left in it (from 1, fresh, to 0, spent; see `ground_effort`): trying to get
/// up, crawling, writhing, curling up, convulsing, or just lying there.
const GROUND_SETTLE: f32 = 0.7;
/// It struggles on the floor for this long, in seconds, from a struggle that was to be shortest
/// to one that was to be longest, give or take `STRUGGLE_VARIANCE`, with a second wind: its
/// strength there starts at this and fades as what is left of that time raised to this power.
const GROUND_TIME: (f32, f32) = (2.5, 7.0);
const GROUND_STRENGTH: f32 = 0.8;
const GROUND_FADE_POWER: f32 = 0.8;
const GET_UP_WEIGHT: f32 = 3.0;
const CRAWL_WEIGHT: f32 = 2.0;
const WRITHE_WEIGHT: f32 = 1.5;
/// It doesn't curl up on the floor: it looked too stiff.
const CURL_WEIGHT: f32 = 0.0;
/// Bleeding out, it is less likely to try to get up.
const BLEEDING_CURL_WEIGHT: f32 = 0.0;
const BLEEDING_GET_UP_WEIGHT: f32 = 0.5;
/// Convulsing and lying there take these weights fresh, and these spent.
const CONVULSE_WEIGHT: (f32, f32) = (0.3, 1.5);
const LIE_WEIGHT: (f32, f32) = (0.6, 2.0);
/// How long each lasts, in seconds, at random between these.
const GET_UP_TIME: (f32, f32) = (3.0, 4.5);
const CRAWL_TIME: (f32, f32) = (2.5, 5.0);
const WRITHE_TIME: (f32, f32) = (2.0, 4.0);
const GROUND_CURL_TIME: (f32, f32) = (2.5, 5.0);
const CONVULSE_TIME: (f32, f32) = (0.8, 2.0);
const LIE_TIME: (f32, f32) = (1.0, 2.5);
/// Each eases in and out over this long, in seconds.
const GROUND_EASE: f32 = 0.4;
/// It counts as on its front once its chest faces at least this far down, as the cosine of the
/// angle from straight down.
const PRONE: f32 = 0.4;

/// Chance that a struggling dummy puts its hands out to catch itself as it falls.
const HAND_CATCH_CHANCE: f32 = 0.6;
/// It reaches for the floor once its chest tips this far from upright, in radians, fully once
/// it tips this much further...
const HAND_CATCH_TILT: (f32, f32) = (0.35, 0.5);
/// ...or once it drops this fast, in metres per second, fully once this much faster.
const HAND_CATCH_DROP: (f32, f32) = (0.3, 0.7);
/// Reaching for the floor eases in and out, with these time constants in seconds, and the way
/// it reaches turns with the first, so its arms don't flick about with every wobble.
const HAND_CATCH_EASE: (f32, f32) = (0.15, 0.5);
/// Where it reaches: this far beyond its shoulders the way it is falling, and this far out to
/// each side, in metres.
const HAND_REACH_AHEAD: f32 = 0.3;
const HAND_REACH_SPREAD: f32 = 0.1;
/// How much stronger its arms are while reaching to catch itself than when flailing, at full
/// strength. They weaken with the square of the struggle's strength, giving way sooner.
const HAND_CATCH_STRENGTH: f32 = 2.0;
/// Chance that a dummy, as it struggles, clutches at where it was hit by the shot that killed it,
/// or where it was last wounded if it bled to death, if that was in its torso or neck. It holds
/// the hand on that side over the front of the part hit, level with the wound and no further
/// across than the middle, so the arm never has to reach across or through the body. Putting
/// its hands out to catch itself takes over from this.
const CLUTCH_CHANCE: f32 = 0.5;
/// How long it takes to bring its hand there, in seconds, and how far in front of the part it
/// holds it, in metres, clear of the body even with the forearm's thickness.
const CLUTCH_TIME: f32 = 0.8;
const CLUTCH_OFFSET: f32 = 0.08;
/// How strong its arm is while clutching, as a fraction of usual: gentle, so it doesn't jerk the
/// arm in or grind it against the body.
const CLUTCH_STRENGTH: f32 = 0.7;
/// While still on its feet, a struggling dummy keeps trying to do something, changing its mind
/// every so often, after this long, in seconds, at random: staggering off somewhere, turning to
/// face somewhere else, or just holding on, with these chances (holding on the rest)...
const INTENT_TIME: (f32, f32) = (1.3, 3.3);
const STAGGER_OFF_CHANCE: f32 = 0.47;
const TURN_CHANCE: f32 = 0.24;
/// ...taking ever longer to make up its mind as the struggle goes on, up to this many times
/// longer again by its end.
const INTENT_SLOWING: f32 = 0.6;
/// Chance that it first staggers off the way the shot that killed it was going, as if driven
/// back by it, rather than deciding at random.
const DRIVEN_BACK_CHANCE: f32 = 0.6;
/// How fast it tries to stagger off, in metres per second, at full strength; weaker, it goes
/// slower, down to this fraction with no strength left. Each new way it heads is up to this far,
/// in radians, from where it was heading.
const STAGGER_OFF_SPEED: f32 = 0.6;
const STAGGER_OFF_WEAK: f32 = 0.4;
const STAGGER_OFF_TURN: f32 = 1.6;
/// Each time it changes its mind, there's this chance it makes for its killer (the player, here)
/// or an ally, rather than anywhere: its killer with this chance, an ally otherwise, the nearest
/// still standing within this far, in metres.
const SEEK_CHANCE: f32 = 0.3;
const SEEK_KILLER_CHANCE: f32 = 0.6;
const ALLY_RANGE: f32 = 8.0;

pub struct DummiesPlugin;

impl Plugin for DummiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Damage>()
            .add_plugins(BipedPlugin)
            // Ready for anything spawning dummies at startup.
            .add_systems(PreStartup, |mut commands: Commands| commands.init_resource::<DummyAssets>())
            // Damage first, so a dummy killed this frame is handed to the physics at once; then
            // what each dummy is trying to do, just before its balance acts on it. The physics
            // itself steps after all this, in `PostUpdate`.
            .add_systems(
                Update,
                (
                    (bleed, apply_damage.in_set(DamageSystems), recover_ragdolls).chain(),
                    (knock_down, take_over, falter, struggle, settle, animation::animate.in_set(AnimateSystems), flinch).chain(),
                )
                    .chain()
                    .before(BipedSystems),
            );
    }
}

/// Where damage is dealt to dummies: anything spawning one and hurting it at once does so before
/// this.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DamageSystems;

/// Where animated dummies are posed, each frame: steer them before this.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnimateSystems;

#[derive(Component, Debug)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

/// A dummy's open wounds. Each stack bleeds `BLEED_PER_SECOND` until it runs out.
#[derive(Component)]
pub struct Bleeding {
    /// Ticks of bleeding left on each stack (see `BLEED_TICK`).
    pub stacks: Vec<u32>,
    tick: Timer,
    /// The latest wound, as the body part and where on it, in the part's own space.
    pub(crate) last_wound: Option<(Entity, Vec3)>,
}

impl Default for Bleeding {
    fn default() -> Self {
        Self { stacks: Vec::new(), tick: Timer::from_seconds(BLEED_TICK, TimerMode::Repeating), last_wound: None }
    }
}

impl Bleeding {
    /// How much its bleeding slows down everything that depends on its strength: 1 when not
    /// bleeding, more with each stack.
    fn weakness(&self) -> f32 {
        1.0 + BLEED_WEAKNESS_PER_STACK * self.stacks.len() as f32
    }
}

/// A dummy's state: standing, knocked off its feet until it gets back up, or dead.
#[derive(Component)]
pub struct Ragdoll {
    /// Dead.
    down: bool,
    /// Alive, but knocked off its feet and lying there.
    knocked: bool,
    /// How long it has been fallen over while alive, before it counts as knocked down.
    fallen: f32,
    /// Time since the dummy was last hit while down or knocked down.
    idle: Timer,
    /// Which body part the shot that killed it struck, if a shot did.
    killed_in: Option<BodyPartKind>,
}

impl Ragdoll {
    /// Dead.
    pub fn dead(&self) -> bool {
        self.down
    }

    /// Alive, but knocked off its feet and lying there until it gets back up.
    pub fn knocked(&self) -> bool {
        self.knocked
    }

    /// Which body part the shot that killed it struck, if it is dead from a shot.
    pub fn killed_in(&self) -> Option<BodyPartKind> {
        self.killed_in.filter(|_| self.down)
    }
}

/// A dummy going over with this velocity, in metres per second, as its body is handed to the
/// physics: whatever it was carried along with, like the player's corpse with how fast they were
/// going. Taken off once handed over.
#[derive(Component)]
pub struct Momentum(pub Vec3);

/// A downed dummy fighting to keep its balance. Its muscles weaken until it goes limp, sooner
/// the more it is hurt, or are cut at once by a headshot.
#[derive(Component)]
pub struct Struggle {
    elapsed: f32,
    /// How long it lasts, if not hurried along or finished off.
    duration: f32,
    /// Finished off: the muscles are cut on the next physics step.
    finished: bool,
    /// Tending to curl up as it struggles.
    curl: bool,
    /// Putting its hands out to catch itself as it falls.
    catches: bool,
    /// How far it is reaching for the floor, from 0 to 1, and which way, horizontally.
    reach: f32,
    reach_towards: Vec3,
    /// Where it clutches at, if it does, as the body part and where on it, in the part's own
    /// space (see `CLUTCH_CHANCE`).
    clutch: Option<(Entity, Vec3)>,
    /// How much it was faltering when it died, from 0 to 1: it starts out sagging as much as it
    /// was.
    slump: f32,
    /// Bleeding out: weak, and swaying as it did while faltering.
    bleed_out: bool,
    /// Which way it is swaying while bleeding out, as an angle about the vertical.
    wander: f32,
    /// Bleeding out, staggering about for a while first (see `STAGGER_CHANCE`).
    stagger: bool,
    /// How much of their strength its legs keep, from 1 down to `LEG_FALLEN_STRENGTH` once it
    /// has fallen over.
    legs: f32,
    /// Which way its balance gives way as it weakens, as an angle about the vertical, if it
    /// topples rather than dropping to its knees (see `KNEEL_CHANCE`).
    topple: Option<f32>,
    /// How it is getting on with dropping to its knees, if it does.
    kneel: Kneel,
    /// How far it has dropped onto its knees, from 0 to 1 (see `Drive::kneel`).
    kneeling: f32,
    /// How far, and which way, it twists its chest round on its guts as it topples, at the
    /// struggle's end.
    twist: f32,
    /// What it is trying to do just now, and for how much longer, in seconds.
    intent: Intent,
    intent_left: f32,
    /// How long it has been down on the floor, in seconds, and what it is doing about it, if
    /// anything yet: since when (in seconds into the struggle) and until when.
    down_for: f32,
    ground: Option<(GroundAct, f32, f32)>,
    /// Down on the floor for good: since when, in seconds into the struggle, and for how long it
    /// struggles there (see `GROUND_TIME`).
    floor: Option<(f32, f32)>,
}

/// What a struggling dummy down on the floor is doing about it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum GroundAct {
    /// Trying to get up: on its back, trying to sit up; once on its front, since when, pushing
    /// itself up and drawing its knees under it.
    GetUp { front: Option<f32> },
    /// Crawling on its front; on its back, trying to sit up instead.
    Crawl,
    Writhe,
    Curl,
    Convulse,
    /// Just lying there.
    Lie,
}

/// What a struggling dummy down on the floor does with its body, over its struggle.
#[derive(Default)]
struct GroundDrive {
    /// How much it is doing, from 0 to 1, easing in and out.
    effort: f32,
    tuck: [f32; 2],
    writhe: f32,
    convulse: f32,
    curl: f32,
    twist: f32,
    /// How far its chest straightens out of its slump, as a fraction.
    straighten: f32,
}

/// A struggling dummy dropping to its knees as it pitches forwards (see `KNEEL_CHANCE`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kneel {
    /// It doesn't, toppling instead.
    Never,
    /// Not yet.
    Ready,
    /// On its knees since `since` (in seconds into the struggle), until `until`.
    Down { since: f32, until: f32 },
    /// Giving way since `since`, tipping over towards `towards`, as an angle about the vertical.
    GivingWay { since: f32, towards: f32 },
    /// Done with, down on the floor.
    Done,
}

/// What a struggling dummy on its feet is trying to do.
#[derive(Clone, Copy, Debug)]
enum Intent {
    /// Just stay on its feet.
    Hold,
    /// Stagger off the way given, as an angle about the vertical.
    Walk(f32),
    /// Turn round to face the way given.
    Turn(f32),
    /// Make for the given thing (its killer, or an ally): turning to face it first if it is off
    /// to the side or behind, then staggering towards it, and stopping once there.
    Seek(Entity),
}

impl Struggle {
    /// A struggle after being killed by a shot doing `damage`, travelling `shot` (zero if it bled
    /// to death).
    fn new(damage: f32, curl: bool, slump: f32, bleed_out: bool, wound: Option<(Entity, Vec3)>, wander: f32, shot: Vec3) -> Self {
        let severity = (damage / STRUGGLE_SHORTEST_DAMAGE).clamp(0.0, 1.0);
        let variance = 1.0 + STRUGGLE_VARIANCE * (2.0 * fastrand::f32() - 1.0);
        let kneels = fastrand::f32() < KNEEL_CHANCE;
        let topple = (!kneels).then(|| 2.0 * PI * fastrand::f32());
        let stagger = bleed_out && fastrand::f32() < STAGGER_CHANCE;
        Self {
            elapsed: 0.0,
            duration: STRUGGLE_LONGEST.lerp(STRUGGLE_SHORTEST, severity) * variance,
            finished: false,
            // Curling up would draw its knees up off the floor.
            curl: curl && !stagger && !kneels,
            stagger,
            catches: fastrand::f32() < HAND_CATCH_CHANCE,
            reach: 0.0,
            reach_towards: Vec3::ZERO,
            clutch: wound.filter(|_| fastrand::f32() < CLUTCH_CHANCE),
            slump,
            bleed_out,
            wander,
            legs: 1.0,
            topple,
            kneel: if kneels { Kneel::Ready } else { Kneel::Never },
            kneeling: 0.0,
            twist: if topple.is_some() && fastrand::f32() < TOPPLE_TWIST_CHANCE {
                if fastrand::bool() { TOPPLE_TWIST } else { -TOPPLE_TWIST }
            } else {
                0.0
            },
            // Driven back by the shot, or otherwise making up its mind shortly.
            intent: match shot.with_y(0.0).try_normalize() {
                Some(way) if fastrand::f32() < DRIVEN_BACK_CHANCE => Intent::Walk(way.x.atan2(way.z)),
                _ => Intent::Hold,
            },
            intent_left: INTENT_TIME.0.lerp(INTENT_TIME.1, fastrand::f32()),
            down_for: 0.0,
            ground: None,
            floor: None,
        }
    }

    /// Picks what to do next, down on the floor, with `effort` left in it (see `GROUND_SETTLE`),
    /// and for how long.
    fn pick_ground_act(&self, effort: f32) -> (GroundAct, f32) {
        let (get_up, curl) = if self.bleed_out {
            (BLEEDING_GET_UP_WEIGHT, BLEEDING_CURL_WEIGHT)
        } else {
            (GET_UP_WEIGHT, CURL_WEIGHT)
        };
        let choices = [
            (GroundAct::GetUp { front: None }, get_up * effort * effort, GET_UP_TIME),
            (GroundAct::Crawl, CRAWL_WEIGHT * effort, CRAWL_TIME),
            (GroundAct::Writhe, WRITHE_WEIGHT, WRITHE_TIME),
            (GroundAct::Curl, curl, GROUND_CURL_TIME),
            (GroundAct::Convulse, CONVULSE_WEIGHT.1.lerp(CONVULSE_WEIGHT.0, effort), CONVULSE_TIME),
            (GroundAct::Lie, LIE_WEIGHT.1.lerp(LIE_WEIGHT.0, effort), LIE_TIME),
        ];
        let total: f32 = choices.iter().map(|(_, weight, _)| weight).sum();
        let mut pick = fastrand::f32() * total;
        for (act, weight, (shortest, longest)) in choices {
            if pick < weight {
                return (act, shortest.lerp(longest, fastrand::f32()));
            }
            pick -= weight;
        }
        (GroundAct::Lie, LIE_TIME.0)
    }

    /// Plays its intent on by `dt`, changing its mind once it's had long enough, with `facing` the
    /// way it faces now, as an angle about the vertical. The further through the struggle, the
    /// longer it takes to make up its mind.
    fn intend(&mut self, dt: f32, facing: f32, killer: Option<Entity>, ally: Option<Entity>) {
        self.intent_left -= dt;
        if self.intent_left > 0.0 {
            return;
        }
        self.intent_left = INTENT_TIME.0.lerp(INTENT_TIME.1, fastrand::f32()) * (1.0 + INTENT_SLOWING * self.progress());
        let from = match self.intent {
            Intent::Walk(heading) | Intent::Turn(heading) => heading,
            Intent::Hold | Intent::Seek(_) => facing,
        };
        // Now and then it makes for its killer, or failing that an ally nearby, rather than just
        // anywhere.
        if fastrand::f32() < SEEK_CHANCE {
            let seek = if fastrand::f32() < SEEK_KILLER_CHANCE { killer.or(ally) } else { ally.or(killer) };
            if let Some(target) = seek {
                self.intent = Intent::Seek(target);
                return;
            }
        }
        let chance = fastrand::f32();
        let turned = from + STAGGER_OFF_TURN * (2.0 * fastrand::f32() - 1.0);
        self.intent = if chance < STAGGER_OFF_CHANCE {
            Intent::Walk(turned)
        } else if chance < STAGGER_OFF_CHANCE + TURN_CHANCE {
            Intent::Turn(facing + PI * (2.0 * fastrand::f32() - 1.0))
        } else {
            Intent::Hold
        };
    }

    /// How far through the struggle it is, from 0 to 1.
    fn progress(&self) -> f32 {
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }

    /// How far through its struggle it is, from 0 to 1.
    pub fn how_far(&self) -> f32 {
        self.progress()
    }

    /// Strength of its muscles, from 1 while holding on to 0 once spent. It eases out steeply,
    /// so it starts to buckle partway through and sinks from there. Bleeding out, there's no
    /// fight left: it starts weak and gives way smoothly.
    fn strength(&self) -> f32 {
        if self.finished {
            return 0.0;
        }
        // Down on the floor for good, it finds a second wind, fading over its time there.
        if let Some((start, length)) = self.floor {
            let remaining = (1.0 - (self.elapsed - start) / length).clamp(0.0, 1.0);
            return GROUND_STRENGTH * remaining.powf(GROUND_FADE_POWER);
        }
        if self.bleed_out {
            return BLEED_OUT_HOLD * (1.0 - smoothstep(self.progress()));
        }
        let hold = self.duration * STRUGGLE_HOLD_FRACTION;
        let remaining = (1.0 - (self.elapsed - hold).max(0.0) / (self.duration - hold)).clamp(0.0, 1.0);
        remaining.powf(STRUGGLE_FADE_POWER)
    }

    /// How far it has got towards going down, from 0 to 1: staggering, it only starts once it
    /// stops.
    fn going_down(&self) -> f32 {
        if self.stagger {
            ((self.progress() - STAGGER_UNTIL) / (1.0 - STAGGER_UNTIL)).max(0.0)
        } else {
            self.progress()
        }
    }

    fn staggering(&self) -> bool {
        self.stagger && self.floor.is_none() && self.progress() < STAGGER_UNTIL
    }
}

/// How hard a living dummy has been knocked about lately, and how much longer it reacts to it
/// with its whole body; see `REACT_KNOCKBACK`.
#[derive(Component, Default)]
struct Reactive {
    knockback: f32,
    left: f32,
}

/// How much a standing dummy is faltering from blood loss, from 0 (not at all) to 1; see
/// `FALTER_HEALTH`.
#[derive(Component, Default)]
struct Falter {
    level: f32,
    /// Which way it is swaying while faltering, as an angle about the vertical.
    wander: f32,
}

/// How slack a body part's muscle has gone from flinching at a hit, from 0 to 1.
#[derive(Component, Default)]
struct Flinch(f32);

/// Damage a body part has taken since the dummy last stood up, which weakens its muscle.
#[derive(Component, Default)]
struct LimbDamage(f32);

/// What a dummy wears over its body.
#[derive(Clone, Copy)]
pub enum Protection {
    None,
    Light,
    Heavy,
}

/// What an armor piece is made of, which sets its look.
#[derive(Clone, Copy)]
enum ArmorMaterial {
    Light,
    Heavy,
    Visor,
}

/// What dummies are drawn with, shared by all of them: a mesh for each body part and armor piece,
/// and the materials for skin, the suit and each kind of armor. Every mesh carries tangents, which
/// ray tracing needs, and every material is matte, as glossy ones glint noisily under it.
#[derive(Resource)]
pub struct DummyAssets {
    pub(crate) parts: [Handle<Mesh>; PART_COUNT],
    pub(crate) light_armor: Vec<Handle<Mesh>>,
    pub(crate) heavy_armor: Vec<Handle<Mesh>>,
    /// Bare skin, on the head and neck; a suit everywhere else, in a colour for each protection.
    pub(crate) skin: Handle<StandardMaterial>,
    pub(crate) suit: Handle<StandardMaterial>,
    pub(crate) heavy_suit: Handle<StandardMaterial>,
    pub(crate) light: Handle<StandardMaterial>,
    pub(crate) heavy: Handle<StandardMaterial>,
    pub(crate) visor: Handle<StandardMaterial>,
}

impl FromWorld for DummyAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let mut cuboid = |size: Vec3| {
            let mesh = Mesh::from(Cuboid::from_size(size)).with_generated_tangents().expect("a box has all that tangents need");
            meshes.add(mesh)
        };
        let parts = biped::body::Part::ALL.map(|part| cuboid(part.size()));
        let light_armor = spawn::LIGHT_ARMOR.iter().map(|piece| cuboid(piece.4)).collect();
        let heavy_armor = spawn::HEAVY_ARMOR.iter().map(|piece| cuboid(piece.4)).collect();
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut matte = |r: f32, g: f32, b: f32| {
            materials.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.9, metallic: 0.0, ..default() })
        };
        Self {
            parts,
            light_armor,
            heavy_armor,
            skin: matte(0.82, 0.72, 0.52),
            suit: matte(0.55, 0.12, 0.1),
            heavy_suit: matte(0.3, 0.08, 0.08),
            light: matte(0.33, 0.4, 0.24),
            heavy: matte(0.7, 0.7, 0.76),
            visor: matte(0.08, 0.1, 0.13),
        }
    }
}

/// A dummy just spawned: itself, its body parts in `Part` order, and the armor pieces it wears.
pub struct SpawnedDummy {
    pub dummy: Entity,
    pub parts: [Entity; PART_COUNT],
    pub armor: Vec<Entity>,
}
