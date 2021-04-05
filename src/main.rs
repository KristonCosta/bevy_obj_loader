use bevy_obj_loader::*;

use bevy::{pbr::AmbientLight, prelude::*};

fn main() {
    App::build()
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 1.0 / 5.0f32,
        })
        .insert_resource(Msaa { samples: 4 })
        .add_plugins(DefaultPlugins)
        .add_plugin(ObjPlugin)
        .add_startup_system(setup.system())
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn_scene(asset_server.load("monu5.obj"));
    commands.spawn_bundle(PerspectiveCameraBundle {
        transform: Transform::from_xyz(0.7, 0.7, 1.0).looking_at(Vec3::new(0.0, 0.3, 0.0), Vec3::Y),
        ..Default::default()
    });
    commands.spawn_bundle(LightBundle {
        transform: Transform::from_xyz(3.0, 5.0, 3.0),
        ..Default::default()
    });
}
