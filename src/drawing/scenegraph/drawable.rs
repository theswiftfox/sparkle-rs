use cgmath::Matrix4;

pub trait Drawable {
    fn draw(&mut self, model: Matrix4<f32>);
}

