use bevy::prelude::*;
use bevy::solari::prelude::RaytracingMesh3d;

use crate::player::Player;
use crate::world::ARENA_HALF_SIZE;

pub struct LightsPlugin;
impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        // Свет дают только светящиеся объекты: пластина в фонаре и пули
        app.insert_resource(GlobalAmbientLight {
                brightness: 0.0,
                ..default()
            })
            .insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, spawn_wall_lamps)
            .add_systems(Update, spawn_flashlight);
    }
}

/// Готовит меш для Solari: трассировке нужны тангенты.
pub fn raytraced(mesh: impl Into<Mesh>) -> Mesh {
    mesh.into()
        .with_generated_tangents()
        .expect("меш должен поддерживать генерацию тангентов")
}

/// Лампы на стенах периметра стоят с таким шагом вдоль стены.
const LAMP_POSITIONS: [f32; 4] = [-15.0, -5.0, 5.0, 15.0];
/// Ширина лампы и её высота на высоких стенах и на низкой ближней.
const LAMP_WIDTH: f32 = 1.2;
const LAMP_HEIGHT: f32 = 0.3;
const LOW_LAMP_HEIGHT: f32 = 0.2;
/// Высота центра лампы над полом: под верхом трёхметровой стены и на низкой ближней стене.
const LAMP_Y: f32 = 2.5;
const LOW_LAMP_Y: f32 = 0.22;
/// Лампа висит на таком расстоянии от стены, чтобы не сливаться с ней.
const LAMP_GAP: f32 = 0.02;
const LAMP_EMISSIVE: f32 = 10_000.0;

/// Лампы на стенах арены: светящиеся панели, смотрящие внутрь. Без них арену освещали
/// только фонарь игрока и снаряды.
///
/// Solari выбирает сэмплы равномерно по треугольникам источника, поэтому панель - плоский
/// прямоугольник из двух треугольников, а не коробка.
fn spawn_wall_lamps(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let material = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(1.0, 0.9, 0.75) * LAMP_EMISSIVE,
        ..default()
    });
    // Прямоугольник лежит в плоскости XY и светит вдоль +Z
    let high = meshes.add(raytraced(Rectangle::new(LAMP_WIDTH, LAMP_HEIGHT)));
    let low = meshes.add(raytraced(Rectangle::new(LAMP_WIDTH, LOW_LAMP_HEIGHT)));
    let edge = ARENA_HALF_SIZE - LAMP_GAP;
    use std::f32::consts::{FRAC_PI_2, PI};
    for along in LAMP_POSITIONS {
        // Стена, поворот панели внутрь арены, меш и высота
        let lamps = [
            (Vec3::new(along, LAMP_Y, -edge), 0.0, &high),
            (Vec3::new(-edge, LAMP_Y, along), FRAC_PI_2, &high),
            (Vec3::new(edge, LAMP_Y, along), -FRAC_PI_2, &high),
            (Vec3::new(along, LOW_LAMP_Y, edge), PI, &low),
        ];
        for (position, turn, mesh) in lamps {
            commands.spawn((
                Mesh3d(mesh.clone()),
                RaytracingMesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(position).with_rotation(Quat::from_rotation_y(turn)),
                Name::new("Wall Lamp"),
            ));
        }
    }
}

// 1. Создаем компонент-маркер "Фонарик выдан"
#[derive(Component)]
struct FlashlightEquipped;

/// Половина стороны светящейся пластины фонаря.
const TORCH_PLATE: f32 = 0.04;
/// Головка фонаря: внешняя сторона, толщина стенок и глубина бленды перед пластиной.
/// Чем глубже бленда, тем уже луч: сейчас он расходится примерно на 30 градусов от оси.
const TORCH_HEAD: f32 = 0.12;
const TORCH_WALL: f32 = 0.01;
const TORCH_HOOD: f32 = 0.15;
/// Рукоять за головкой: сторона и длина.
const TORCH_GRIP: f32 = 0.05;
const TORCH_GRIP_LENGTH: f32 = 0.17;
/// Пластина маленькая, поэтому светит очень ярко: общий поток тот же, что давала бы
/// пластина 30 на 30 см с яркостью 1 000 000.
const TORCH_EMISSIVE: f32 = 14_000_000.0;

/// Solari не поддерживает SpotLight, поэтому фонарь устроен как настоящий:
/// светящаяся пластина в глубине чёрной головки. Конус луча дают её стенки, загораживающие свет.
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
            emissive: LinearRgba::rgb(1.0, 0.95, 0.85) * TORCH_EMISSIVE,
            ..default()
        });
        let shell = materials.add(StandardMaterial {
            base_color: Color::BLACK,
            ..default()
        });
        // Solari выбирает сэмплы равномерно по треугольникам источника, поэтому
        // пластина - одна плоскость из двух треугольников, смотрящая вперёд
        let plate = meshes.add(raytraced(Plane3d::new(Vec3::NEG_Z, Vec2::splat(TORCH_PLATE))));
        let back = meshes.add(raytraced(Cuboid::new(TORCH_HEAD, TORCH_HEAD, TORCH_WALL)));
        let horizontal = meshes.add(raytraced(Cuboid::new(TORCH_HEAD, TORCH_WALL, TORCH_HOOD)));
        let vertical = meshes.add(raytraced(Cuboid::new(TORCH_WALL, TORCH_HEAD, TORCH_HOOD)));
        let grip = meshes.add(raytraced(Cuboid::new(TORCH_GRIP, TORCH_GRIP, TORCH_GRIP_LENGTH)));
        // Стенки бленды стоят по краям головки и уходят вперёд от пластины
        let side = (TORCH_HEAD - TORCH_WALL) * 0.5;
        let hood = -TORCH_HOOD * 0.5;

        commands.entity(player_entity)
            // 3. Сразу вешаем маркер на игрока, чтобы в следующем кадре Query его не нашел
            .insert(FlashlightEquipped)
            .with_children(|parent| {
                parent.spawn((
                    // Фонарь в руках перед грудью, луч немного наклонён к полу
                    Transform::from_xyz(0.0, 0.25, -0.5)
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
                        (&back, Vec3::new(0.0, 0.0, TORCH_WALL)),
                        (&horizontal, Vec3::new(0.0, side, hood)),
                        (&horizontal, Vec3::new(0.0, -side, hood)),
                        (&vertical, Vec3::new(side, 0.0, hood)),
                        (&vertical, Vec3::new(-side, 0.0, hood)),
                        (&grip, Vec3::new(0.0, 0.0, TORCH_WALL * 1.5 + TORCH_GRIP_LENGTH * 0.5)),
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
