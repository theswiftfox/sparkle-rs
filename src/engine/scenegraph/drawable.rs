pub trait Drawable {
    fn draw(&mut self, model: glm::Mat4);
}
