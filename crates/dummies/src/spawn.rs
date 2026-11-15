//! Spawning dummies, with their body and armor.

use bevy::prelude::*;
use bevy::solari::prelude::RaytracingMesh3d;
use bevy_rapier3d::prelude::*;
use biped::{Biped, Drive, Muscle};
use biped::body::{Part, spawn_body};
use game_core::collision::enemy_groups;
use game_core::combat::{Armor, BodyPart};

use super::{ArmorMaterial, Bleeding, DummyAssets, Falter, Flinch, Health, LimbDamage, Protection, Ragdoll, Reactive, SpawnedDummy};
use super::animation::Animation;
use super::ragdoll::KNOCKDOWN_RECOVERY_TIME;

/// Health a dummy starts with.
const DUMMY_HEALTH: f32 = 100.0;

/// A vest enclosing the chest and guts, in two pieces so it bends at the waist, as (name, body
/// part it covers, material, centre, size), with positions as the body stands (see `body`).
pub(super) const LIGHT_ARMOR: [(&str, Part, ArmorMaterial, Vec3, Vec3); 2] = [
    ("Vest chest", Part::Chest, ArmorMaterial::Light, Vec3::new(0.0, 1.355, 0.0), Vec3::new(0.42, 0.31, 0.27)),
    ("Vest belly", Part::Guts, ArmorMaterial::Light, Vec3::new(0.0, 1.045, 0.0), Vec3::new(0.42, 0.31, 0.27)),
];

/// Plates enclosing the chest, guts, thighs and the tops of the arms, and a helmet with a visor
/// over the face, open at the bottom, as (name, body part it covers, material, centre, size),
/// with positions as the body stands.
pub(super) const HEAVY_ARMOR: [(&str, Part, ArmorMaterial, Vec3, Vec3); 11] = [
    ("Chest plate", Part::Chest, ArmorMaterial::Heavy, Vec3::new(0.0, 1.35, 0.0), Vec3::new(0.41, 0.32, 0.28)),
    ("Waist guard", Part::Guts, ArmorMaterial::Heavy, Vec3::new(0.0, 1.05, 0.0), Vec3::new(0.38, 0.3, 0.26)),
    ("Left thigh guard", Part::LeftThigh, ArmorMaterial::Heavy, Vec3::new(-0.105, 0.7, 0.0), Vec3::new(0.17, 0.38, 0.2)),
    ("Right thigh guard", Part::RightThigh, ArmorMaterial::Heavy, Vec3::new(0.105, 0.7, 0.0), Vec3::new(0.17, 0.38, 0.2)),
    ("Left shoulder pad", Part::LeftUpperArm, ArmorMaterial::Heavy, Vec3::new(-0.26, 1.47, 0.0), Vec3::new(0.14, 0.1, 0.14)),
    ("Right shoulder pad", Part::RightUpperArm, ArmorMaterial::Heavy, Vec3::new(0.26, 1.47, 0.0), Vec3::new(0.14, 0.1, 0.14)),
    ("Helmet top", Part::Head, ArmorMaterial::Heavy, Vec3::new(0.0, 1.815, 0.0), Vec3::new(0.26, 0.03, 0.28)),
    ("Helmet back", Part::Head, ArmorMaterial::Heavy, Vec3::new(0.0, 1.71, -0.125), Vec3::new(0.26, 0.21, 0.03)),
    ("Helmet left", Part::Head, ArmorMaterial::Heavy, Vec3::new(-0.115, 1.71, 0.0), Vec3::new(0.03, 0.21, 0.28)),
    ("Helmet right", Part::Head, ArmorMaterial::Heavy, Vec3::new(0.115, 1.71, 0.0), Vec3::new(0.03, 0.21, 0.28)),
    // Between the helmet's sides, in front of the face, down to the helmet's lower edge.
    ("Visor", Part::Head, ArmorMaterial::Visor, Vec3::new(0.0, 1.71, 0.125), Vec3::new(0.2, 0.21, 0.03)),
];

/// Spawns a dummy named `name` wearing `protection`, standing at `position` and facing `facing`,
/// as an angle about the vertical, with its body parts and armor. Each body part is drawn as its
/// own box, and each armor piece is a box on the part it covers, moving with it as part of the
/// same rigid body: every collider is exactly the box drawn.
pub fn spawn_dummy(
    commands: &mut Commands,
    assets: &DummyAssets,
    name: &str,
    protection: Protection,
    position: Vec3,
    facing: f32,
) -> SpawnedDummy {
    let home = Transform::from_rotation(Quat::from_rotation_y(facing));
    let dummy = commands
        .spawn((
            Name::new(name.to_string()),
            Health { current: DUMMY_HEALTH, max: DUMMY_HEALTH },
            Ragdoll {
                down: false,
                knocked: false,
                fallen: 0.0,
                idle: Timer::from_seconds(KNOCKDOWN_RECOVERY_TIME, TimerMode::Once),
                killed_in: None,
            },
            Falter::default(),
            Reactive::default(),
            Bleeding::default(),
            Drive::default(),
            Transform::from_translation(position),
            Visibility::default(),
        ))
        .id();

    let parts = spawn_body(commands, dummy, home);
    commands.entity(dummy).insert((Biped::new(parts), Animation::standing(home)));
    let suit = if matches!(protection, Protection::Heavy) { &assets.heavy_suit } else { &assets.suit };
    for part in Part::ALL {
        let mesh = assets.parts[part.index()].clone();
        commands.entity(parts[part.index()]).insert((
            BodyPart(part.kind()),
            LimbDamage::default(),
            Flinch::default(),
            Muscle::default(),
            // Animated to begin with.
            RigidBody::KinematicPositionBased,
            enemy_groups(),
            Mesh3d(mesh.clone()),
            RaytracingMesh3d(mesh),
            MeshMaterial3d(if matches!(part, Part::Head | Part::Neck) { assets.skin.clone() } else { suit.clone() }),
        ));
    }

    let (pieces, meshes): (&[_], &[Handle<Mesh>]) = match protection {
        Protection::None => (&[], &[]),
        Protection::Light => (&LIGHT_ARMOR, &assets.light_armor),
        Protection::Heavy => (&HEAVY_ARMOR, &assets.heavy_armor),
    };
    let armor = pieces
        .iter()
        .zip(meshes)
        .map(|(&(name, part, kind, center, size), mesh)| {
            let material = match kind {
                ArmorMaterial::Light => assets.light.clone(),
                ArmorMaterial::Heavy => assets.heavy.clone(),
                ArmorMaterial::Visor => assets.visor.clone(),
            };
            commands
                .spawn((
                    Name::new(name),
                    Armor { dummy },
                    Transform::from_translation(center - part.center()),
                    Collider::cuboid(size.x / 2.0, size.y / 2.0, size.z / 2.0),
                    // Light enough not to change how the body moves.
                    ColliderMassProperties::Density(0.0),
                    enemy_groups(),
                    Mesh3d(mesh.clone()),
                    RaytracingMesh3d(mesh.clone()),
                    MeshMaterial3d(material),
                    ChildOf(parts[part.index()]),
                ))
                .id()
        })
        .collect();
    SpawnedDummy { dummy, parts, armor }
}
