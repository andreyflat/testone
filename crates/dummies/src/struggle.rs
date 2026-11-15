//! A downed dummy struggling: fighting to stay up, then on the ground, until it goes limp.

use std::f32::consts::PI;

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use biped::{Biped, Drive, HandTarget};
use biped::body::{FEET, Part, SIDE_SIGNS, UPPER_ARMS};
use game_core::math::smoothstep;

use super::{ALLY_RANGE, CLUTCH_OFFSET, CLUTCH_STRENGTH, CLUTCH_TIME, FALTER_CROUCH, FALTER_DROOP, FALTER_SLUMP, FALTER_SWAY, FALTER_WANDER, GROUND_EASE, GROUND_SETTLE, GROUND_TIME, GroundAct, GroundDrive, HAND_CATCH_DROP, HAND_CATCH_EASE, HAND_CATCH_STRENGTH, HAND_CATCH_TILT, HAND_REACH_AHEAD, HAND_REACH_SPREAD, Intent, KNEEL_ANYWAY, KNEEL_BACKING, KNEEL_DROP, KNEEL_FROM, KNEEL_GIVE_WAY, KNEEL_HOLD, KNEEL_LUNGE, KNEEL_MAX_SPEED, KNEEL_SETTLE, KNEEL_SINKING, KNEEL_TILT, KNEEL_TIP, KNEEL_TIP_ASIDE, KNEEL_TIP_OVER, KNEEL_UNTIL, Kneel, LEG_FALLEN_SLACKEN, LEG_FALLEN_STRENGTH, PRONE, Ragdoll, STAGGER_OFF_SPEED, STAGGER_OFF_WEAK, STAGGER_SWAY, STRUGGLE_LONGEST, STRUGGLE_SHORTEST, STRUGGLE_VARIANCE, STRUGGLE_WOBBLE, Struggle, TOPPLE_LATENESS, TOPPLE_LEAN};

/// As a struggle goes on, the dummy's muscles weaken by up to this fraction more on top of its
/// fading strength, and the most torque they can put out, so how sharply they can change how it
/// moves, falls from this fraction of usual by up to this fraction of that...
const STRUGGLE_FADE: f32 = 0.3;
const STRUGGLE_FORCE: f32 = 0.8;
const STRUGGLE_FORCE_FADE: f32 = 0.75;
/// ...its steps slow down, up to this many times slower by the end...
const STRUGGLE_STEP_SLOWING: f32 = 1.0;
/// ...its balance gets sloppier, from this much to begin with up to this much by the end (see
/// `Drive::sloppiness`)...
const STRUGGLE_SLOPPINESS: (f32, f32) = (0.2, 1.0);
/// ...and less responsive, from this languid to begin with up to this by the end (see
/// `Drive::languor`): only a little, so it stays a fight to the end, but slowing down.
const STRUGGLE_LANGUOR: (f32, f32) = (0.1, 0.32);

/// The legs give way sooner than the rest of a struggle, their strength further scaled by how
/// far it is from going down, but only late on: by one less how far it is going down, raised to
/// this power.
const LEG_FADE_LATENESS: f32 = 2.0;

/// Staggering, the way it sways wanders this much more slowly than it otherwise would, so it
/// keeps stepping the same way for a while.
const STAGGER_WANDER: f32 = 0.4;

/// How far a struggling dummy sinks at the knees, in metres, slumps its chest forwards and hangs
/// its head, in radians, when it has no strength left.
const WEAK_CROUCH: f32 = 0.25;
const WEAK_SLUMP: f32 = 0.5;
const WEAK_HEAD_DROOP: f32 = 0.4;

/// How much its muscles' targets wobble about while on its knees, as for `STRUGGLE_WOBBLE`.
const KNEEL_WOBBLE: f32 = 0.3;

/// Not on its front, trying to get up or crawl, it tries to sit up instead: pushing down on the
/// floor with its hands this far back from its shoulders towards its hips and out to the side,
/// in metres, as when pushing itself up, curling its chest and head forwards and drawing its
/// knees up this far into the curled-up pose (see `Drive::curl`).
const SIT_UP_BACK: f32 = 0.25;
const SIT_UP_OUT: f32 = 0.15;
const SIT_UP_CURL: f32 = 0.7;

/// It strains to sit up in bursts this often, in seconds, drawing its knees up this far.
const SIT_UP_EVERY: f32 = 1.4;
const SIT_UP_KNEES: f32 = 0.7;

/// Straining to sit up, it rocks from side to side, twisting its chest round this far, in
/// radians.
const SIT_UP_ROCK: f32 = 0.35;

/// On its front, trying to get up, it plants its hands beside its shoulders, this far towards
/// its head and out to the side, in metres, and pushes down through them as if to reach this
/// far below its shoulders, its arms this much stronger than usual at full effort: pushing up off
/// the floor. Meanwhile it draws its knees up under it, after this long, in seconds, over this
/// long, onto all fours, and starts to tremble with the effort after this long, as much as a
/// convulsion this hard.
const PUSH_AHEAD: f32 = 0.08;
const PUSH_OUT: f32 = 0.1;
const PUSH_REACH: f32 = 0.65;
const PUSH_STRENGTH: f32 = 3.5;
const PUSH_KNEES: (f32, f32) = (0.8, 0.8);
const PUSH_TREMBLE: (f32, f32) = (2.0, 0.12);

/// Crawling, each arm reaches out this far beyond its shoulder towards its head, in metres,
/// lifting its hand this high, for this fraction of each stroke, then plants it and drags itself
/// up to it, pressing down this far into the floor, its arms this much stronger than usual at
/// full effort. A stroke of both arms takes this long, in seconds; its knees shuffle up in turn,
/// this far, as a fraction, and it writhes a little as it goes.
const CRAWL_REACH: f32 = 0.4;
const CRAWL_LIFT: f32 = 0.08;
const CRAWL_REACHING: f32 = 0.35;
const CRAWL_PRESS: f32 = 0.2;
const CRAWL_STRENGTH: f32 = 4.0;
const CRAWL_STROKE: f32 = 1.6;
const CRAWL_KNEES: f32 = 0.8;
const CRAWL_WRITHE: f32 = 0.15;

/// Writhing, this hard (see `Drive::writhe`), twisting its chest round this far, in radians, and
/// drawing its knees up by turns as far as this, as a fraction.
const GROUND_WRITHE: f32 = 0.8;
const WRITHE_TWIST: f32 = 0.3;
const WRITHE_KNEES: f32 = 0.4;

/// Curling up on the floor, this far into the curled-up pose (see `Drive::curl`), drawing itself
/// in over this long, in seconds, rocking a little as it does.
const GROUND_CURL: f32 = 0.85;
const GROUND_CURL_IN: f32 = 1.2;
const GROUND_CURL_ROCK: f32 = 0.1;

/// Convulsing, this hard (see `Drive::convulse`), in spasms coming and going about this often,
/// in radians per second.
const GROUND_CONVULSE: f32 = 0.9;
const SPASM_RATES: (f32, f32) = (2.7, 1.3);

/// On the floor, doing anything but lying there, its legs keep this much of their strength,
/// slack as they had gone.
const GROUND_LEGS: f32 = 0.8;

/// How long a dummy curling up takes to draw itself in, in seconds, and how far its muscles
/// lean towards the curled-up pose over whatever else they are doing, as a fraction: only a
/// suggestion of it. Its balance weakens by half that much.
const CURL_TIME: f32 = 1.5;
const CURL_NUDGE: f32 = 0.3;

/// Curling up, it is giving up: all its muscles go this much slacker, as a fraction, by the
/// time it has drawn itself in, so it slumps loosely towards the pose rather than holding it.
const CURL_SLACKEN: f32 = 0.5;

/// Making for something more than this far off to the side, in radians, it turns to face it
/// first; and it stops once this close, in metres.
const SEEK_TURN_FIRST: f32 = 1.0;
const SEEK_NEAR: f32 = 1.2;

/// Drives struggling dummies as they weaken: fighting to keep their feet, clutching at their
/// wound or reaching out to catch themselves, sinking to their knees, toppling or curling up,
/// until their strength is spent, they're finished off, or they get up first. Then their
/// muscles relax and the struggle ends.
pub(super) fn struggle(
    mut commands: Commands,
    time: Res<Time>,
    mut dummies: Query<(Entity, &mut Struggle, &mut Drive, &Ragdoll, &Biped)>,
    allies: Query<(Entity, &Biped, &Ragdoll)>,
    players: Query<Entity, With<game_core::Player>>,
    places: Query<&GlobalTransform>,
    parts: Query<(&GlobalTransform, &Velocity)>,
) {
    let dt = time.delta_secs();
    for (dummy, mut struggle, mut drive, ragdoll, biped) in &mut dummies {
        struggle.elapsed += dt;
        let strength = if ragdoll.down { struggle.strength() } else { 0.0 };
        if strength <= 0.0 {
            *drive = Drive { strength: 0.0, ..default() };
            commands.entity(dummy).remove::<Struggle>();
            continue;
        }
        if struggle.bleed_out {
            let wander = if struggle.stagger { FALTER_WANDER * STAGGER_WANDER } else { FALTER_WANDER };
            struggle.wander += wander * (2.0 * fastrand::f32() - 1.0) * dt.sqrt();
        }
        let going_down = smoothstep(struggle.going_down());
        let decline = smoothstep(struggle.progress());
        let curl = if struggle.curl { CURL_NUDGE * smoothstep(struggle.elapsed / CURL_TIME) } else { 0.0 };
        // Fallen over, the legs go slack and stay so, even if the body rolls back upright.
        // Staggering, it catches itself however far it tips.
        let legs_target = if struggle.staggering() || !biped.fallen() { 1.0 } else { LEG_FALLEN_STRENGTH };
        let slacken = (1.0 - LEG_FALLEN_STRENGTH) * dt / LEG_FALLEN_SLACKEN;
        struggle.legs = (struggle.legs - slacken).max(legs_target).min(struggle.legs);
        let legs = (1.0 - going_down.powf(LEG_FADE_LATENESS)) * struggle.legs;
        let legs = if struggle.staggering() { 1.0 } else { legs };

        // Weakening, it sags into its posture: from the one it was faltering in, if bleeding out.
        let weakness = (1.0 - strength).max(struggle.slump);
        let (crouch, slump, droop) = if struggle.bleed_out {
            (
                FALTER_CROUCH.lerp(WEAK_CROUCH, decline),
                FALTER_SLUMP.lerp(WEAK_SLUMP, decline),
                FALTER_DROOP.lerp(WEAK_HEAD_DROOP, decline),
            )
        } else {
            (WEAK_CROUCH * weakness, WEAK_SLUMP * weakness, WEAK_HEAD_DROOP * weakness)
        };
        // Bleeding out, its weight sways about as it did faltering; weakening, its balance gives
        // way to one side, so it topples rather than kneels.
        let sway_by = if struggle.staggering() { STAGGER_SWAY } else { FALTER_SWAY };
        let sway = if struggle.bleed_out { Vec3::new(struggle.wander.cos(), 0.0, struggle.wander.sin()) * sway_by } else { Vec3::ZERO };

        let Ok((chest, _)) = parts.get(biped.part(Part::Chest)) else {
            continue;
        };
        let facing = biped::Biped::facing(&chest.rotation());
        let chest_up = chest.rotation() * Vec3::Y;
        let (mut mass, mut momentum) = (0.0, Vec3::ZERO);
        for part in Part::ALL {
            if let Ok((_, velocity)) = parts.get(biped.part(part)) {
                mass += part.mass();
                momentum += velocity.linear * part.mass();
            }
        }
        let velocity = momentum / mass.max(1.0);
        let ground = FEET
            .iter()
            .filter_map(|&foot| parts.get(biped.part(foot)).ok().map(|(transform, _)| transform.translation().y - foot.size().y / 2.0))
            .fold(f32::INFINITY, f32::min);

        // It may pitch forwards onto its knees, stay there a while, then tip over.
        let now = struggle.elapsed;
        let forward = Quat::from_rotation_y(facing) * Vec3::Z;
        match struggle.kneel {
            Kneel::Ready if biped.fallen() => struggle.kneel = Kneel::Done,
            Kneel::Ready => {
                let down_by = struggle.going_down();
                // Pitching forwards, or going over or down any which way.
                let pitching = chest_up.dot(forward) > KNEEL_TIP.sin()
                    || velocity.dot(forward) > KNEEL_LUNGE
                    || chest_up.y < KNEEL_TILT.cos()
                    || -velocity.y > KNEEL_SINKING;
                // Not while driven back: it only goes down onto its knees going forwards.
                let backing = velocity.dot(forward) < -KNEEL_BACKING;
                let rushing = velocity.with_y(0.0).length() > KNEEL_MAX_SPEED;
                let ready = !struggle.staggering() && !backing && !rushing;
                if ready && down_by > KNEEL_FROM && (pitching || down_by > KNEEL_ANYWAY) {
                    let hold = KNEEL_HOLD.0.lerp(KNEEL_HOLD.1, fastrand::f32());
                    let until = (now + hold).min(struggle.duration * KNEEL_UNTIL).max(now + KNEEL_DROP);
                    struggle.kneel = Kneel::Down { since: now, until };
                }
            }
            Kneel::Down { until, .. } if now >= until || biped.fallen() => {
                let aside = KNEEL_TIP_ASIDE * (2.0 * fastrand::f32() - 1.0);
                struggle.kneel = Kneel::GivingWay { since: now, towards: facing + aside };
            }
            Kneel::GivingWay { since, .. } if now - since > KNEEL_GIVE_WAY => struggle.kneel = Kneel::Done,
            _ => {}
        }
        let kneeling = match struggle.kneel {
            Kneel::Down { .. } => (struggle.kneeling + dt / KNEEL_DROP).min(1.0),
            Kneel::GivingWay { .. } => (struggle.kneeling - dt / KNEEL_GIVE_WAY).max(0.0),
            _ => 0.0,
        };
        struggle.kneeling = kneeling;
        let kneeling = smoothstep(kneeling);
        let toward = |angle: f32| Vec3::Y.cross(Quat::from_rotation_y(angle) * Vec3::Z);
        let lean = match (struggle.topple, struggle.kneel) {
            // Only taking hold late on, leaving it time to stagger about first: toppling towards
            // its way, or, about to kneel, forwards.
            (Some(angle), _) => toward(angle) * TOPPLE_LEAN * going_down.powf(TOPPLE_LATENESS),
            (_, Kneel::Ready) => toward(facing) * TOPPLE_LEAN * going_down.powf(TOPPLE_LATENESS),
            (_, Kneel::GivingWay { since, towards }) => toward(towards) * KNEEL_TIP_OVER * smoothstep((now - since) / KNEEL_GIVE_WAY),
            _ => Vec3::ZERO,
        };

        let mut hands = [None, None];
        // It may clutch at its wound, over the front of the part, with the hand on that side.
        if let Some((wounded, local)) = struggle.clutch
            && let Some(index) = biped.parts.iter().position(|&part| part == wounded)
            && matches!(Part::from_index(index), Part::Chest | Part::Guts | Part::Neck)
            && let Ok((wounded_transform, _)) = parts.get(wounded)
        {
            let half = Part::from_index(index).size() / 2.0;
            let side = if local.x < 0.0 { 0 } else { 1 };
            let across = local.x.abs().min(half.x) * SIDE_SIGNS[side];
            let over = Vec3::new(across, local.y.clamp(-half.y, half.y), half.z + CLUTCH_OFFSET);
            let clutching = smoothstep(struggle.elapsed / CLUTCH_TIME);
            hands[side] = Some(HandTarget {
                point: wounded_transform.transform_point(over),
                // The elbow down at its side, a little back.
                elbow: Vec3::new(SIDE_SIGNS[side] * 0.4, -1.0, -0.1),
                weight: clutching,
                strength: CLUTCH_STRENGTH,
            });
        }

        // Falling, it may put its hands out to the floor ahead of it to catch itself; but not while
        // settling on its knees, which its hands pushing on the floor would only shove it off.
        let reaching = if struggle.catches && !matches!(struggle.kneel, Kneel::Down { .. }) && struggle.ground.is_none() {
            let tilt = (chest_up.angle_between(Vec3::Y) - HAND_CATCH_TILT.0) / HAND_CATCH_TILT.1;
            let drop = (-velocity.y - HAND_CATCH_DROP.0) / HAND_CATCH_DROP.1;
            smoothstep(tilt).max(smoothstep(drop))
        } else {
            0.0
        };
        let ease = if reaching > struggle.reach { HAND_CATCH_EASE.0 } else { HAND_CATCH_EASE.1 };
        struggle.reach += (reaching - struggle.reach) * (1.0 - (-dt / ease).exp());
        let towards = chest_up.with_y(0.0) + velocity.with_y(0.0) * 0.5;
        let reach_towards = struggle.reach_towards;
        struggle.reach_towards = reach_towards + (towards - reach_towards) * (1.0 - (-dt / HAND_CATCH_EASE.0).exp());
        if struggle.reach > 0.01 {
            let heading = chest.rotation() * Vec3::Z;
            let falling = struggle.reach_towards.normalize_or(heading.with_y(0.0).normalize_or(Vec3::Z));
            let right = Vec3::Y.cross(falling).normalize_or(Vec3::X);
            for side in 0..2 {
                let shoulder = chest.transform_point(UPPER_ARMS[side].joint().expect("arms are jointed").point - Part::Chest.center());
                let point = shoulder.with_y(ground) + falling * HAND_REACH_AHEAD + right * SIDE_SIGNS[side] * HAND_REACH_SPREAD;
                hands[side] = Some(HandTarget {
                    point,
                    // The elbow out and back, the forearm reaching on down.
                    elbow: Vec3::new(SIDE_SIGNS[side] * 0.5, 0.0, -1.0),
                    weight: struggle.reach,
                    // Stronger for the effort, but giving way all the sooner as it weakens.
                    strength: HAND_CATCH_STRENGTH * strength,
                });
            }
        }

        // Down on the floor a while, it does something about it.
        struggle.down_for = if biped.fallen() { struggle.down_for + dt } else { 0.0 };
        let settled = matches!(struggle.kneel, Kneel::Never | Kneel::Done);
        if struggle.ground.is_none() && settled && struggle.down_for > GROUND_SETTLE {
            // It struggles on there for longer the longer its struggle was to be, carrying on past
            // when it would have ended on its feet.
            let longest = (struggle.duration - STRUGGLE_SHORTEST) / (STRUGGLE_LONGEST - STRUGGLE_SHORTEST);
            let variance = 1.0 + STRUGGLE_VARIANCE * (2.0 * fastrand::f32() - 1.0);
            let length = GROUND_TIME.0.lerp(GROUND_TIME.1, longest.clamp(0.0, 1.0)) * variance;
            struggle.floor = Some((now, length));
            struggle.duration = struggle.duration.max(now + length);
            let (act, time) = struggle.pick_ground_act(ground_effort(struggle.strength()));
            struggle.ground = Some((act, now, now + time));
        }
        let ground = match struggle.ground {
            // Done with that: on to the next thing.
            Some((_, _, until)) if now >= until => {
                let (act, time) = struggle.pick_ground_act(ground_effort(strength));
                struggle.ground = Some((act, now, now + time));
                GroundDrive::default()
            }
            Some((mut act, since, until)) => {
                let ease = smoothstep((now - since) / GROUND_EASE) * smoothstep((until - now) / GROUND_EASE);
                let drive = on_ground(&mut act, now - since, now, ease, ground_effort(strength), chest, ground, &mut hands);
                struggle.ground = Some((act, since, until));
                drive
            }
            None => GroundDrive::default(),
        };

        // On its feet, it tries to stagger off or turn round now and then, ever more feebly. Not
        // while curling up, on its knees, or staggering about from blood loss, which is its own
        // swaying.
        let here = chest.translation().with_y(0.0);
        let killer = players.iter().next();
        let ally = allies
            .iter()
            .filter(|&(other, _, ragdoll)| other != dummy && !ragdoll.down && !ragdoll.knocked)
            .filter_map(|(_, other, _)| places.get(other.part(Part::Chest)).ok().map(|place| (other.part(Part::Chest), place.translation())))
            .map(|(entity, at)| (entity, at.with_y(0.0).distance(here)))
            .filter(|&(_, distance)| distance < ALLY_RANGE)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(entity, _)| entity);
        struggle.intend(dt, facing, killer, ally);
        let (velocity, heading) = match struggle.intent {
            _ if biped.fallen() || curl > 0.0 || kneeling > 0.0 || struggle.staggering() => (Vec3::ZERO, None),
            _ if struggle.kneel == Kneel::Ready && struggle.going_down() > KNEEL_FROM - KNEEL_SETTLE => (Vec3::ZERO, None),
            Intent::Walk(heading) => {
                let speed = STAGGER_OFF_SPEED * STAGGER_OFF_WEAK.lerp(1.0, strength);
                (Quat::from_rotation_y(heading) * Vec3::Z * speed, Some(heading))
            }
            Intent::Turn(heading) => (Vec3::ZERO, Some(heading)),
            Intent::Hold => (Vec3::ZERO, None),
            Intent::Seek(target) => match places.get(target) {
                Ok(place) => {
                    let to = (place.translation() - chest.translation()).with_y(0.0);
                    let heading = to.x.atan2(to.z);
                    let off = (heading - facing + PI).rem_euclid(2.0 * PI) - PI;
                    let speed = STAGGER_OFF_SPEED * STAGGER_OFF_WEAK.lerp(1.0, strength);
                    // Off to the side or behind, it turns to face it first; there, it stops.
                    let walks = off.abs() < SEEK_TURN_FIRST && to.length() > SEEK_NEAR;
                    (if walks { to.normalize_or_zero() * speed } else { Vec3::ZERO }, Some(heading))
                }
                Err(_) => (Vec3::ZERO, None),
            },
        };
        // The longer it goes on, the weaker, slower and sloppier it gets.
        let worn = struggle.progress();
        *drive = Drive {
            strength: strength * (1.0 - CURL_SLACKEN * curl / CURL_NUDGE) * (1.0 - STRUGGLE_FADE * worn),
            // On its knees, it is no longer fighting to balance but resting there, steadily; on
            // the floor, it puts what it has into whatever it is doing.
            force: (STRUGGLE_FORCE * (1.0 - STRUGGLE_FORCE_FADE * worn)).lerp(1.0, kneeling.max(ground.effort)),
            sloppiness: STRUGGLE_SLOPPINESS.0.lerp(STRUGGLE_SLOPPINESS.1, worn) * (1.0 - kneeling),
            languor: STRUGGLE_LANGUOR.0.lerp(STRUGGLE_LANGUOR.1, worn) * (1.0 - kneeling.max(ground.effort)),
            // On its knees, they hold it up regardless of how far it has gone down; on the floor,
            // they move again for whatever it is doing.
            legs: legs.lerp(1.0, kneeling).max(GROUND_LEGS * ground.effort),
            // The neck holds the head up through most of a struggle, giving way only near the end.
            neck: (strength.sqrt() / strength).min(3.0),
            arms: 1.0,
            tone: 1.0,
            balance: 1.0 - curl / 2.0,
            steps: kneeling <= 0.0,
            reach: strength.max(0.2),
            step_time: (1.0 + STRUGGLE_STEP_SLOWING * worn) / strength.max(0.3),
            velocity,
            heading,
            sway: sway * (1.0 - kneeling),
            lean,
            crouch,
            slump: slump * (1.0 - ground.straighten),
            droop: droop * (1.0 - ground.straighten),
            twist: struggle.twist * going_down + ground.twist,
            curl: curl.max(ground.curl),
            kneel: kneeling,
            tuck: ground.tuck,
            writhe: ground.writhe,
            convulse: ground.convulse,
            // Down on its knees, it isn't balancing on its feet to throw its arms out for.
            arm_reflex: strength * (1.0 - kneeling),
            hands,
            wobble: STRUGGLE_WOBBLE.lerp(KNEEL_WOBBLE, kneeling),
        };
    }
}

/// How much effort a struggling dummy with muscles at `strength` puts into doing something about
/// being down on the floor, from 1 to 0: more than its strength, giving out only near the end.
pub(super) fn ground_effort(strength: f32) -> f32 {
    strength.clamp(0.0, 1.0).sqrt()
}

/// What a struggling dummy down on the floor does with its body, `time` into doing `act`, `now`
/// into the struggle, eased in by `ease`, with `effort` left in it, its chest where it is and
/// the floor at `floor`, setting where it wants its hands.
pub(super) fn on_ground(
    act: &mut GroundAct,
    time: f32,
    now: f32,
    ease: f32,
    effort: f32,
    chest: &GlobalTransform,
    floor: f32,
    hands: &mut [Option<HandTarget>; 2],
) -> GroundDrive {
    let mut drive = GroundDrive { effort: ease, ..default() };
    let rotation = chest.rotation();
    let prone = (rotation * Vec3::Z).y < -PRONE;
    // Towards its head, along the floor, and out to its right.
    let headwards = (rotation * Vec3::Y).with_y(0.0).normalize_or(Vec3::Z);
    let right = rotation * Vec3::X;
    let shoulders = [0, 1].map(|side| chest.transform_point(UPPER_ARMS[side].joint().expect("arms are jointed").point - Part::Chest.center()));
    // On its back or side, trying to get up, it props itself up on its hands and tries to sit
    // up, curling forwards.
    let sit_up = |drive: &mut GroundDrive, hands: &mut [Option<HandTarget>; 2]| {
        // In bursts of straining, sagging back between them.
        let straining = (PI * time / SIT_UP_EVERY).sin().abs().sqrt() * ease;
        drive.curl = SIT_UP_CURL * straining;
        drive.tuck = [SIT_UP_KNEES * straining; 2];
        // Rocking from side to side as it strains, which may tip it over onto its front.
        drive.twist = SIT_UP_ROCK * (PI * time / SIT_UP_EVERY).sin() * ease;
        for side in 0..2 {
            let shoulder = shoulders[side];
            let behind = shoulder - headwards * SIT_UP_BACK + right * SIDE_SIGNS[side] * SIT_UP_OUT;
            hands[side] = Some(HandTarget {
                point: behind.with_y(shoulder.y - PUSH_REACH),
                elbow: Vec3::new(SIDE_SIGNS[side], 0.0, -0.5),
                weight: straining,
                strength: 1.0 + (PUSH_STRENGTH - 1.0) * effort,
            });
        }
    };
    match act {
        GroundAct::GetUp { front } => {
            if front.is_none() && prone {
                *front = Some(time);
            }
            match *front {
                None => sit_up(&mut drive, hands),
                Some(front) => {
                    // Hands down beside its shoulders, pushing up off the floor, knees drawn
                    // up under it, trembling with the effort.
                    let pushing = time - front;
                    let placed = smoothstep(pushing / 0.5) * ease;
                    for side in 0..2 {
                        let shoulder = shoulders[side];
                        let beside = shoulder + headwards * PUSH_AHEAD + right * SIDE_SIGNS[side] * PUSH_OUT;
                        hands[side] = Some(HandTarget {
                            point: beside.with_y(shoulder.y - PUSH_REACH),
                            elbow: Vec3::new(SIDE_SIGNS[side] * 0.3, -1.0, 0.0),
                            weight: placed,
                            strength: 1.0 + (PUSH_STRENGTH - 1.0) * effort,
                        });
                    }
                    let knees = smoothstep((pushing - PUSH_KNEES.0) / PUSH_KNEES.1) * ease;
                    drive.tuck = [knees; 2];
                    drive.straighten = placed;
                    drive.convulse = PUSH_TREMBLE.1 * smoothstep(pushing - PUSH_TREMBLE.0) * ease;
                }
            }
        }
        GroundAct::Crawl => {
            if !prone {
                sit_up(&mut drive, hands);
                return drive;
            }
            // Each arm in turn reaches out ahead, then plants its hand and drags itself up to it.
            let stroke = time / CRAWL_STROKE;
            for side in 0..2 {
                let phase = (stroke + side as f32 * 0.5).fract();
                let shoulder = shoulders[side];
                let (along, lift) = if phase < CRAWL_REACHING {
                    let reaching = phase / CRAWL_REACHING;
                    (smoothstep(reaching), CRAWL_LIFT * (PI * reaching).sin())
                } else {
                    (1.0 - smoothstep((phase - CRAWL_REACHING) / (1.0 - CRAWL_REACHING)), -CRAWL_PRESS)
                };
                let point = shoulder + headwards * along * CRAWL_REACH + right * SIDE_SIGNS[side] * PUSH_OUT;
                hands[side] = Some(HandTarget {
                    point: point.with_y(floor + lift),
                    elbow: Vec3::new(SIDE_SIGNS[side] * 0.5, -1.0, 0.0),
                    weight: ease,
                    strength: 1.0 + (CRAWL_STRENGTH - 1.0) * effort,
                });
                // The knee on the other side shuffles up as the arm pulls.
                let shuffle = (2.0 * PI * (stroke + side as f32 * 0.5)).sin().max(0.0);
                drive.tuck[1 - side] = CRAWL_KNEES * shuffle * ease;
            }
            drive.writhe = CRAWL_WRITHE * ease;
            drive.straighten = 0.5 * ease;
        }
        GroundAct::Writhe => {
            drive.writhe = GROUND_WRITHE * ease * effort.max(0.5);
            drive.twist = WRITHE_TWIST * (now * 1.3).sin() * ease;
            for (foot, tuck) in drive.tuck.iter_mut().enumerate() {
                *tuck = WRITHE_KNEES * (now * 1.1 + foot as f32 * 2.0).sin().max(0.0) * ease;
            }
        }
        GroundAct::Curl => {
            drive.curl = GROUND_CURL * smoothstep(time / GROUND_CURL_IN) * ease;
            // Its legs too, firmly enough to draw them up off the floor, unless lying on its
            // front, where that would only lift its feet into the air.
            if !prone {
                drive.tuck = [drive.curl; 2];
            }
            drive.writhe = GROUND_CURL_ROCK * ease;
        }
        GroundAct::Convulse => {
            let spasms = ((now * SPASM_RATES.0).sin() * (now * SPASM_RATES.1 + 0.7).sin()).abs().sqrt();
            drive.convulse = GROUND_CONVULSE * spasms * ease;
        }
        GroundAct::Lie => drive.effort = 0.0,
    }
    drive
}
