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
mod crosshair;

use bevy::anti_alias::dlss::DlssProjectId;
use bevy::asset::uuid::uuid;
use bevy::solari::prelude::SolariPlugins;
use bevy_rapier3d::prelude::*;
//use bevy_inspector_egui::quick::WorldInspectorPlugin;
use player::PlayerPlugin;
use camera::CameraPlugin;
use world::WorldPlugin;
use lights::LightsPlugin;
use weapons::{WeaponsPlugin, equip_player_weapon};
use enemies::EnemiesPlugin;
use crosshair::CrosshairPlugin;

fn main() {
    App::new()
        // Должен быть добавлен до DefaultPlugins
        .insert_resource(DlssProjectId(uuid!("0a071ebc-923e-41cd-b00c-1272045d8302")))
        .add_plugins(PlayerPlugin)
        .add_plugins(CameraPlugin)
        .add_plugins(WorldPlugin)
        .add_plugins(LightsPlugin)
        .add_plugins(WeaponsPlugin)
        .add_plugins(EnemiesPlugin)
        .add_plugins(CrosshairPlugin)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                mode: WindowMode::Windowed,
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(SolariPlugins)
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        //.add_plugins(WorldInspectorPlugin::new())
        .add_plugins((
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin::default(),
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
    mut app_exit_events: MessageWriter<AppExit>,
) {
    if keyboard_input.just_pressed(KeyCode::Escape) {
        app_exit_events.write(AppExit::default());
    }
}
