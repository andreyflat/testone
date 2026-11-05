use bevy::prelude::*;

use crate::player::Player;

pub struct LightsPlugin;
impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        // Единственный источник света - фонарик: всё вне его луча остаётся в темноте
        app.insert_resource(GlobalAmbientLight {
                brightness: 0.0,
                ..default()
            })
            .insert_resource(ClearColor(Color::BLACK))
            .add_systems(Update, spawn_spotlight);
    }
}

// 1. Создаем компонент-маркер "Фонарик выдан"
#[derive(Component)]
struct FlashlightEquipped;

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
                        intensity: 1_000_000.0, // Других источников нет, поэтому луч должен быть ярким
                        shadow_maps_enabled: true,
                        range: 150.0, // Ограничьте дальность, чтобы не грузить рендер
                        outer_angle: 0.6,
                        ..default()
                    },
                    // Смещение относительно игрока, луч немного наклонён к полу
                    Transform::from_xyz(0.2, 0.5, -0.5)
                        .looking_at(Vec3::new(0.2, -1.0, -6.0), Vec3::Y),
                    Name::new("Flash Light")
                ));
            });

        info!("Flashlight spawned for player!"); // В логе должно появиться только ОДИН раз
    }
}
