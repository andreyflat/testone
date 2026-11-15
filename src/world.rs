// src/world.rs - Пол и геометрия уровня
use bevy::prelude::*;
use bevy::solari::prelude::RaytracingMesh3d;
use bevy_rapier3d::prelude::*;

pub struct WorldPlugin;
impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_floor, spawn_level));
    }
}

#[derive(Component)]
pub struct Ground;

const FLOOR_HALF_THICKNESS: f32 = 0.5;

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
        // Верх коллайдера совпадает с мешем пола, иначе тела врагов висели бы над ним.
        // Плита толстая, чтобы упавшие тела не проваливались сквозь неё.
        Collider::compound(vec![(
            Vec3::new(0.0, -FLOOR_HALF_THICKNESS, 0.0),
            Quat::IDENTITY,
            Collider::cuboid(25.0, FLOOR_HALF_THICKNESS, 25.0),
        )]),
        // Трение нужно упавшим телам. Игрок движется своим кодом и от него не зависит
        Friction::coefficient(0.8),
        Restitution {
            coefficient: 0.0, // Никакого отскока
            combine_rule: CoefficientCombineRule::Min,
        },
    ));
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
/// Высота стен периметра. Невидимых продолжений у них нет: в них упирались бы прицел и снаряды.
/// Выпрыгнуть с арены не даёт ограничение в движении игрока.
const BOUNDARY_HEIGHT: f32 = 3.0;
/// Ближняя к камере стена низкая, иначе она закрывала бы игрока у края.
const NEAR_BOUNDARY_HEIGHT: f32 = 0.4;

const BOX: f32 = 0.9; // Обычный прыжок с запасом (потолок около 1.14 м)
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

/// Где отрезок на полу входит в прямоугольник с центром `center` и полуразмерами `half`,
/// как доля длины отрезка от 0 до 1, если входит.
fn segment_enters(from: Vec2, to: Vec2, center: Vec2, half: Vec2) -> Option<f32> {
    let delta = to - from;
    let (mut enter, mut leave) = (0.0_f32, 1.0_f32);
    for axis in 0..2 {
        let (low, high) = (center[axis] - half[axis], center[axis] + half[axis]);
        if delta[axis].abs() < 1e-6 {
            if from[axis] <= low || from[axis] >= high {
                return None;
            }
            continue;
        }
        let (a, b) = ((low - from[axis]) / delta[axis], (high - from[axis]) / delta[axis]);
        enter = enter.max(a.min(b));
        leave = leave.min(a.max(b));
    }
    (enter < leave).then_some(enter)
}

/// Куда идти кругу радиуса `radius` из `from`, чтобы попасть в `to`, если прямой путь
/// загораживает блок уровня: к тому углу блока, через который путь короче. Если путь
/// свободен, возвращает `None`. Высота не учитывается.
pub fn detour(from: Vec3, to: Vec3, radius: f32) -> Option<Vec3> {
    let (start, goal) = (Vec2::new(from.x, from.z), Vec2::new(to.x, to.z));
    // Чуть меньше радиуса: стоящий вплотную к блоку не считается стоящим внутри него
    let grown = Vec2::splat(radius * 0.9);
    let footprint = |b: &Block| (Vec2::new(b.center.x, b.center.z), Vec2::new(b.half.x, b.half.z));
    let (block, _) = LEVEL
        .iter()
        .filter_map(|b| {
            let (center, half) = footprint(b);
            segment_enters(start, goal, center, half + grown).map(|at| (b, at))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))?;

    let (center, half) = footprint(block);
    // Угол с запасом, чтобы, дойдя до него, увидеть путь дальше
    let reach = half + Vec2::splat(radius + 0.25);
    [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)]
        .into_iter()
        .map(|(x, z)| center + reach * Vec2::new(x, z))
        .filter(|&corner| {
            segment_enters(start, corner, center, half + grown).is_none()
                && !is_blocked(Vec3::new(corner.x, 0.0, corner.y), radius * 0.9)
        })
        .min_by(|a, b| {
            let cost = |corner: &Vec2| start.distance(*corner) + corner.distance(goal);
            cost(a).total_cmp(&cost(b))
        })
        .map(|corner| Vec3::new(corner.x, from.y, corner.y))
}

/// Выталкивает круг радиуса `radius` из блоков уровня и возвращает его внутрь арены.
/// Высота не учитывается: так ходят враги, которых анимация ведёт без физики.
pub fn push_out(position: Vec3, radius: f32) -> Vec3 {
    let mut position = position;
    for b in LEVEL {
        let local = Vec2::new(position.x - b.center.x, position.z - b.center.z);
        let half = Vec2::new(b.half.x, b.half.z);
        let nearest = local.clamp(-half, half);
        let away = local - nearest;
        let distance = away.length();
        if distance >= radius {
            continue;
        }
        let push = if distance > 1e-4 {
            // Снаружи блока: от ближайшей точки его края
            away / distance * (radius - distance)
        } else {
            // Внутри блока: к ближайшей грани и ещё на радиус
            let depth = half - local.abs();
            if depth.x < depth.y {
                Vec2::new((depth.x + radius).copysign(local.x), 0.0)
            } else {
                Vec2::new(0.0, (depth.y + radius).copysign(local.y))
            }
        };
        position.x += push.x;
        position.z += push.y;
    }
    let limit = ARENA_HALF_SIZE - radius;
    position.x = position.x.clamp(-limit, limit);
    position.z = position.z.clamp(-limit, limit);
    position
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
        spawn_block(&mut commands, &mut meshes, material, b.center, b.half);
    }

    // Периметр
    let edge = ARENA_HALF_SIZE + BOUNDARY_THICKNESS * 0.5;
    let half_t = BOUNDARY_THICKNESS * 0.5;
    let long = ARENA_HALF_SIZE + BOUNDARY_THICKNESS;
    let walls = [
        (Vec3::new(0.0, 0.0, -edge), Vec3::new(long, 0.0, half_t), BOUNDARY_HEIGHT),
        (Vec3::new(0.0, 0.0, edge), Vec3::new(long, 0.0, half_t), NEAR_BOUNDARY_HEIGHT),
        (Vec3::new(-edge, 0.0, 0.0), Vec3::new(half_t, 0.0, ARENA_HALF_SIZE), BOUNDARY_HEIGHT),
        (Vec3::new(edge, 0.0, 0.0), Vec3::new(half_t, 0.0, ARENA_HALF_SIZE), BOUNDARY_HEIGHT),
    ];
    for (base, half, height) in walls {
        let half = half.with_y(height * 0.5);
        spawn_block(&mut commands, &mut meshes, boundary.clone(), base.with_y(half.y), half);
    }
}

/// Неподвижный блок: меш и коллайдер одного размера.
fn spawn_block(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    center: Vec3,
    half: Vec3,
) {
    let mesh = meshes.add(crate::lights::raytraced(Cuboid::from_size(half * 2.0)));
    commands.spawn((
        Mesh3d(mesh.clone()),
        RaytracingMesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(center),
        RigidBody::Fixed,
        Collider::cuboid(half.x, half.y, half.z),
    ));
}
