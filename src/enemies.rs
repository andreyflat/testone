// src/enemies.rs - Враги, бесконечные волны, урон при касании и смерть игрока
//
// Тело врага - человечек из коробок с рэгдоллом из крейта `dummies` (перенесён из pistol_game).
// Пока враг цел, его ведёт анимация; от сильного удара или смерти тело переходит к физике.
// Здесь решается, куда враг идёт, что с ним делает попадание и когда убирать тела.
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use biped::Biped;
use biped::body::Part;
use dummies::{AnimateSystems, Animation, DamageSystems, DummiesPlugin, DummyAssets, Protection, Ragdoll, Steer, Struggle, spawn_dummy};
use game_core::combat::{self, BodyPart, Damage};
use crate::player::Player;
use crate::world::{ARENA_HALF_SIZE, is_blocked, push_out};

pub use game_core::collision::{ENEMY_GROUP, PLAYER_GROUP, RAGDOLL_GROUP};

pub const PLAYER_MAX_HEALTH: f32 = 100.0;

/// Радиус, которым враг упирается в стены, других врагов и достаёт игрока.
pub const ENEMY_RADIUS: f32 = 0.35;
/// Рост врага: выше этого он не достаёт ни игрока, ни игрок его перчаткой.
pub const ENEMY_HEIGHT: f32 = 1.8;
/// Пока броня цела, попадания ниже этой высоты (в ноги) урона не наносят:
/// выстрел с земли броню не достаёт, нужен выстрел в прыжке или взрыв.
const ARMOR_MIN_Y: f32 = 1.1;
const ARMOR_HEALTH: f32 = 100.0;
/// Попадание в броню только слегка дёргает врага: такая доля толчка доходит до тела.
const ARMOR_IMPACT: f32 = 0.3;
const ARMORED_SPEED_FACTOR: f32 = 0.7;

// Обход препятствий: враг, который почти не сдвинулся за время проверки, шагает вбок
const STUCK_CHECK_INTERVAL: f32 = 0.5;
const STUCK_MOVED_FRACTION: f32 = 0.25;
const SIDESTEP_DURATION: f32 = 1.0;
const NEAR_PLAYER_DISTANCE: f32 = 1.5;
const PLAYER_RADIUS: f32 = 0.3;
const PLAYER_HALF_HEIGHT: f32 = 0.8;
const CONTACT_DAMAGE: f32 = 10.0;
const CONTACT_COOLDOWN: f32 = 0.5;

const WAVE_SPAWN_RADIUS: f32 = 15.0;
const WAVE_DELAY: f32 = 2.0;

/// Обмякшее тело лежит столько секунд, потом исчезает.
const CORPSE_LINGER: f32 = 6.0;
/// Больше тел одновременно не бывает: самые старые убираются раньше срока.
/// Каждое тело - 14 связанных суставами твёрдых тел, это дорого для физики.
const MAX_CORPSES: usize = 14;

/// Взрыв отбрасывает части тела с такой скоростью вплотную к центру, м/с.
const BLAST_SPEED: f32 = 6.0;

pub struct EnemiesPlugin;
impl Plugin for EnemiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DummiesPlugin)
            .init_resource::<WaveState>()
            .add_systems(Update, (
                wave_system,
                track_enemies,
                steer_enemies.before(AnimateSystems),
                enemy_contact_damage,
                player_death_system,
            ).chain())
            .add_systems(Update, (bury_dead, clear_corpses).chain().after(DamageSystems));
    }
}

#[derive(Resource)]
pub struct WaveState {
    pub wave: u32,
    pub delay: f32,
}

impl Default for WaveState {
    fn default() -> Self {
        Self {
            wave: 0,
            delay: WAVE_DELAY,
        }
    }
}

/// Живой враг. Висит на корне тела; у убитого снимается, тело остаётся как `Corpse`.
#[derive(Component)]
pub struct Enemy {
    pub speed: f32,
    pub attack_cooldown: f32,
    pub stuck_timer: f32,
    pub last_position: Vec3,
    pub sidestep_timer: f32,
    pub sidestep_side: f32,
    /// Где враг стоит на полу, в мире. Обновляется каждый кадр по его тазу.
    pub position: Vec3,
    /// На ногах и под управлением анимации: не сбит с ног и не шатается от удара.
    pub standing: bool,
}

/// Броня бронированного врага. Пока она цела, тело неуязвимо.
#[derive(Component)]
pub struct ArmorPlating {
    pub current: f32,
    /// Пластины и шлем: исчезают, когда броня сбита.
    pub pieces: Vec<Entity>,
}

/// Тело убитого врага: сколько секунд оно уже лежит обмякшим.
#[derive(Component)]
pub struct Corpse {
    limp_for: f32,
    /// Порядок гибели: при переполнении первыми убираются самые старые.
    order: u32,
}

/// Здоровье игрока. У врагов своё, внутри `dummies`.
#[derive(Component)]
pub struct Health {
    pub current: f32,
}

impl Health {
    pub fn new(max_health: f32) -> Self {
        Self {
            current: max_health,
        }
    }

    pub fn take_damage(&mut self, damage: f32) {
        self.current = (self.current - damage).max(0.0);
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }
}

/// Всё, что нужно оружию, чтобы попасть во врага: от коллайдера к телу, броня, урон и толчки.
#[derive(SystemParam)]
pub struct EnemyHits<'w, 's> {
    commands: Commands<'w, 's>,
    damage: MessageWriter<'w, Damage>,
    body_parts: Query<'w, 's, &'static ChildOf, With<BodyPart>>,
    armor_pieces: Query<'w, 's, (&'static combat::Armor, &'static ChildOf)>,
    platings: Query<'w, 's, &'static mut ArmorPlating>,
    bipeds: Query<'w, 's, (Entity, &'static Biped)>,
    places: Query<'w, 's, &'static GlobalTransform>,
    impulses: Query<'w, 's, &'static mut ExternalImpulse, With<BodyPart>>,
}

impl EnemyHits<'_, '_> {
    /// Чьё это тело и какая его часть, если коллайдер - часть тела или пластина брони на ней.
    pub fn target(&self, collider: Entity) -> Option<(Entity, Entity)> {
        if let Ok(parent) = self.body_parts.get(collider) {
            return Some((parent.parent(), collider));
        }
        let (armor, parent) = self.armor_pieces.get(collider).ok()?;
        Some((armor.dummy, parent.parent()))
    }

    /// Ближайшая к точке часть тела и где она.
    pub fn nearest_part(&self, body: Entity, point: Vec3) -> Option<(Entity, Vec3)> {
        let (_, biped) = self.bipeds.get(body).ok()?;
        biped
            .parts
            .iter()
            .filter_map(|&part| Some((part, self.places.get(part).ok()?.translation())))
            .min_by(|a, b| a.1.distance_squared(point).total_cmp(&b.1.distance_squared(point)))
    }

    /// Попадание в часть `part` тела `body` в точке `point`, летевшее в сторону `direction`.
    /// `impact` от 0 до 1 - сила толчка: слабый только дёргает врага, сильный сбивает с шага.
    ///
    /// Пока броня цела, урон принимает она, а попадание ниже брони не проходит вовсе.
    /// `hit_y = None` - взрыв: он достаёт броню с любой высоты.
    pub fn hit(
        &mut self,
        body: Entity,
        part: Entity,
        point: Vec3,
        direction: Vec3,
        amount: f32,
        impact: f32,
        hit_y: Option<f32>,
    ) {
        let (amount, impact, blunt) = match self.platings.get_mut(body) {
            Ok(mut plating) => {
                if hit_y.is_none_or(|y| y >= ARMOR_MIN_Y) {
                    plating.current -= amount;
                    if plating.current <= 0.0 {
                        for &piece in &plating.pieces {
                            self.commands.entity(piece).try_despawn();
                        }
                        self.commands.entity(body).remove::<ArmorPlating>();
                        info!("Броня сбита");
                    }
                }
                (0.0, impact * ARMOR_IMPACT, true)
            }
            Err(_) => (amount, impact, false),
        };
        self.damage.write(Damage {
            dummy: body,
            part,
            point,
            direction,
            amount,
            grenade: false,
            lethal: false,
            blunt,
            impact,
            through: false,
            bleed: false,
        });
    }

    /// Взрыв в точке `origin`: урон каждому, кого достаёт, живому или уже упавшему,
    /// и толчок от центра всем частям его тела, чтобы его отбросило.
    pub fn explode(&mut self, origin: Vec3, radius: f32, amount: f32) {
        let bodies: Vec<(Entity, [Entity; biped::body::PART_COUNT])> =
            self.bipeds.iter().map(|(body, biped)| (body, biped.parts)).collect();
        for (body, parts) in bodies {
            let Some((part, place)) = self.nearest_part(body, origin) else {
                continue;
            };
            // Части тела - коробки до 0.2 м от центра до края
            if place.distance(origin) - 0.2 > radius {
                continue;
            }
            let away = (place - origin).try_normalize().unwrap_or(Vec3::Y);
            self.hit(body, part, place, away, amount, 1.0, None);

            for (index, part) in parts.into_iter().enumerate() {
                let (Ok(place), Ok(mut impulse)) = (self.places.get(part), self.impulses.get_mut(part)) else {
                    continue;
                };
                let offset = place.translation() - origin;
                let falloff = (1.0 - offset.length() / (radius * 2.0)).max(0.0);
                // Немного вверх, чтобы тело подбросило, а не протащило по полу
                let direction = (offset.normalize_or_zero() + Vec3::Y * 0.5).normalize_or_zero();
                impulse.impulse += direction * Part::from_index(index).mass() * BLAST_SPEED * falloff;
            }
        }
    }
}

/// Когда врагов не осталось, после паузы запускает следующую, более сложную волну.
fn wave_system(
    time: Res<Time>,
    mut wave_state: ResMut<WaveState>,
    mut commands: Commands,
    assets: Res<DummyAssets>,
    enemies: Query<(), With<Enemy>>,
    player: Query<&Transform, With<Player>>,
) {
    if !enemies.is_empty() {
        return;
    }
    wave_state.delay -= time.delta_secs();
    if wave_state.delay > 0.0 {
        return;
    }
    let Ok(player) = player.single() else {
        return;
    };

    wave_state.wave += 1;
    wave_state.delay = WAVE_DELAY;
    let wave = wave_state.wave;
    let count = 1 + wave * 2;
    let armored_count = wave - 1; // С каждой следующей волной на одного бронированного больше
    let speed = (2.5 + wave as f32 * 0.25).min(7.0);
    info!("Волна {wave}: обычных {count}, бронированных {armored_count}, скорость {speed:.2}");

    // Золотой угол равномерно раскладывает врагов по кольцу вокруг игрока
    const GOLDEN_ANGLE: f32 = 2.399_963;
    for i in 0..count + armored_count {
        let position = spawn_point(player.translation, (i + wave) as f32 * GOLDEN_ANGLE);
        let armored = i >= count;
        // Лицом к игроку
        let to_player = player.translation - position;
        let facing = to_player.x.atan2(to_player.z);
        let (name, protection, speed) = if armored {
            ("Armored Enemy", Protection::Heavy, speed * ARMORED_SPEED_FACTOR)
        } else {
            ("Enemy", Protection::None, speed)
        };
        let spawned = spawn_dummy(&mut commands, &assets, name, protection, position, facing);
        let mut enemy = commands.entity(spawned.dummy);
        enemy.insert(Enemy {
            speed,
            attack_cooldown: 0.0,
            stuck_timer: STUCK_CHECK_INTERVAL,
            last_position: position,
            sidestep_timer: 0.0,
            sidestep_side: 1.0,
            position,
            standing: true,
        });
        if armored {
            enemy.insert(ArmorPlating {
                current: ARMOR_HEALTH,
                pieces: spawned.armor,
            });
        }
    }
}

/// Точка на кольце вокруг игрока, не занятая стенами и платформами.
/// Если угол упирается в геометрию, поворачивает дальше и сужает кольцо.
fn spawn_point(player: Vec3, angle: f32) -> Vec3 {
    const MARGIN: f32 = ENEMY_RADIUS * 2.0;
    let limit = ARENA_HALF_SIZE - MARGIN;
    let candidate = |angle: f32, radius: f32| Vec3::new(
        (player.x + angle.cos() * radius).clamp(-limit, limit),
        0.0,
        (player.z + angle.sin() * radius).clamp(-limit, limit),
    );
    for radius in [WAVE_SPAWN_RADIUS, WAVE_SPAWN_RADIUS * 0.75, WAVE_SPAWN_RADIUS * 0.5] {
        for step in 0..12 {
            let position = candidate(angle + step as f32 * 0.3, radius);
            if !is_blocked(position, MARGIN) {
                return position;
            }
        }
    }
    candidate(angle, WAVE_SPAWN_RADIUS)
}

/// Запоминает, где каждый враг стоит и на ногах ли он: по тазу, куда бы тело ни унесла физика.
/// Таз лежит в пространстве корня тела, а корень не повёрнут. Мировое положение таза здесь
/// не годится: у только что появившегося врага оно ещё не посчитано и равно нулю.
fn track_enemies(
    mut enemies: Query<(&mut Enemy, &Biped, &Animation, &Ragdoll, &Transform)>,
    parts: Query<&Transform, With<BodyPart>>,
) {
    for (mut enemy, biped, animation, ragdoll, origin) in enemies.iter_mut() {
        if let Ok(pelvis) = parts.get(biped.part(Part::Guts)) {
            enemy.position = (origin.translation + pelvis.translation).with_y(0.0);
        }
        enemy.standing = animation.active && !ragdoll.dead() && !ragdoll.knocked();
    }
}

/// Ведёт врагов к игроку. Идёт и поворачивается сама анимация, сюда она отдаёт только
/// направление и скорость. Стен для анимированного тела нет, поэтому из них и друг из друга
/// врагов выталкивает этот же код.
fn steer_enemies(
    time: Res<Time>,
    player: Query<&Transform, With<Player>>,
    mut enemies: Query<(Entity, &mut Enemy, &mut Animation, &Transform), Without<Player>>,
    mut crowd: Local<Vec<(Entity, Vec3)>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let dt = time.delta_secs();

    crowd.clear();
    for (entity, mut enemy, mut animation, origin) in enemies.iter_mut() {
        if !enemy.standing {
            // Телом владеет физика: враг шатается или лежит, пока не оправится
            animation.steer.speed = 0.0;
            continue;
        }
        // Анимация считает положение от точки появления врага
        let position = origin.translation + animation.position();
        let flat_offset = (player.translation - position).with_y(0.0);
        let distance = flat_offset.length();
        let direction = flat_offset.normalize_or_zero();

        enemy.stuck_timer -= dt;
        if enemy.stuck_timer <= 0.0 {
            let moved = position.distance(enemy.last_position);
            let expected = enemy.speed * STUCK_CHECK_INTERVAL;
            if moved < expected * STUCK_MOVED_FRACTION
                && distance > NEAR_PLAYER_DISTANCE
                && enemy.sidestep_timer <= 0.0
            {
                enemy.sidestep_timer = SIDESTEP_DURATION;
                enemy.sidestep_side = -enemy.sidestep_side;
            }
            enemy.stuck_timer = STUCK_CHECK_INTERVAL;
            enemy.last_position = position;
        }

        let heading = if enemy.sidestep_timer > 0.0 {
            enemy.sidestep_timer -= dt;
            Vec3::new(-direction.z, 0.0, direction.x) * enemy.sidestep_side
        } else {
            direction
        };
        // Вплотную к игроку останавливается и только поворачивается к нему, а не входит в него
        let reached = distance < (ENEMY_RADIUS + PLAYER_RADIUS) * 0.8;
        animation.steer = Steer {
            speed: if reached { 0.0 } else { enemy.speed },
            heading: (heading != Vec3::ZERO).then(|| heading.x.atan2(heading.z)),
            floor: Some(0.0),
            ..default()
        };

        let kept = push_out(position, ENEMY_RADIUS);
        animation.nudge(kept - position);
        crowd.push((entity, kept));
    }

    // Враги расталкивают друг друга, чтобы не сливаться в одного
    let spacing = ENEMY_RADIUS * 2.0;
    for i in 0..crowd.len() {
        let mut push = Vec3::ZERO;
        for j in 0..crowd.len() {
            if i == j {
                continue;
            }
            let apart = (crowd[i].1 - crowd[j].1).with_y(0.0);
            let distance = apart.length();
            if distance < spacing {
                // Стоящих точно друг в друге разводит в стороны по порядку в списке
                let away = apart.try_normalize().unwrap_or(if i < j { Vec3::X } else { Vec3::NEG_X });
                push += away * (spacing - distance) * 0.5;
            }
        }
        if push != Vec3::ZERO
            && let Ok((_, _, mut animation, _)) = enemies.get_mut(crowd[i].0)
        {
            // Не всё сразу: резкий сдвиг был бы виден как рывок
            animation.nudge(push * (10.0 * dt).min(1.0));
        }
    }
}

/// Урон считается по пересечению объёмов, а не по физике. Бьёт только враг, стоящий на ногах.
fn enemy_contact_damage(
    time: Res<Time>,
    mut player: Query<(&Transform, &mut Health), With<Player>>,
    mut enemies: Query<&mut Enemy>,
) {
    let Ok((player_transform, mut health)) = player.single_mut() else {
        return;
    };

    for mut enemy in enemies.iter_mut() {
        enemy.attack_cooldown = (enemy.attack_cooldown - time.delta_secs()).max(0.0);
        if !enemy.standing {
            continue;
        }

        let offset = player_transform.translation - enemy.position;
        let touching = Vec2::new(offset.x, offset.z).length() < ENEMY_RADIUS + PLAYER_RADIUS
            && player_transform.translation.y - PLAYER_HALF_HEIGHT < ENEMY_HEIGHT;

        if touching && enemy.attack_cooldown <= 0.0 {
            enemy.attack_cooldown = CONTACT_COOLDOWN;
            health.take_damage(CONTACT_DAMAGE);
            info!("Игрок получил урон, здоровье: {}", health.current);
        }
    }
}

/// Убитый враг перестаёт быть врагом: волна его больше не ждёт, а тело доживает как рэгдолл.
fn bury_dead(
    mut commands: Commands,
    enemies: Query<(Entity, &Ragdoll), With<Enemy>>,
    mut buried: Local<u32>,
) {
    for (entity, ragdoll) in enemies.iter() {
        if ragdoll.dead() {
            *buried += 1;
            commands
                .entity(entity)
                .remove::<(Enemy, ArmorPlating)>()
                .insert(Corpse { limp_for: 0.0, order: *buried });
        }
    }
}

/// Убирает тела: обмякшие через `CORPSE_LINGER`, а лишние сверх `MAX_CORPSES` сразу.
fn clear_corpses(
    time: Res<Time>,
    mut commands: Commands,
    mut corpses: Query<(Entity, &mut Corpse, Has<Struggle>)>,
) {
    let excess = corpses.iter().count().saturating_sub(MAX_CORPSES) as u32;
    let oldest = corpses.iter().map(|(_, corpse, _)| corpse.order).min().unwrap_or(0);
    for (entity, mut corpse, struggling) in corpses.iter_mut() {
        // Пока тело борется, оно не лежит
        if !struggling {
            corpse.limp_for += time.delta_secs();
        }
        if corpse.limp_for > CORPSE_LINGER || corpse.order < oldest + excess {
            commands.entity(entity).try_despawn();
        }
    }
}

/// Смерть запускает забег заново с первой волны.
fn player_death_system(
    mut commands: Commands,
    mut player: Query<(&mut Transform, &mut crate::player::Velocity, &mut Health), With<Player>>,
    bodies: Query<Entity, Or<(With<Enemy>, With<Corpse>)>>,
    mut wave_state: ResMut<WaveState>,
) {
    let Ok((mut transform, mut velocity, mut health)) = player.single_mut() else {
        return;
    };
    if !health.is_dead() {
        return;
    }

    info!("Игрок погиб на волне {}. Новый забег", wave_state.wave);
    for entity in bodies.iter() {
        commands.entity(entity).try_despawn();
    }
    transform.translation = Vec3::new(0.0, 3.0, 0.0);
    velocity.0 = Vec3::ZERO;
    health.current = PLAYER_MAX_HEALTH;
    *wave_state = WaveState::default();
}
