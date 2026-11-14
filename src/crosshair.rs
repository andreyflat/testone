// src/crosshair.rs - Экранный прицел вместо системного курсора
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

const COLOR: Color = Color::srgb(0.95, 1.0, 0.95);
/// Тень под линиями, чтобы прицел читался на светлом пятне фонаря.
const SHADOW: Color = Color::srgba(0.0, 0.0, 0.0, 0.6);
const LINE_LENGTH: f32 = 8.0;
const LINE_WIDTH: f32 = 2.0;
/// Зазор от центра до начала линии.
const GAP: f32 = 5.0;
const DOT_SIZE: f32 = 2.0;

pub struct CrosshairPlugin;
impl Plugin for CrosshairPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_crosshair, hide_system_cursor))
            .add_systems(Update, follow_cursor);
    }
}

#[derive(Component)]
struct Crosshair;

fn hide_system_cursor(mut cursors: Query<&mut CursorOptions>) {
    for mut cursor in cursors.iter_mut() {
        cursor.visible = false;
        // Курсор не уходит за окно, иначе прицел пропадает посреди боя
        cursor.grab_mode = CursorGrabMode::Confined;
    }
}

fn spawn_crosshair(mut commands: Commands) {
    // Корень нулевого размера ставится в точку курсора, линии расходятся от неё
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            Visibility::Hidden,
            Crosshair,
            Name::new("Crosshair"),
        ))
        .with_children(|root| {
            let far = GAP + LINE_LENGTH;
            // (left, top, width, height) относительно центра
            let parts = [
                (-LINE_WIDTH * 0.5, -far, LINE_WIDTH, LINE_LENGTH),
                (-LINE_WIDTH * 0.5, GAP, LINE_WIDTH, LINE_LENGTH),
                (-far, -LINE_WIDTH * 0.5, LINE_LENGTH, LINE_WIDTH),
                (GAP, -LINE_WIDTH * 0.5, LINE_LENGTH, LINE_WIDTH),
                (-DOT_SIZE * 0.5, -DOT_SIZE * 0.5, DOT_SIZE, DOT_SIZE),
            ];
            for (color, outline) in [(SHADOW, 1.0), (COLOR, 0.0)] {
                for (left, top, width, height) in parts {
                    root.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(left - outline),
                            top: Val::Px(top - outline),
                            width: Val::Px(width + outline * 2.0),
                            height: Val::Px(height + outline * 2.0),
                            ..default()
                        },
                        BackgroundColor(color),
                    ));
                }
            }
        });
}

fn follow_cursor(
    windows: Query<&Window>,
    mut crosshair: Query<(&mut Node, &mut Visibility), With<Crosshair>>,
) {
    let (Ok(window), Ok((mut node, mut visibility))) = (windows.single(), crosshair.single_mut())
    else {
        return;
    };
    match window.cursor_position() {
        Some(cursor) => {
            node.left = Val::Px(cursor.x);
            node.top = Val::Px(cursor.y);
            *visibility = Visibility::Inherited;
        }
        None => *visibility = Visibility::Hidden,
    }
}
