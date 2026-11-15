//! What a shot does to whatever it hits, shared between the weapon that decides it and the
//! targets that take it: which body parts and armor there are, what a hit on each does, and the
//! damage messages the weapon writes.

use bevy::prelude::*;

/// Damage multiplier for hits to arms and legs.
const LIMB_DAMAGE_MULTIPLIER: f32 = 0.5;
/// Half the width and height of the heart zone at the centre of the chest, where a hit kills
/// outright.
const HEART_HALF_SIZE: Vec2 = Vec2::new(0.045, 0.045);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyPartKind {
    Head,
    Neck,
    Chest,
    Guts,
    Arm,
    Leg,
}

/// A box of a dummy's body. Its parent is the dummy.
#[derive(Component, Debug)]
pub struct BodyPart(pub BodyPartKind);

/// What a hit on a body part does before any armor in the way is taken into account.
pub struct PartHit {
    pub amount: f32,
    /// Kills outright, whatever the dummy's health.
    pub lethal: bool,
}

impl BodyPart {
    /// A hit for `damage` at `local`, a point in the part's own space.
    pub fn hit(&self, damage: f32, local: Vec3) -> PartHit {
        let (multiplier, lethal) = match self.0 {
            BodyPartKind::Head | BodyPartKind::Neck => (1.0, true),
            BodyPartKind::Chest => (1.0, local.x.abs() <= HEART_HALF_SIZE.x && local.y.abs() <= HEART_HALF_SIZE.y),
            BodyPartKind::Guts => (1.0, false),
            BodyPartKind::Arm | BodyPartKind::Leg => (LIMB_DAMAGE_MULTIPLIER, false),
        };
        PartHit { amount: damage * multiplier, lethal }
    }
}

/// A piece of armor. Its parent is the body part it covers.
#[derive(Component, Debug)]
pub struct Armor {
    /// The dummy wearing it.
    pub dummy: Entity,
}

/// A shot's effect on a dummy, written by the weapon.
#[derive(Message, Debug)]
pub struct Damage {
    pub dummy: Entity,
    /// The body part struck.
    pub part: Entity,
    /// Where on the dummy the shot struck.
    pub point: Vec3,
    /// Direction the shot was travelling.
    pub direction: Vec3,
    pub amount: f32,
    /// From a grenade, its fragments or its blast: any wound it opens bleeds for longer.
    pub grenade: bool,
    /// Kills outright, whatever the dummy's health.
    pub lethal: bool,
    /// Stopped by armor, which passed on only a fraction of the damage.
    pub blunt: bool,
    /// Share of the shot's momentum this hit delivered, from 0 to 1: all of it if the shot
    /// stopped here or in armor over it, less if it went right through.
    pub impact: f32,
    /// The shot went right through the part.
    pub through: bool,
    /// Not a shot but bleeding, which only saps health: no flinching, pushing or new bleeding.
    pub bleed: bool,
}
