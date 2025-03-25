use bevy::{
    prelude::*,
    pbr::CascadeShadowConfig,
};

use crate::player::{Player, spawn_player};

pub struct LightsPlugin;
impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_directional_light, spawn_spotlight.after(spawn_player)));
    }
}

fn spawn_directional_light(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 5000.0,
            shadow_depth_bias: 0.02,
            shadow_normal_bias: 0.6,
            ..default()
        },
        Transform::from_xyz(14.0, 10.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfig {
            minimum_distance: 0.1,
            bounds: vec![0.1, 5.0, 20.0, 100.0],
            overlap_proportion: 0.2,
        },
        Name::new("Directional Light")
    ));
}

fn spawn_spotlight(mut commands: Commands, player_query: Query<Entity, With<Player>>) {
    let player = player_query.single();
    commands.entity(player).with_children(|commands| {
    commands.spawn((
                SpotLight {
            intensity: 100000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::default(),
        Name::new("Flash Light")
        ));
    });
}