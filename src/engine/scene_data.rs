//! Serializable scene state for save/load.
//!
//! `SceneData` captures the editable state of a scene: node transforms and
//! lights. It does NOT store geometry or materials — those come from the
//! base glTF file. Think of this as an "overlay" of edits on top of the
//! imported scene.

use serde::{Deserialize, Serialize};

/// A serializable representation of a light.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LightData {
    pub position: [f32; 3],
    pub light_type: LightTypeData,
    pub color: [f32; 3],
    pub radius: f32,
    pub penumbra_radius: f32,
}

/// Serializable light type enum.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LightTypeData {
    Ambient,
    Directional,
    Area,
}

/// A serializable node transform override.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeTransform {
    /// Node name (used to look up the node in the scenegraph).
    pub name: String,
    /// The 4x4 local transform matrix, stored as column-major [f32; 16].
    pub transform: [f32; 16],
}

/// The complete serializable scene state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneData {
    /// Path to the base glTF scene file.
    pub scene_file: String,
    /// Per-node transform overrides (only nodes whose transforms were edited).
    pub node_transforms: Vec<NodeTransform>,
    /// All lights in the scene (replaces the default lights from import).
    pub lights: Vec<LightData>,
}

// Conversion helpers between engine types and serializable types

use crate::engine::geometry::{Light, LightType};

impl From<&Light> for LightData {
    fn from(light: &Light) -> Self {
        LightData {
            position: [light.position.x, light.position.y, light.position.z],
            light_type: match light.t {
                LightType::Ambient => LightTypeData::Ambient,
                LightType::Directional => LightTypeData::Directional,
                LightType::Area => LightTypeData::Area,
            },
            color: [light.color.x, light.color.y, light.color.z],
            radius: light.radius,
            penumbra_radius: light.penumbra_radius,
        }
    }
}

impl LightData {
    /// Convert back to an engine Light.
    ///
    /// Note: `light_proj` is set to identity — the renderer recomputes it
    /// each frame for directional lights.
    pub fn to_light(&self) -> Light {
        Light {
            position: glm::vec3(self.position[0], self.position[1], self.position[2]),
            t: match self.light_type {
                LightTypeData::Ambient => LightType::Ambient,
                LightTypeData::Directional => LightType::Directional,
                LightTypeData::Area => LightType::Area,
            },
            color: glm::vec3(self.color[0], self.color[1], self.color[2]),
            radius: self.radius,
            penumbra_radius: self.penumbra_radius,
            light_proj: glm::identity(),
        }
    }
}

/// Helper: convert a glm::Mat4 to a [f32; 16] array (column-major).
pub fn mat4_to_array(m: &glm::Mat4) -> [f32; 16] {
    let s = m.as_slice();
    let mut arr = [0.0f32; 16];
    arr.copy_from_slice(s);
    arr
}

/// Helper: convert a [f32; 16] array (column-major) back to a glm::Mat4.
pub fn array_to_mat4(arr: &[f32; 16]) -> glm::Mat4 {
    glm::make_mat4(arr)
}

impl SceneData {
    /// Save this scene data to a RON file.
    pub fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let pretty = ron::ser::PrettyConfig::default();
        let s = ron::ser::to_string_pretty(self, pretty)?;
        std::fs::write(path, s)?;
        Ok(())
    }

    /// Load scene data from a RON file.
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let s = std::fs::read_to_string(path)?;
        let data: SceneData = ron::from_str(&s)?;
        Ok(data)
    }
}
