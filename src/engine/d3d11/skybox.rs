use std::cell::RefCell;
use std::f32::consts::PI;
use std::rc::Rc;

use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

use crate::engine::{
    d3d11::{
        drawable::{DxDrawable, ObjType},
        textures::Texture2D,
        DxError,
    },
    geometry::Vertex,
};

pub(crate) struct SkyBox {
    scale: glm::Mat4,
    drawable: Rc<RefCell<DxDrawable>>,
}

impl SkyBox {
    pub fn update(&mut self, model: &glm::Mat4) {
        let mvp = self.scale * model;
        self.drawable.borrow_mut().update_model(&mvp);
    }
    pub fn draw(&self) {
        self.drawable.borrow().draw(true);
    }

    pub fn new(
        lat_lines: u32,
        long_lines: u32,
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<SkyBox, DxError> {
        let num_verts = ((lat_lines - 2) * long_lines) + 2;
        // let num_faces = ((lat_lines - 3) * long_lines * 2) + (long_lines * 2);

        let mut yaw = 0.0;
        let mut pitch = 0.0;

        let mut vertices = Vec::<Vertex>::new();
        vertices.push(Vertex {
            position: glm::vec3(0.0, 0.0, 1.0),
            normal: glm::zero(),
            tex_coord: glm::zero(),
        });

        let rot_base = glm::Mat4::identity();
        for i in 0..(lat_lines - 2) {
            pitch = ((i + 1) as f32) * (PI / ((lat_lines - 1) as f32));
            let rot_x = glm::rotate(&rot_base, pitch, &glm::vec3(1.0, 0.0, 0.0));
            for j in 0..long_lines {
                yaw = (j as f32) * (2.0 * PI / (long_lines as f32));
                let rot_y = glm::rotate(&rot_base, yaw, &glm::vec3(0.0, 0.0, 1.0));
                let rot = glm::mat4_to_mat3(&rot_x) * (glm::mat4_to_mat3(&rot_y));
                let vtx_pos = (rot * glm::vec3(0.0, 0.0, 1.0)).normalize();
                vertices.push(Vertex {
                    position: vtx_pos,
                    normal: glm::zero(),
                    tex_coord: glm::zero(),
                });
            }
        }
        vertices.push(Vertex {
            position: glm::vec3(0.0, 0.0, -1.0),
            normal: glm::zero(),
            tex_coord: glm::zero(),
        });

        let mut indices = Vec::<u32>::new();
        for i in 0..(long_lines - 1) {
            indices.push(0);
            indices.push(i + 1);
            indices.push(i + 2);
        }
        indices.push(0);
        indices.push(long_lines);
        indices.push(1);

        for i in 0..(lat_lines - 3) {
            for j in 0..(long_lines - 1) {
                // quad
                indices.push(i * long_lines + j + 1);
                indices.push(i * long_lines + j + 2);
                indices.push((i + 1) * long_lines + j + 1);

                indices.push((i + 1) * long_lines + j + 1);
                indices.push(i * long_lines + j + 2);
                indices.push((i + 1) * long_lines + j + 2);
            }

            indices.push(i * long_lines + long_lines);
            indices.push(i * long_lines + 1);
            indices.push((i + 1) * long_lines + long_lines);
            indices.push((i + 1) * long_lines + long_lines);
            indices.push(i * long_lines + 1);
            indices.push((i + 1) * long_lines + 1);
        }

        for i in 0..(long_lines - 1) {
            indices.push(num_verts - 1);
            indices.push(num_verts - i - 2);
            indices.push(num_verts - i - 3);
        }

        indices.push(num_verts - 1);
        indices.push(num_verts - 1 - long_lines);
        indices.push(num_verts - 2);

        let drawable = DxDrawable::from_verts(device, context, vertices, indices, ObjType::Any)?;

        // load texture
        let x = image::open("assets/sky_box_x.png").unwrap();
        let x_neg = image::open("assets/sky_box_x_neg.png").unwrap();
        let y = image::open("assets/sky_box_y.png").unwrap();
        let y_neg = image::open("assets/sky_box_y_neg.png").unwrap();
        let z = image::open("assets/sky_box_z.png").unwrap();
        let z_neg = image::open("assets/sky_box_z_neg.png").unwrap();
        let images = [x, x_neg, y, y_neg, z, z_neg];
        let tex = std::rc::Rc::from(std::cell::RefCell::from(
            Texture2D::create_cubemap_from_image_obj(
                &images,
                dx11::D3D11_TEXTURE_ADDRESS_CLAMP,
                dx11::D3D11_TEXTURE_ADDRESS_CLAMP,
                dx11::D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                device,
                context,
            )?,
        ));
        drawable.borrow_mut().add_texture(0, tex);
        let scale = glm::scale(&glm::identity(), &glm::vec3(5.0, 5.0, 5.0));
        Ok(SkyBox {
            drawable: drawable,
            scale: scale,
        })
    }
}
