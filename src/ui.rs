// src/ui.rs - Надписи поверх игры: кадры в секунду, фраги, оружие, здоровье и броня игрока
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use crate::enemies::{Frags, Health};
use crate::player::Player;
use crate::weapons::{Ammo, Weapon, WeaponKind};

const MARGIN: f32 = 16.0;
const TEXT_COLOR: Color = Color::srgb(0.95, 1.0, 0.95);
const HEALTH_COLOR: Color = Color::srgb(1.0, 0.45, 0.4);
const ARMOR_COLOR: Color = Color::srgb(0.5, 0.75, 1.0);
const NOTICE_COLOR: Color = Color::srgb(1.0, 0.95, 0.6);
/// Тень под буквами, чтобы надписи читались на светлом пятне фонаря.
const SHADOW_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.8);
const SMALL: f32 = 18.0;
const LARGE: f32 = 30.0;

pub struct UiPlugin;
impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Notices>()
            .add_systems(Startup, spawn_hud)
            .add_systems(Update, update_hud);
    }
}

/// Сколько секунд уведомление висит на экране и сколько их видно одновременно.
const NOTICE_TIME: f32 = 2.5;
const NOTICE_COUNT: usize = 4;

/// Уведомления о подобранном: что именно и сколько. Новые снизу, старые гаснут.
#[derive(Resource, Default)]
pub struct Notices(Vec<(String, f32)>);

impl Notices {
    pub fn show(&mut self, text: String) {
        self.0.push((text, NOTICE_TIME));
        if self.0.len() > NOTICE_COUNT {
            self.0.remove(0);
        }
    }
}

/// Какую величину показывает надпись.
#[derive(Component, Clone, Copy)]
enum Readout {
    Fps,
    Weapon,
    Health,
    Armor,
    Frags,
    Ammo,
    Notices,
}

fn spawn_hud(mut commands: Commands) {
    let label = |readout: Readout, size: f32, color: Color| {
        (
            Text::new(""),
            TextFont { font_size: size.into(), ..default() },
            TextColor(color),
            TextShadow { offset: Vec2::splat(2.0), color: SHADOW_COLOR },
            readout,
        )
    };
    // Слева вверху - кадры в секунду
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(MARGIN),
            top: Val::Px(MARGIN),
            ..default()
        },
        label(Readout::Fps, SMALL, TEXT_COLOR),
    ));
    // Справа вверху - убитые за забег
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(MARGIN),
            top: Val::Px(MARGIN),
            ..default()
        },
        label(Readout::Frags, LARGE, TEXT_COLOR),
    ));
    // По центру внизу - уведомления о подобранном
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Percent(18.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((label(Readout::Notices, LARGE, NOTICE_COLOR), TextLayout::justify(Justify::Center)));
        });
    // Слева внизу - здоровье и броня
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(MARGIN),
            bottom: Val::Px(MARGIN),
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|column| {
            column.spawn(label(Readout::Armor, LARGE, ARMOR_COLOR));
            column.spawn(label(Readout::Health, LARGE, HEALTH_COLOR));
        });
    // Справа внизу - запас патронов и выбранное оружие с его патронами
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            right: Val::Px(MARGIN),
            bottom: Val::Px(MARGIN),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            ..default()
        })
        .with_children(|column| {
            column.spawn(label(Readout::Ammo, SMALL, TEXT_COLOR));
            column.spawn(label(Readout::Weapon, LARGE, TEXT_COLOR));
        });
}

fn update_hud(
    diagnostics: Res<DiagnosticsStore>,
    frags: Res<Frags>,
    time: Res<Time>,
    mut notices: ResMut<Notices>,
    player: Query<(&Health, Option<&Weapon>, Option<&Ammo>), With<Player>>,
    mut labels: Query<(&Readout, &mut Text)>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed());
    let player = player.single().ok();
    if !notices.0.is_empty() {
        let dt = time.delta_secs();
        notices.0.retain_mut(|(_, left)| {
            *left -= dt;
            *left > 0.0
        });
    }
    for (readout, mut text) in labels.iter_mut() {
        let shown = match readout {
            Readout::Fps => fps.map(|fps| format!("FPS {fps:.0}")),
            Readout::Weapon => player.and_then(|(_, weapon, ammo)| weapon.zip(ammo)).map(|(weapon, ammo)| {
                let name = match weapon.kind {
                    WeaponKind::Gauntlet => "GAUNTLET",
                    WeaponKind::MachineGun => "MACHINE GUN",
                    WeaponKind::Railgun => "RAILGUN",
                    WeaponKind::RocketLauncher => "ROCKET LAUNCHER",
                };
                // У перчатки патронов нет
                match ammo.left(weapon.kind) {
                    Some(left) => format!("{name} {left}"),
                    None => name.to_string(),
                }
            }),
            Readout::Ammo => player.and_then(|(_, _, ammo)| ammo).map(|ammo| {
                format!("MG {}   RAIL {}   ROCKET {}", ammo.bullets, ammo.rails, ammo.rockets)
            }),
            Readout::Health => player.map(|(health, ..)| format!("HP {:.0}", health.current.ceil())),
            Readout::Notices => {
                Some(notices.0.iter().map(|(text, _)| text.as_str()).collect::<Vec<_>>().join("\n"))
            }
            Readout::Frags => Some(format!("FRAGS {}", frags.0)),
            Readout::Armor => player.map(|(health, ..)| format!("ARMOR {:.0}", health.armor.ceil())),
        };
        // Текст меняется только вместе со значением, иначе он перерисовывался бы каждый кадр
        if let Some(shown) = shown
            && text.0 != shown
        {
            text.0 = shown;
        }
    }
}
