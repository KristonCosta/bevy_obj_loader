use bevy::app::{AppBuilder, Plugin};
use bevy::prelude::*;
use bevy::reflect::*;

mod loader;
use loader::ObjLoader;

#[derive(Default)]
pub struct ObjPlugin;

impl Plugin for ObjPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.init_asset_loader::<ObjLoader>()
            .add_asset::<Obj>()
            .add_asset::<ObjMesh>();
    }
}

#[derive(Debug, TypeUuid)]
#[uuid = "a4de4700-f615-4910-bc86-84f9a24ce5ae"]
pub struct Obj {
    pub materials: Vec<Handle<StandardMaterial>>,
    pub meshes: Vec<Handle<ObjMesh>>,
}

#[derive(Debug, TypeUuid)]
#[uuid = "a01f5ccf-0db1-4577-a951-c8749caa5b4e"]
pub struct ObjMesh {
    pub mesh: Handle<Mesh>,
    pub material: Option<Handle<StandardMaterial>>,
}
