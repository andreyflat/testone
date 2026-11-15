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
use bevy::render::error_handler::{RenderError, RenderErrorHandler, RenderErrorPolicy};
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

/// Что делать при ошибке рендера. По умолчанию Bevy закрывает игру при любой.
///
/// Одну ошибку игра переживает: при смене размера окна (например, на весь экран) wgpu иногда
/// не успевает дождаться видеокарты и не перенастраивает поверхность окна. Это не поломка
/// рендера: поверхность перенастроится со следующим кадром. Всё остальное по-прежнему закрывает игру.
fn render_error_policy(error: &RenderError, main_world: &mut World, _: &mut World) -> RenderErrorPolicy {
    if error.description.contains("Surface::configure") {
        warn!("Поверхность окна не перенастроилась, продолжаем: {}", error.description);
        return RenderErrorPolicy::Ignore;
    }
    error!("Закрываем игру из-за ошибки рендера {:?}", error.ty);
    main_world.write_message(AppExit::error());
    RenderErrorPolicy::StopRendering
}

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
        .insert_resource(RenderErrorHandler(render_error_policy))
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
