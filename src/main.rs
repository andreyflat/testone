// src/main.rs - Обновленный main с новыми системами
use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin}, 
    prelude::*,
    window::{PresentMode, WindowMode}
};

mod lights;
mod player;
mod camera;
mod world;
mod weapons;
mod enemies;

use bevy_rapier3d::prelude::*;
//use bevy_inspector_egui::quick::WorldInspectorPlugin;
use player::PlayerPlugin;
use camera::CameraPlugin;
use world::WorldPlugin;
use lights::LightsPlugin;
use weapons::{WeaponsPlugin, equip_player_weapon};
use enemies::EnemiesPlugin;

fn main() {
    App::new()
        .add_plugins(PlayerPlugin)
        .add_plugins(CameraPlugin)
        .add_plugins(WorldPlugin)
        .add_plugins(LightsPlugin)
        .add_plugins(WeaponsPlugin)
        .add_plugins(EnemiesPlugin)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                mode: WindowMode::Windowed,
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            RapierDebugRenderPlugin::default(),
            RapierPhysicsPlugin::<NoUserData>::default(),
        ))
        //.add_plugins(WorldInspectorPlugin::new())
        .add_plugins((
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, (
            equip_player_weapon, // Добавляем оружие игроку после спавна
            check_exit,
        ))
        .run();
}

fn setup() {
    // Здесь будет код инициализации
}

fn check_exit(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut app_exit_events: EventWriter<AppExit>,
) {
    if keyboard_input.just_pressed(KeyCode::Escape) {
        app_exit_events.send(AppExit::default());
    }
}
