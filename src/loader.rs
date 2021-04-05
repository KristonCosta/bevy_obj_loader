use anyhow::{Context, Result};
use bevy::asset::{Asset, AssetLoader, AssetPath, BoxedFuture, LoadContext, LoadedAsset};

use bevy::pbr::PbrBundle;
use bevy::prelude::{
    BuildWorldChildren, Color, GlobalTransform, Handle, Mat4, Mesh, StandardMaterial, Texture,
    Transform, World,
};
use bevy::render::mesh::{Indices, VertexAttributeValues};
use bevy::render::pipeline::PrimitiveTopology;
use bevy::render::texture::{FilterMode, ImageType, SamplerDescriptor, TextureFormat};
use bevy::scene::Scene;
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::future::Future;
use std::io::{BufRead, BufReader, ErrorKind};
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tobj::{LoadError, MTLLoadResult};

#[derive(Error, Debug)]
pub enum ObjError {
    #[error("invalid obj format")]
    InvalidObjFormat,
}

#[derive(Default)]
pub struct ObjLoader;

impl AssetLoader for ObjLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<()>> {
        Box::pin(async move { Ok(load_obj(bytes, load_context).await?) })
    }

    fn extensions(&self) -> &[&str] {
        &["obj"]
    }
}

struct Builder;

impl Builder {
    pub fn build(&mut self, path: &Path) -> MTLLoadResult {
        Err(LoadError::ReadError)
    }
}

async fn load_obj<'a, 'b>(
    bytes: &'a [u8],
    load_context: &'a mut LoadContext<'b>,
) -> Result<(), ObjError> {
    // For now do two passes:
    // 1.  fetch all required materials
    // 2.  load required materials
    // 3.  reprocess the obj file

    let mut pending_materials = get_material_lib_paths(&mut BufReader::new(bytes))
        .map_err(|e| ObjError::InvalidObjFormat)?;

    let mut materials = HashMap::new();
    let parent = load_context.path().parent().unwrap();

    for material in &pending_materials {
        let bytes_vec = load_context
            .read_asset_bytes(parent.join(material))
            .await
            .unwrap();
        materials.insert(
            material.clone(),
            tobj::load_mtl_buf(&mut BufReader::new(bytes_vec.as_slice())),
        );
    }

    let (meshes, materials) = tobj::load_obj_buf(&mut BufReader::new(bytes), false, |p| {
        if let Some(res) = materials.get(&p.to_str().unwrap().to_string()) {
            res.clone()
        } else {
            Err(LoadError::ReadError)
        }
    })
    .unwrap();

    let mut loaded_materials = Vec::with_capacity(materials.len());
    for material in materials {
        loaded_materials.push(
            load_material(&material, load_context)
                .await
                .map_err(|e| ObjError::InvalidObjFormat)?,
        );
    }

    let mut loaded_meshes = Vec::with_capacity(meshes.len());

    let mut world = World::default();

    world
        .spawn()
        .insert_bundle((Transform::identity(), GlobalTransform::identity()))
        .with_children(|parent| {
            for (i, tobj_mesh) in meshes.into_iter().enumerate() {
                let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);
                mesh.set_attribute(
                    Mesh::ATTRIBUTE_POSITION,
                    VertexAttributeValues::Float3(
                        chunk_by::<f32, 3>(&tobj_mesh.mesh.positions).unwrap(),
                    ),
                );

                mesh.set_attribute(
                    Mesh::ATTRIBUTE_NORMAL,
                    VertexAttributeValues::Float3(
                        chunk_by::<f32, 3>(&tobj_mesh.mesh.normals).unwrap(),
                    ),
                );

                mesh.set_attribute(
                    Mesh::ATTRIBUTE_UV_0,
                    VertexAttributeValues::Float2(
                        chunk_by::<f32, 2>(&tobj_mesh.mesh.texcoords).unwrap(),
                    ),
                );

                mesh.set_indices(Some(Indices::U32(tobj_mesh.mesh.indices)));

                let mesh = load_context.set_labeled_asset(&tobj_mesh.name, LoadedAsset::new(mesh));
                let material = tobj_mesh
                    .mesh
                    .material_id
                    .and_then(|i| loaded_materials.get(i).cloned());
                let loaded_mesh = load_context.set_labeled_asset(
                    &format!("ObjMesh{}", i),
                    LoadedAsset::new(super::ObjMesh {
                        mesh: mesh.clone(),
                        material: material.clone(),
                    }),
                );

                let bundle = if let Some(material) = material {
                    PbrBundle {
                        mesh,
                        material,
                        ..Default::default()
                    }
                } else {
                    PbrBundle {
                        mesh,
                        ..Default::default()
                    }
                };

                parent.spawn_bundle(bundle);

                loaded_meshes.push(loaded_mesh);
            }
        });
    load_context.set_labeled_asset(
        "Obj",
        LoadedAsset::new(super::Obj {
            materials: loaded_materials,
            meshes: loaded_meshes,
        }),
    );

    load_context.set_labeled_asset("Scene", LoadedAsset::new(Scene::new(world)));

    Ok(())
}

async fn load_material<'a, 'b>(
    material: &tobj::Material,
    load_context: &'a mut LoadContext<'b>,
) -> Result<Handle<StandardMaterial>> {
    let material_label = material_label(material);

    let base_color_texture = try_texture_handle(&material.diffuse_texture, load_context).await?;

    let normal_map = try_texture_handle(&material.normal_texture, load_context).await?;

    let metallic_roughness_texture =
        try_texture_handle(&material.specular_texture, load_context).await?;

    let occlusion_texture = try_texture_handle(&material.ambient_texture, load_context).await?;

    Ok(load_context.set_labeled_asset(
        &material_label,
        LoadedAsset::new(StandardMaterial {
            base_color: Color::rgb(
                material.diffuse[0],
                material.diffuse[1],
                material.diffuse[2],
            ),
            base_color_texture,
            metallic_roughness_texture,
            reflectance: material.shininess,
            normal_map,
            occlusion_texture,
            ..Default::default()
        }),
    ))
}

fn chunk_by<'a, T: 'a + Clone, const N: usize>(v: &'a [T]) -> Result<Vec<[T; N]>>
where
    [T; N]: TryFrom<&'a [T]>,
{
    v.chunks(N)
        .map(|x| {
            x.clone()
                .try_into()
                .map_err(|e| anyhow::Error::msg("failed to chunk"))
        })
        .collect()
}

async fn try_texture_handle<'a, 'b>(
    texture: &String,
    load_context: &'a mut LoadContext<'b>,
) -> Result<Option<Handle<Texture>>> {
    if !texture.is_empty() {
        let label = texture_label(&texture);
        load_texture(texture, load_context).await?;
        let path = AssetPath::new_ref(load_context.path(), Some(&label));

        Ok(Some(load_context.get_handle(path)))
    } else {
        Ok(None)
    }
}

async fn load_texture<'a, 'b>(
    texture: &String,
    load_context: &'a mut LoadContext<'b>,
) -> Result<()> {
    let label = texture_label(texture);
    let parent = load_context.path().parent().unwrap();
    let image_path = parent.join(texture);

    let bytes = load_context.read_asset_bytes(image_path.clone()).await?;

    let mut texture = Texture::from_buffer(
        &bytes,
        ImageType::Extension(image_path.extension().unwrap().to_str().unwrap()),
    )?;
    texture.sampler = texture_sampler();
    texture.format = TextureFormat::Rgba8UnormSrgb;
    load_context.set_labeled_asset(&label, LoadedAsset::new(texture));
    Ok(())
}

fn texture_sampler() -> SamplerDescriptor {
    SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..Default::default()
    }
}

fn texture_label(texture: &String) -> String {
    texture.clone()
}

fn material_label(material: &tobj::Material) -> String {
    material.name.clone()
}

fn model_label(model: &tobj::Model) -> String {
    model.name.clone()
}

fn get_material_lib_paths<B: BufRead>(reader: &mut B) -> Result<Vec<String>> {
    let mut materials = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("mtllib") => {
                let mtllib = parts.next().context("invalid mtllib definition")?;
                materials.push(mtllib.to_string());
            }
            _ => {}
        }
    }
    Ok(materials)
}
