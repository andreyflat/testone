use bevy::prelude::*;
use bevy::solari::prelude::RaytracingMesh3d;

use crate::player::Player;

pub struct LightsPlugin;
impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        // Свет дают только светящиеся объекты: пластина в фонаре и пули
        app.insert_resource(GlobalAmbientLight {
                brightness: 0.0,
                ..default()
            })
            .insert_resource(ClearColor(Color::BLACK))
            .add_systems(Update, spawn_flashlight);
    }
}

/// Готовит меш для Solari: трассировке нужны тангенты.
pub fn raytraced(mesh: impl Into<Mesh>) -> Mesh {
    mesh.into()
        .with_generated_tangents()
        .expect("меш должен поддерживать генерацию тангентов")
}

// 1. Создаем компонент-маркер "Фонарик выдан"
#[derive(Component)]
struct FlashlightEquipped;

/// Solari не поддерживает SpotLight, поэтому фонарь устроен как настоящий:
/// светящаяся пластина в чёрном коробе. Конус луча дают стенки, загораживающие свет.
fn spawn_flashlight(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    // 2. Ищем игрока БЕЗ маркера FlashlightEquipped
    player_query: Query<Entity, (With<Player>, Without<FlashlightEquipped>)>
) {
    for player_entity in player_query.iter() {
        let emitter = materials.add(StandardMaterial {
            base_color: Color::BLACK,
            emissive: LinearRgba::rgb(1.0, 0.95, 0.85) * 1_000_000.0,
            ..default()
        });
        let shell = materials.add(StandardMaterial {
            base_color: Color::BLACK,
            ..default()
        });
        // Solari выбирает сэмплы равномерно по треугольникам источника, поэтому
        // пластина - одна плоскость из двух треугольников, смотрящая вперёд
        let plate = meshes.add(raytraced(Plane3d::new(Vec3::NEG_Z, Vec2::splat(0.15))));
        let back = meshes.add(raytraced(Cuboid::new(0.34, 0.34, 0.02)));
        let horizontal = meshes.add(raytraced(Cuboid::new(0.34, 0.02, 0.54)));
        let vertical = meshes.add(raytraced(Cuboid::new(0.02, 0.34, 0.54)));

        commands.entity(player_entity)
            // 3. Сразу вешаем маркер на игрока, чтобы в следующем кадре Query его не нашел
            .insert(FlashlightEquipped)
            .with_children(|parent| {
                parent.spawn((
                    // Короб перед игроком, луч немного наклонён к полу
                    Transform::from_xyz(0.0, 0.2, -0.4)
                        .looking_at(Vec3::new(0.0, -1.0, -6.0), Vec3::Y),
                    Visibility::default(),
                    Name::new("Flash Light"),
                )).with_children(|rig| {
                    rig.spawn((
                        Mesh3d(plate.clone()),
                        RaytracingMesh3d(plate.clone()),
                        MeshMaterial3d(emitter.clone()),
                        Transform::default(),
                    ));
                    let parts = [
                        (&back, Vec3::new(0.0, 0.0, 0.02)),
                        (&horizontal, Vec3::new(0.0, 0.16, -0.24)),
                        (&horizontal, Vec3::new(0.0, -0.16, -0.24)),
                        (&vertical, Vec3::new(0.16, 0.0, -0.24)),
                        (&vertical, Vec3::new(-0.16, 0.0, -0.24)),
                    ];
                    for (mesh, position) in parts {
                        rig.spawn((
                            Mesh3d(mesh.clone()),
                            RaytracingMesh3d(mesh.clone()),
                            MeshMaterial3d(shell.clone()),
                            Transform::from_translation(position),
                        ));
                    }
                });
            });

        info!("Flashlight spawned for player!"); // В логе должно появиться только ОДИН раз
    }
}
