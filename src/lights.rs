use bevy::{
    prelude::*,
    pbr::CascadeShadowConfig,
};

pub struct LightsPlugin;
impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_lights);
    }
}

fn spawn_lights(mut commands: Commands) {
    spawn_directional_light(&mut commands);
}

// Функция для создания направленного света
fn spawn_directional_light(commands: &mut Commands) {
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
    ));
}