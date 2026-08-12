use std::fs;

pub fn load_gltf_mesh(
    gltf_path: &str,
    bin_path: &str,
) -> Result<(Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>), Box<dyn std::error::Error>> {
    let json_text = fs::read_to_string(gltf_path)?;
    let gltf: serde_json::Value = serde_json::from_str(&json_text)?;
    let bin_data = fs::read(bin_path)?;
    let accessors = gltf["accessors"].as_array().ok_or("missing accessors")?;
    let buffer_views = gltf["bufferViews"].as_array().ok_or("missing bufferViews")?;

    let primitive = &gltf["meshes"][0]["primitives"][0];
    let pos_accessor_idx = primitive["attributes"]["POSITION"]
        .as_u64()
        .ok_or("missing POSITION attribute")? as usize;
    let idx_accessor_idx = primitive["indices"]
        .as_u64()
        .ok_or("missing indices")? as usize;

    let positions = read_vec3_accessor(&accessors[pos_accessor_idx], buffer_views, &bin_data)?;

    let normals = if let Some(norm_idx) = primitive["attributes"]["NORMAL"].as_u64() {
        read_vec3_accessor(&accessors[norm_idx as usize], buffer_views, &bin_data)?
    } else {
        vec![[0.0, 0.0, 1.0]; positions.len()]
    };

    let idx_accessor = &accessors[idx_accessor_idx];
    let idx_count = idx_accessor["count"].as_u64().ok_or("missing index count")? as usize;
    let idx_bv_idx = idx_accessor["bufferView"].as_u64().ok_or("missing index bufferView")? as usize;
    let idx_bv = &buffer_views[idx_bv_idx];
    let idx_offset = idx_bv["byteOffset"].as_u64().unwrap_or(0) as usize
        + idx_accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
    let component_type = idx_accessor["componentType"].as_u64().ok_or("missing componentType")?;

    let mut indices = Vec::with_capacity(idx_count);
    for i in 0..idx_count {
        let val = match component_type {
            5123 => {
                let base = idx_offset + i * 2;
                u16::from_le_bytes(bin_data[base..base + 2].try_into()?) as u32
            }
            5125 => {
                let base = idx_offset + i * 4;
                u32::from_le_bytes(bin_data[base..base + 4].try_into()?)
            }
            _ => return Err(format!("Unsupported index component type: {}", component_type).into()),
        };
        indices.push(val);
    }

    Ok((positions, normals, indices))
}

fn read_vec3_accessor(
    accessor: &serde_json::Value,
    buffer_views: &[serde_json::Value],
    bin_data: &[u8],
) -> Result<Vec<[f32; 3]>, Box<dyn std::error::Error>> {
    let count = accessor["count"].as_u64().ok_or("missing accessor count")? as usize;
    let bv_idx = accessor["bufferView"].as_u64().ok_or("missing bufferView")? as usize;
    let bv = &buffer_views[bv_idx];
    let offset = bv["byteOffset"].as_u64().unwrap_or(0) as usize
        + accessor["byteOffset"].as_u64().unwrap_or(0) as usize;

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let base = offset + i * 12;
        let x = f32::from_le_bytes(bin_data[base..base + 4].try_into()?);
        let y = f32::from_le_bytes(bin_data[base + 4..base + 8].try_into()?);
        let z = f32::from_le_bytes(bin_data[base + 8..base + 12].try_into()?);
        out.push([x, y, z]);
    }
    Ok(out)
}