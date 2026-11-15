//! What the game's crates share: collision groups, the player's marker, the vocabulary of hits
//! (which body parts there are, the damage messages), and the physics' pull. Everything depends
//! on this. Ported from pistol_game, cut down to what testone uses.

pub mod collision;
pub mod combat;
pub mod math;

use bevy::prelude::*;

/// Gravity's pull, in metres per second squared, as Rapier has it.
pub const GRAVITY: f32 = 9.81;

/// The player's character. Its controller is in the game itself; this marker is here so anything
/// that only needs to find the player doesn't depend on how it moves.
#[derive(Component)]
pub struct Player;
