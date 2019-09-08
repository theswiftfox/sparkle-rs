use cgmath::Matrix4;

pub trait Drawable {
    fn draw(&self, model: Matrix4<f32>);
}

