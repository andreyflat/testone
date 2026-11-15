//! Damage dealt to dummies, their flinching and bleeding.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use biped::{Biped, Muscle};
use biped::body::Part;
use game_core::collision::ragdoll_groups;
use game_core::combat::{BodyPart, BodyPartKind, Damage};
use game_core::math::smoothstep;

use super::{BLEED_PER_SECOND, BLEED_TICK, Bleeding, FLINCH_FALLOFF, FLINCH_RECOVERY, FLINCH_SLACK, Falter, Flinch, Health, LIMB_CRIPPLE_DAMAGE, LimbDamage, MAX_FLINCH_SLACK, MIN_LIVING_MUSCLE, REACT_KNOCKBACK, REACT_TIME, Ragdoll, Reactive, STRUGGLE_EASE, Struggle};
use super::animation::Animation;

/// Push a shot gives the body part it hits, in newton-seconds, on a living dummy...
const LIVING_SHOT_IMPULSE: f32 = 7.5;
/// ...on a limp one...
const SHOT_IMPULSE: f32 = 12.0;
/// ...and on one it has just killed or that is struggling, so the hit visibly knocks it about.
const KILL_SHOT_IMPULSE: f32 = 16.0;

/// Damage taken while struggling that would use up the whole struggle. Less hurries it towards
/// its end in proportion: a fifth of this skips a fifth of it.
const STRUGGLE_FINISH_DAMAGE: f32 = 130.0;

/// Whether shots into the chest or guts open wounds that bleed. Not in testone: its weapons are
/// balanced on plain health, and a burst into the chest would otherwise bleed anything dry.
const WOUNDS_BLEED: bool = false;
/// How long a stack of bleeding lasts from a chest wound, and from a gut wound. One gut stack
/// bleeds 63 in all, so a gut shot (35) and its own bleeding leave an unhurt dummy on 2 health.
const CHEST_BLEED_DURATION: f32 = 12.0;
const GUT_BLEED_DURATION: f32 = 24.0;
/// A grenade's wounds bleed this many times as long: a gut wound from one bleeds 189, more than a
/// dummy has.
const GRENADE_BLEEDS_LONGER: f32 = 3.0;

/// Chance that a dummy bleeding to death tends to curl up as it struggles: never, as it looked
/// too stiff.
const BLEED_CURL_CHANCE: f32 = 0.0;

/// Applies damage to dummies. Every shot pushes the part it hits;
/// a living dummy flinches from it, and one that is killed starts struggling, or drops if shot in
/// the head.
pub(super) fn apply_damage(
    mut commands: Commands,
    mut damage: MessageReader<Damage>,
    mut dummies: Query<(
        &mut Health,
        &mut Ragdoll,
        &mut Biped,
        Option<&mut Struggle>,
        &Falter,
        &mut Bleeding,
        &mut Reactive,
        &mut Animation,
        &GlobalTransform,
    )>,
    mut parts: Query<(&BodyPart, &GlobalTransform, &mut ExternalImpulse, &mut LimbDamage, &mut Flinch, Option<&Children>)>,
    mut colliders: Query<&mut CollisionGroups>,
    mut bodies: Query<(&mut RigidBody, &mut Velocity)>,
) {
    for damage in damage.read() {
        let Ok((mut health, mut ragdoll, mut biped, struggle, falter, mut bleeding, mut reactive, mut animation, dummy_transform)) =
            dummies.get_mut(damage.dummy)
        else {
            continue;
        };
        let struggling = struggle.is_some();
        let kind = parts.get(damage.part).map(|(part, ..)| part.0).ok();
        // Only a shot into the head counts, not one its helmet stopped.
        let headshot = !damage.blunt && kind == Some(BodyPartKind::Head);
        if !damage.bleed {
            if let Ok((.., mut limb, _, _)) = parts.get_mut(damage.part) {
                limb.0 += damage.amount;
            }
            // A shot into the chest or guts opens a wound; one right through, two.
            let wound = match kind {
                Some(BodyPartKind::Chest) => Some(CHEST_BLEED_DURATION),
                Some(BodyPartKind::Guts) => Some(GUT_BLEED_DURATION),
                _ => None,
            };
            if let Some(duration) = wound
                && WOUNDS_BLEED
                && !damage.blunt
            {
                if let Ok((_, transform, ..)) = parts.get(damage.part) {
                    bleeding.last_wound = Some((damage.part, transform.affine().inverse().transform_point3(damage.point)));
                }
                let wounds = if damage.through { 2 } else { 1 };
                let duration = if damage.grenade { duration * GRENADE_BLEEDS_LONGER } else { duration };
                let ticks = (duration / BLEED_TICK).round() as u32;
                bleeding.stacks.extend(std::iter::repeat_n(ticks, wounds));
            }
        }
        // A headshot finishes off a struggling dummy; other damage hurries it towards the end.
        if let Some(mut struggle) = struggle {
            struggle.elapsed += damage.amount / STRUGGLE_FINISH_DAMAGE * struggle.duration;
            struggle.finished |= headshot;
        }
        ragdoll.idle.reset();
        let killed = !ragdoll.down && (damage.lethal || damage.amount >= health.current);
        if killed {
            ragdoll.down = true;
            ragdoll.killed_in = if damage.bleed { None } else { kind };
            // Its body no longer gets in the way of anyone walking past.
            for &part in &biped.parts {
                let armor = parts.get(part).ok().and_then(|(.., children)| children).into_iter().flatten().copied();
                for collider in std::iter::once(part).chain(armor.collect::<Vec<_>>()) {
                    if let Ok(mut groups) = colliders.get_mut(collider) {
                        *groups = ragdoll_groups();
                    }
                }
            }
        }
        // Shot in the head, it drops; anywhere else, it tries to stay on its feet, unless it is
        // already lying on the floor.
        if killed && !headshot && !ragdoll.knocked {
            // Bleeding to death, it may curl up (see `BLEED_CURL_CHANCE`).
            let curl = damage.bleed && fastrand::f32() < BLEED_CURL_CHANCE;
            // Bleeding out, it struggles for as long as it can, however it got there.
            let severity = if damage.bleed { 0.0 } else { damage.amount };
            // It may clutch at the killing wound: where it was shot, or where it is bleeding from.
            let wound = if damage.bleed {
                bleeding.last_wound
            } else {
                parts.get(damage.part).ok().map(|(_, transform, ..)| {
                    (damage.part, transform.affine().inverse().transform_point3(damage.point))
                })
            };
            let struggle = Struggle::new(severity, curl, smoothstep(falter.level), damage.bleed, wound, falter.wander, damage.direction);
            commands.entity(damage.dummy).insert(struggle);
            // Its muscles ease into its struggle rather than snapping to it.
            biped.ease_in(STRUGGLE_EASE, false);
        }
        health.current = if ragdoll.down { 0.0 } else { health.current - damage.amount };

        if !damage.bleed
            && let Ok((_, transform, mut impulse, _, _, _)) = parts.get_mut(damage.part)
        {
            let push = if killed || struggling {
                KILL_SHOT_IMPULSE
            } else if ragdoll.down {
                SHOT_IMPULSE
            } else {
                LIVING_SHOT_IMPULSE * damage.impact
            };
            // Knocked about hard enough at once, or killed, it reacts with its whole body: the
            // physics takes over from its animation, the shot shoving it. Otherwise it just
            // flinches.
            if !ragdoll.down || killed {
                reactive.knockback += push;
                if reactive.knockback >= REACT_KNOCKBACK || killed {
                    reactive.left = REACT_TIME;
                    animation.stop(&biped.parts, &mut bodies);
                }
            }
            if animation.active {
                if let Some(part) = biped.parts.iter().position(|&entity| entity == damage.part).map(Part::from_index) {
                    let to_local = dummy_transform.affine().inverse();
                    let point = to_local.transform_point3(damage.point);
                    let direction = to_local.transform_vector3(damage.direction);
                    animation.flinch(part, point, direction, damage.impact);
                }
            } else {
                let shove = damage.direction * push;
                impulse.impulse += shove;
                impulse.torque_impulse += (damage.point - transform.translation()).cross(shove);
            }
        }
        // A living dummy flinches: the muscle holding the part hit goes slack, and less so those
        // above it, back to the pelvis.
        if !ragdoll.down
            && !damage.bleed
            && !animation.active
            && let Some(mut part) = biped.parts.iter().position(|&entity| entity == damage.part).map(Part::from_index)
        {
            let mut slack = FLINCH_SLACK * damage.impact;
            loop {
                if let Ok((.., mut flinch, _)) = parts.get_mut(biped.part(part)) {
                    flinch.0 = (flinch.0 + slack).min(MAX_FLINCH_SLACK);
                }
                slack *= FLINCH_FALLOFF;
                match part.parent() {
                    Some(parent) => part = parent,
                    None => break,
                }
            }
        }
    }
}

/// Lets flinching muscles recover, more slowly while bleeding, and sets every body part's muscle
/// strength from its flinching and the damage it has taken.
pub(super) fn flinch(
    time: Res<Time>,
    dummies: Query<(&Biped, &Bleeding, &Ragdoll)>,
    mut parts: Query<(&mut Flinch, &LimbDamage, &mut Muscle)>,
) {
    let dt = time.delta_secs();
    for (biped, bleeding, ragdoll) in &dummies {
        for part in biped.parts {
            let Ok((mut flinch, damage, mut muscle)) = parts.get_mut(part) else {
                continue;
            };
            flinch.0 = (flinch.0 - FLINCH_RECOVERY / bleeding.weakness() * dt).max(0.0);
            let injury = (1.0 - damage.0 / LIMB_CRIPPLE_DAMAGE).clamp(0.0, 1.0);
            // Alive, a crippled limb still does enough to stand on.
            let injury = if ragdoll.down { injury } else { injury.max(MIN_LIVING_MUSCLE) };
            muscle.0 = injury * (1.0 - flinch.0);
        }
    }
}

/// Saps living dummies' health with their bleeding, a tick at a time, as `Damage` from the
/// pelvis. Every stack bleeds on each tick until it runs out, whether or not the dummy is down
/// to feel it.
pub(super) fn bleed(
    time: Res<Time>,
    mut dummies: Query<(Entity, &mut Bleeding, &Ragdoll, &Biped)>,
    parts: Query<&GlobalTransform>,
    mut damage: MessageWriter<Damage>,
) {
    for (dummy, mut bleeding, ragdoll, biped) in &mut dummies {
        if bleeding.stacks.is_empty() {
            // The first wound bleeds a full tick after it opens.
            bleeding.tick.reset();
            continue;
        }
        let ticks = bleeding.tick.tick(time.delta()).times_finished_this_tick();
        for _ in 0..ticks {
            let open = bleeding.stacks.len();
            bleeding.stacks.retain_mut(|left| {
                *left -= 1;
                *left > 0
            });
            if ragdoll.down || open == 0 {
                continue;
            }
            let pelvis = biped.part(Part::Guts);
            let Ok(transform) = parts.get(pelvis) else {
                continue;
            };
            damage.write(Damage {
                dummy,
                part: pelvis,
                point: transform.translation(),
                direction: Vec3::ZERO,
                amount: BLEED_PER_SECOND * BLEED_TICK * open as f32,
                lethal: false,
                blunt: false,
                impact: 0.0,
                through: false,
                bleed: true,
                grenade: false,
            });
        }
    }
}
