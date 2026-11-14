// src/world.rs - Восстановленная версия с поворотом игрока к курсору
use bevy::prelude::*;
use bevy::solari::prelude::RaytracingMesh3d;
use bevy_rapier3d::prelude::*;
use crate::player::{Player, WishDirection, PlayerCamera};

pub struct WorldPlugin;
impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_floor, spawn_level))
           .add_systems(Update, draw_cursor);
    }
}

#[derive(Component)]
pub struct Ground;

fn spawn_floor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(crate::lights::raytraced(Plane3d::default().mesh().size(50.0, 50.0)));
    let entity = commands.spawn((
        Mesh3d(mesh.clone()), // Увеличили арену
        RaytracingMesh3d(mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.5, 0.5, 0.5),
            perceptual_roughness: 0.9,
            metallic: 0.0,
            ..default()
        })),
        Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
        Ground,
    )).id();
    
    // Добавляем физическую коллизию для земли
    commands.entity(entity).insert((
        RigidBody::Fixed,
        // Коллайдер пола - тонкий прямоугольник
        Collider::cuboid(25.0, 0.1, 25.0), // width, height/2, depth
        Friction {
            coefficient: 0.0, // Убираем трение для Quake-физики
            combine_rule: CoefficientCombineRule::Min,
        },
        Restitution {
            coefficient: 0.0, // Никакого отскока
            combine_rule: CoefficientCombineRule::Min,
        },
    ));
}

pub fn draw_cursor(
    camera_query: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    ground: Query<&GlobalTransform, With<Ground>>,
    windows: Query<&Window>,
    mut gizmos: Gizmos,
    mut player_query: Query<(&mut Transform, &WishDirection), With<Player>>,
) {
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let ground_transform = if let Some(transform) = ground.iter().next() {
        transform
    } else {
        return;
    };

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    // Вычисляем луч из камеры в мир на основе позиции курсора
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    // Вычисляем точку пересечения луча с плоскостью земли
    let Some(distance) = ray.intersect_plane(
        ground_transform.translation(),
        InfinitePlane3d::new(ground_transform.up())
    ) else {
        return;
    };
    let point = ray.get_point(distance);

    // Рисуем круг чуть выше плоскости земли в этой позиции
    gizmos.circle(
        Isometry3d::new(
            point + ground_transform.up() * 0.01,
            Quat::from_rotation_arc(Vec3::Z, ground_transform.up().as_vec3()),
        ),
        0.2,
        Color::WHITE,
    );

    // Поворачиваем игрока в направлении точки пересечения
    if let Ok((mut player_transform, wish_dir)) = player_query.single_mut() {
        let target_point = point;
        let current_pos = player_transform.translation;
        
        // Получаем направление только в горизонтальной плоскости
        let direction = Vec3::new(
            target_point.x - current_pos.x,
            0.0, // Игнорируем вертикальную составляющую
            target_point.z - current_pos.z,
        ).normalize();

        // Стандартное направление "вперед" в Bevy
        let forward = Vec3::NEG_Z;

        if direction != Vec3::ZERO {
            // Создаем поворот от вектора вперед к направлению цели
            let rotation = Quat::from_rotation_arc(forward, direction);
            player_transform.rotation = rotation;
        } else if wish_dir.0 != Vec3::ZERO {
            // Если нет направления к курсору, но игрок движется, поворачиваем его в направлении движения
            let rotation = Quat::from_rotation_arc(forward, wish_dir.0);
            player_transform.rotation = rotation;
        }
    }
}

/// Назначение блока определяет его цвет.
#[derive(Clone, Copy)]
enum BlockKind {
    /// Перегородка внутри арены: выше прыжка, забраться можно только отталкиваясь от стен
    Partition,
    /// Ящик и платформа, на которые можно запрыгнуть
    Platform,
}

struct Block {
    center: Vec3,
    half: Vec3,
    kind: BlockKind,
}

const fn block(x: f32, z: f32, half_x: f32, height: f32, half_z: f32, kind: BlockKind) -> Block {
    Block {
        center: Vec3::new(x, height * 0.5, z),
        half: Vec3::new(half_x, height * 0.5, half_z),
        kind,
    }
}

/// Внутренний край стен совпадает с краем пола.
pub const ARENA_HALF_SIZE: f32 = 25.0;
const BOUNDARY_THICKNESS: f32 = 0.5;
/// Стены периметра выше любого прыжка, чтобы с арены нельзя было выпрыгнуть.
const BOUNDARY_HEIGHT: f32 = 8.0;
/// Видимая высота стен. Ближняя к камере стена низкая, иначе она закрывала бы игрока у края.
const BOUNDARY_VISIBLE_HEIGHT: f32 = 3.0;
const NEAR_BOUNDARY_VISIBLE_HEIGHT: f32 = 0.4;

const BOX: f32 = 0.9; // Обычный прыжок с запасом (потолок около 1.25 м)
const LEDGE: f32 = 1.8; // Обычным прыжком только с ящика рядом
const WALL: f32 = 2.0; // Перегородка: нужен прыжок от стены
const TOWER: f32 = 3.0; // Нужна цепочка прыжков от стен

use BlockKind::{Partition as P, Platform as B};

/// Карта арены. X вправо, Z к камере. Центр оставлен пустым под точку появления игрока.
const LEVEL: &[Block] = &[
    // Укрытия вокруг центра: пули с земли в них упираются
    block(0.0, -8.0, 2.0, BOX, 0.5, B),
    block(0.0, 8.0, 2.0, BOX, 0.5, B),
    block(-8.0, 0.0, 0.5, BOX, 2.0, B),
    block(8.0, 0.0, 0.5, BOX, 2.0, B),

    // Северо-запад: угол из перегородок и лестница ящик -> уступ
    block(-12.0, -8.5, 4.0, WALL, 0.25, P),
    block(-8.25, -12.0, 0.25, WALL, 3.75, P),
    block(-15.5, -15.5, 1.0, BOX, 1.0, B),
    block(-19.0, -19.0, 2.5, LEDGE, 2.5, B),

    // Северо-восток: шахта для прыжков между стенами, в конце высокая площадка
    block(12.0, -14.0, 0.25, TOWER + 0.5, 3.0, P),
    block(15.5, -14.0, 0.25, TOWER + 0.5, 3.0, P),
    block(13.75, -18.5, 2.0, TOWER, 1.5, B),
    // Угловая башня у стен периметра: от них тоже можно отталкиваться
    block(22.0, -22.0, 3.0, TOWER, 3.0, B),
    block(18.0, -22.0, 1.0, LEDGE, 1.0, B),

    // Юго-восток: ступени разной высоты
    block(10.0, 11.0, 1.5, BOX, 1.5, B),
    block(13.5, 11.0, 2.0, LEDGE, 1.5, B),
    block(13.5, 14.5, 2.0, BOX, 2.0, B),
    block(18.5, 16.0, 0.25, WALL, 5.0, P),

    // Юго-запад: перегородка с проходом и разбросанные ящики
    block(-10.0, 9.0, 0.25, WALL, 3.0, P),
    block(-10.0, 19.5, 0.25, WALL, 3.0, P),
    block(-17.0, 12.0, 4.0, WALL, 0.25, P),
    block(-15.0, 18.0, 1.0, BOX, 1.0, B),
    block(-20.0, 7.0, 1.5, LEDGE, 1.5, B),
    block(-5.0, 16.0, 1.0, BOX, 1.0, B),
];

/// Занята ли точка геометрией уровня с учётом отступа. Высота не учитывается.
pub fn is_blocked(position: Vec3, margin: f32) -> bool {
    position.x.abs() > ARENA_HALF_SIZE - margin
        || position.z.abs() > ARENA_HALF_SIZE - margin
        || LEVEL.iter().any(|b| {
            (position.x - b.center.x).abs() < b.half.x + margin
                && (position.z - b.center.z).abs() < b.half.z + margin
        })
}

fn spawn_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Матовые материалы: блики от глянцевых поверхностей дают сильный шум в отражённом свете
    let mut matte = |r, g, b| materials.add(StandardMaterial {
        base_color: Color::srgb(r, g, b),
        perceptual_roughness: 0.9,
        metallic: 0.0,
        ..default()
    });
    let boundary = matte(0.35, 0.33, 0.3);
    let partition = matte(0.6, 0.55, 0.45);
    let platform = matte(0.2, 0.3, 0.8);

    for b in LEVEL {
        let material = match b.kind {
            BlockKind::Partition => partition.clone(),
            BlockKind::Platform => platform.clone(),
        };
        spawn_block(&mut commands, &mut meshes, material, b.center, b.half, b.half);
    }

    // Периметр: коллайдер во всю высоту, меш может быть ниже
    let edge = ARENA_HALF_SIZE + BOUNDARY_THICKNESS * 0.5;
    let half_t = BOUNDARY_THICKNESS * 0.5;
    let long = ARENA_HALF_SIZE + BOUNDARY_THICKNESS;
    let walls = [
        (Vec3::new(0.0, 0.0, -edge), Vec3::new(long, 0.0, half_t), BOUNDARY_VISIBLE_HEIGHT),
        (Vec3::new(0.0, 0.0, edge), Vec3::new(long, 0.0, half_t), NEAR_BOUNDARY_VISIBLE_HEIGHT),
        (Vec3::new(-edge, 0.0, 0.0), Vec3::new(half_t, 0.0, ARENA_HALF_SIZE), BOUNDARY_VISIBLE_HEIGHT),
        (Vec3::new(edge, 0.0, 0.0), Vec3::new(half_t, 0.0, ARENA_HALF_SIZE), BOUNDARY_VISIBLE_HEIGHT),
    ];
    for (base, half, visible_height) in walls {
        let collider_half = half.with_y(BOUNDARY_HEIGHT * 0.5);
        let mesh_half = half.with_y(visible_height * 0.5);
        let center = base.with_y(collider_half.y);
        spawn_block(&mut commands, &mut meshes, boundary.clone(), center, collider_half, mesh_half);
    }
}

/// Неподвижный блок. Меш стоит на том же полу, что и коллайдер, но может быть ниже него.
fn spawn_block(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    center: Vec3,
    collider_half: Vec3,
    mesh_half: Vec3,
) {
    let mesh = meshes.add(crate::lights::raytraced(Cuboid::from_size(mesh_half * 2.0)));
    let floor = center.y - collider_half.y;
    commands
        .spawn((
            Transform::from_translation(center),
            Visibility::default(),
            RigidBody::Fixed,
            Collider::cuboid(collider_half.x, collider_half.y, collider_half.z),
        ))
        .with_child((
            Mesh3d(mesh.clone()),
            RaytracingMesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_xyz(0.0, floor + mesh_half.y - center.y, 0.0),
        ));
}
