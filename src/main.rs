use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin}, 
    prelude::*, 
    window::{PresentMode, WindowMode}
};

mod lights;
mod player;
mod camera;
mod world;

use bevy_rapier3d::prelude::*;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use player::{PlayerPlugin, update_position};
use camera::{CameraPlugin, follow_camera};
use world::{WorldPlugin, draw_cursor};
use lights::LightsPlugin;

fn main() {
    App::new()
        .add_plugins(PlayerPlugin)
        .add_plugins(CameraPlugin)
        .add_plugins(WorldPlugin)
        .add_plugins(LightsPlugin)
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
        ))
        .add_plugins((
            RapierPhysicsPlugin::<NoUserData>::default(),
        ))
        .add_plugins(WorldInspectorPlugin::new())
        .add_plugins((
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, (
            draw_cursor.after(update_position),
            follow_camera.after(update_position),
            check_exit,
        ).chain())
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
