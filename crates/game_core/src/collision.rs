//! Collision groups shared across the game. Colliders without a `CollisionGroups` component are
//! the environment: they belong to every group and collide with everything.

use bevy_rapier3d::prelude::*;

/// The player. Enemies hurt it by touching it, but neither they nor their bodies block it.
pub const PLAYER_GROUP: Group = Group::GROUP_1;
/// Body parts of living enemies.
pub const ENEMY_GROUP: Group = Group::GROUP_2;
/// Body parts of downed ragdolls.
pub const RAGDOLL_GROUP: Group = Group::GROUP_3;

/// Groups of the environment: in every group, colliding with everything. The same as having no
/// `CollisionGroups` at all, whose `Default` is instead in no group, colliding with nothing.
pub fn environment_groups() -> CollisionGroups {
    CollisionGroups::new(Group::ALL, Group::ALL)
}

/// Groups for a living enemy's body parts: they collide with everything but the player.
pub fn enemy_groups() -> CollisionGroups {
    CollisionGroups::new(ENEMY_GROUP, Group::ALL.difference(PLAYER_GROUP))
}

/// Groups for a downed ragdoll's body parts: they collide with the environment and each other,
/// so limbs don't pass through the body, but not with the player or living enemies, who walk
/// through them. Neighbouring parts, which touch at their joint and would jitter, are kept apart
/// by their joints disabling contacts.
pub fn ragdoll_groups() -> CollisionGroups {
    CollisionGroups::new(RAGDOLL_GROUP, Group::ALL.difference(PLAYER_GROUP | ENEMY_GROUP))
}
