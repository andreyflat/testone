use bevy::{
    prelude::*,
    pbr::CascadeShadowConfig,
};

use crate::player::Player;

pub struct LightsPlugin;
impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_directional_light)
           .add_systems(Update, spawn_spotlight);
    }
}

// 1. Создаем компонент-маркер "Фонарик выдан"
#[derive(Component)]
struct FlashlightEquipped; 

fn spawn_directional_light(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 15000.0, // Примечание: 50.0 люкс для солнца маловато, обычно ~10000+
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

fn spawn_spotlight(
    mut commands: Commands, 
    // 2. Ищем игрока БЕЗ маркера FlashlightEquipped
    player_query: Query<Entity, (With<Player>, Without<FlashlightEquipped>)>
) {
    for player_entity in player_query.iter() {
        commands.entity(player_entity)
            // 3. Сразу вешаем маркер на игрока, чтобы в следующем кадре Query его не нашел
            .insert(FlashlightEquipped) 
            .with_children(|parent| {
                parent.spawn((
                    SpotLight {
                        intensity: 100000.0, // Для фонарика это очень ярко, возможно стоит уменьшить до ~1000-5000
                        shadows_enabled: true,
                        range: 20.0, // Ограничьте дальность, чтобы не грузить рендер
                        outer_angle: 0.6,
                        ..default()
                    },
                    Transform::from_xyz(0.2, 0.5, -0.5), // Смещение относительно игрока
                    Name::new("Flash Light")
                ));
            });
            
        info!("Flashlight spawned for player!"); // В логе должно появиться только ОДИН раз
    }
}
