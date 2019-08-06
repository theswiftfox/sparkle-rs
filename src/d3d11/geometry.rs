use cgmath::*;

pub struct Vertex {
    pub position : Vector4<f32>,
    pub color : Vector4<f32>
}

impl Default for Vertex {
    fn default() -> Vertex {
        Vertex {
            position : Vector4::zero(),
            color : Vector4::zero()
        }
    }
}

impl Vertex {
    pub fn new(position : &Vector4<f32>, color : &Vector4<f32>) -> Vertex {
        Vertex {
            position : *position,
            color : *color,
        }
    }
    pub fn new_from_f32(x : f32, y : f32, z : f32, w : f32, r : f32, g : f32, b : f32, a : f32) -> Vertex {
        Vertex::new(&Vector4::new(x, y, z, w), &Vector4::new(r, g, b, a))
    }
}