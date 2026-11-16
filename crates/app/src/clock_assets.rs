use assets::clock_textures::{CLOCK_TEXTURES, OPENING_TEXTURES};
use assets::resources::{AssetKind, AssetSet};
use render::NativeRenderer;
use scene::frame::TextureSet;
use scene::matrix::Vec4;
use scene::rods::RodMesh;

// The decoded assets handed to the scene and the renderer: the same data the PNG and JSON files give.
pub fn rod_mesh_of(mesh: &assets::resources::Asset) -> Result<RodMesh, String> {
    if mesh.kind != AssetKind::Mesh || (mesh.width != 16 && mesh.width != 6) || mesh.data.len() != mesh.width as usize * 9 * 16 {
        return Err("mesh: not 16 or 6 faces".to_owned());
    }
    let mut at = 0;
    let mut take = |count: usize| -> Vec<Vec4> {
        (0..count)
            .map(|_| {
                let mut v = [0f32; 4];
                for (n, x) in v.iter_mut().enumerate() {
                    *x = f32::from_le_bytes(mesh.data[at + 4 * n..at + 4 * n + 4].try_into().unwrap());
                }
                at += 16;
                v
            })
            .collect()
    };
    let faces = mesh.width as usize;
    let positions = take(faces * 4);
    let normals = take(faces);
    let coordinates = take(faces * 4);
    Ok(RodMesh { positions, normals, coordinates })
}

// The ten clock textures, texture n at its place in assets::kClockTextures; throws when one is missing.
pub fn upload_clock_textures(renderer: &mut NativeRenderer<'_>, set: &AssetSet) -> Result<(), String> {
    for (n, info) in CLOCK_TEXTURES.iter().enumerate() {
        let texture = set.find(info.name).filter(|t| t.kind == AssetKind::TextureRgba32).ok_or_else(|| format!("no clock texture {}", info.name))?;
        if texture.width != info.width || texture.height != info.height || texture.data.len() != info.width as usize * info.height as usize * 4 {
            return Err(format!("clock texture {}: not {} x {}", info.name, info.width, info.height));
        }
        renderer.set_texture(n as i32, texture.width, texture.height, &texture.data).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// The opening's nine textures at their opening indices (assets::kOpeningTextures); throws when one is missing.
pub fn upload_opening_textures(renderer: &mut NativeRenderer<'_>, set: &AssetSet) -> Result<(), String> {
    for info in &OPENING_TEXTURES {
        let texture = set.find(info.name).filter(|t| t.kind == AssetKind::TextureRgba32).ok_or_else(|| format!("no opening texture {}", info.name))?;
        if texture.width != info.width || texture.height != info.height || texture.data.len() != info.width as usize * info.height as usize * 4 {
            return Err(format!("opening texture {}: not {} x {}", info.name, info.width, info.height));
        }
        renderer.set_texture_in(TextureSet::Opening, info.index, texture.width, texture.height, &texture.data).map_err(|e| e.to_string())?;
    }
    Ok(())
}
