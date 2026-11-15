//! Dummies knocked over, faltering, handed to the physics and back, and getting back up.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use biped::{Biped, Drive};
use biped::body::Part;
use game_core::math::smoothstep;

use super::{Bleeding, FALTER_CROUCH, FALTER_DROOP, FALTER_FADE_IN, FALTER_FADE_OUT, FALTER_HEALTH, FALTER_SLUMP, FALTER_SWAY, FALTER_WANDER, Falter, Flinch, HANDOFF_EASE, Health, KNOCKBACK_FADE, Momentum, REACT_OFF_BALANCE, REACT_TIME, Ragdoll, Reactive, Struggle};
use super::animation::Animation;

/// A living dummy knocked off its feet lies this long, in seconds, without being hit, before
/// getting back up, with whatever health it had...
pub(super) const KNOCKDOWN_RECOVERY_TIME: f32 = 2.5;
/// ...once it has been fallen over for this long, in seconds.
const KNOCKDOWN_TIME: f32 = 0.4;

/// Once it has stopped reacting, it goes back to its animation as soon as it is standing fairly
/// still: moving slower than this, in metres per second, and no further off balance than this,
/// as a fraction of what its feet can hold it over. Blending back takes up what's left.
const SETTLE_SPEED: f32 = 0.3;
const SETTLE_OFF_BALANCE: f32 = 1.2;

/// Stands living dummies back up once they have lain knocked off their feet for
/// `KNOCKDOWN_RECOVERY_TIME` without being hit, as hurt as they were: where they lie, facing the
/// way their pelvis does, blending out of however they lay into their animation.
pub(super) fn recover_ragdolls(
    time: Res<Time>,
    mut dummies: Query<(&mut Ragdoll, &mut Biped, &mut Animation, &mut Reactive)>,
    transforms: Query<&Transform>,
    mut flinches: Query<&mut Flinch>,
    mut bodies: Query<(&mut RigidBody, &mut Velocity)>,
) {
    for (mut ragdoll, mut biped, mut animation, mut reactive) in &mut dummies {
        if ragdoll.down || !ragdoll.knocked {
            continue;
        }
        if !ragdoll.idle.tick(time.delta()).is_finished() {
            continue;
        }
        ragdoll.knocked = false;
        ragdoll.fallen = 0.0;
        ragdoll.idle.reset();
        *reactive = Reactive::default();
        let poses = Part::ALL.map(|part| transforms.get(biped.part(part)).copied().unwrap_or_default());
        animation.resume(poses, &biped.parts, &mut bodies);
        for &part in &biped.parts {
            if let Ok(mut flinch) = flinches.get_mut(part) {
                *flinch = Flinch::default();
            }
        }
        biped.reset();
    }
}

/// Hands animated dummies over to the physics once something calls for their balance: killed,
/// knocked about too hard (see `REACT_KNOCKBACK`), or faltering as they bleed out.
pub(super) fn take_over(
    mut commands: Commands,
    mut dummies: Query<(Entity, &mut Animation, &mut Biped, &Ragdoll, &Falter, &Reactive, Option<&Momentum>)>,
    mut bodies: Query<(&mut RigidBody, &mut Velocity)>,
) {
    for (entity, mut animation, mut biped, ragdoll, falter, reactive, momentum) in &mut dummies {
        if animation.active && (ragdoll.down || reactive.left > 0.0 || falter.level > 0.0) {
            animation.stop(&biped.parts, &mut bodies);
            if let Some(&Momentum(carried)) = momentum {
                for &part in &biped.parts {
                    if let Ok((_, mut velocity)) = bodies.get_mut(part) {
                        velocity.linear += carried;
                    }
                }
                commands.entity(entity).remove::<Momentum>();
            }
            // Its muscles take over from wherever the clip left it, easing into what they want.
            biped.ease_in(HANDOFF_EASE, true);
        }
    }
}

/// Hands living, physically simulated dummies back to their animation once they have settled:
/// no longer reacting or faltering, standing still and balanced. They blend back into it from
/// wherever the physics left them, standing where they ended up.
pub(super) fn settle(
    mut dummies: Query<(&mut Animation, &mut Biped, &Ragdoll, &Falter, &Reactive), Without<Struggle>>,
    transforms: Query<&Transform>,
    mut bodies: Query<(&mut RigidBody, &mut Velocity)>,
) {
    for (mut animation, mut biped, ragdoll, falter, reactive) in &mut dummies {
        if animation.active || ragdoll.down || ragdoll.knocked || reactive.left > 0.0 || falter.level > 0.0 || biped.fallen() {
            continue;
        }
        // It should be standing still; the walker picks up its walk again from there.
        let speed = bodies.get(biped.part(Part::Guts)).map_or(f32::INFINITY, |(_, velocity)| velocity.linear.length());
        if biped.stepping() || biped.off_balance() > SETTLE_OFF_BALANCE || speed > SETTLE_SPEED {
            continue;
        }
        let poses = Part::ALL.map(|part| transforms.get(biped.part(part)).copied().unwrap_or_default());
        animation.resume(poses, &biped.parts, &mut bodies);
        biped.reset();
    }
}

/// Notices living dummies that have been knocked off their feet: fallen over for a moment, they
/// go limp and lie there until they get back up.
pub(super) fn knock_down(
    time: Res<Time>,
    mut dummies: Query<(&mut Ragdoll, &Biped)>,
) {
    for (mut ragdoll, biped) in &mut dummies {
        if ragdoll.down || ragdoll.knocked {
            continue;
        }
        ragdoll.fallen = if biped.fallen() { ragdoll.fallen + time.delta_secs() } else { 0.0 };
        if ragdoll.fallen > KNOCKDOWN_TIME {
            ragdoll.idle.reset();
            ragdoll.knocked = true;
        }
    }
}

/// Sets living, physically simulated dummies' drive: standing, or faltering from blood loss,
/// nearly dead and still bleeding, easing into it and out again once the bleeding stops.
/// Knocked down, they lie limp. Keeps them reacting while they are still catching themselves.
pub(super) fn falter(
    time: Res<Time>,
    mut dummies: Query<(&mut Falter, &mut Reactive, &mut Drive, &Ragdoll, &Bleeding, &Health, &Biped, &Animation), Without<Struggle>>,
) {
    let dt = time.delta_secs();
    for (mut falter, mut reactive, mut drive, ragdoll, bleeding, health, biped, animation) in &mut dummies {
        let physical = !animation.active && !ragdoll.down && !ragdoll.knocked;
        reactive.knockback *= (-dt / KNOCKBACK_FADE).exp();
        reactive.left = (reactive.left - dt).max(0.0);
        // Standing (mid-step, on one foot, it is bound to be off balance), about to fall.
        if physical && !biped.stepping() && biped.off_balance() > REACT_OFF_BALANCE {
            reactive.left = REACT_TIME;
        }
        // Mid-step, it carries on until the step is done.
        if reactive.left > 0.0 && biped.stepping() {
            reactive.left = reactive.left.max(0.3);
        }
        let faltering = health.current < FALTER_HEALTH && !bleeding.stacks.is_empty();
        let (target, rate) = if faltering { (1.0, 1.0 / FALTER_FADE_IN) } else { (0.0, 1.0 / FALTER_FADE_OUT) };
        falter.level += (target - falter.level).clamp(-rate * dt, rate * dt);
        falter.wander += FALTER_WANDER * (2.0 * fastrand::f32() - 1.0) * dt.sqrt();
        let level = smoothstep(falter.level);
        *drive = Drive {
            // Animated, its muscles have nothing to do.
            strength: if ragdoll.down || ragdoll.knocked || animation.active { 0.0 } else { 1.0 },
            // Bleeding, and faltering all the more, make for a sluggish recovery from hits.
            tone: 1.0 / (bleeding.weakness() * (1.0 + level)),
            crouch: FALTER_CROUCH * level,
            slump: FALTER_SLUMP * level,
            droop: FALTER_DROOP * level,
            sway: Vec3::new(falter.wander.cos(), 0.0, falter.wander.sin()) * FALTER_SWAY * level,
            ..default()
        };
    }
}
