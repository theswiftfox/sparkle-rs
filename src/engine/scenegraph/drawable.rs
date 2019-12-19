#[derive(Clone, Copy, Debug)]
pub enum ObjType {
    Opaque,
    Transparent,
    Any, // use only for draw call to match all objects. never assign to object!
}

impl PartialEq for ObjType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (_, ObjType::Any) => true,
            (ObjType::Opaque, ObjType::Opaque) => true,
            (ObjType::Transparent, ObjType::Transparent) => true,
            _ => false,
        }
    }
}

pub trait Drawable {
    fn draw(&self, object_type: ObjType);
    fn update_model(&mut self, model: &glm::Mat4);
}
