use bevy::{
    prelude::*,
    pbr::CascadeShadowConfig,
};

pub struct LightsPlugin;
impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_lights)
            .add_systems(Update, update_player_spotlight);
    }
}

#[derive(Component)]
pub struct PlayerSpotlight;

fn spawn_lights(mut commands: Commands) {
    spawn_directional_light(&mut commands);
    spawn_player_spotlight(&mut commands);
}

// Функция для создания направленного света
fn spawn_directional_light(commands: &mut Commands) {
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 100.0,
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
        Name::new("Directional Light"),
    ));
}

fn spawn_player_spotlight(commands: &mut Commands) {
    commands.spawn((
        SpotLight {
            color: Color::srgb(1.0, 0.9, 0.5),
            intensity: 50000.0,
            range: 30.0,
            radius: 2.0,
            shadows_enabled: true,
            inner_angle: 0.3,
            outer_angle: 0.5,
            ..default()
        },
        Transform::from_xyz(0.0, 6.0, 0.0).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
        PlayerSpotlight,
        Name::new("Flashlight"),
    ));
}

fn update_player_spotlight(
    mut transforms: ParamSet<(
        Query<&Transform, With<crate::player::Player>>,
        Query<(&mut Transform, &PlayerSpotlight)>,
    )>,
) {
    let player_pos = if let Ok(player_transform) = transforms.p0().get_single() {
        player_transform.translation
    } else {
        return;
    };

    for (mut spotlight_transform, _) in transforms.p1().iter_mut() {
        spotlight_transform.translation = Vec3::new(
            player_pos.x,
            player_pos.y + 6.0,
            player_pos.z
        );
        spotlight_transform.look_at(Vec3::new(player_pos.x, 0.0, player_pos.z), Vec3::Y);
    }
}