use std::cell::RefCell;
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
    drawable: Rc<RefCell<DxDrawable>>,
}

impl SkyBox {
    pub fn update(&mut self, model: &glm::Mat4) {
        self.drawable.borrow_mut().update_model(&model);
    }
    pub fn draw(&self) {
        self.drawable.borrow().draw(true);
    }

    pub fn new(
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<SkyBox, DxError> {
        let mut vertices = Vec::<Vertex>::new();
        let mut indices = Vec::<u32>::new();

        // 0
        vertices.push(Vertex {
            position: glm::vec3(1.0, 1.0, 1.0),
            normal: glm::zero(),
            tangent: glm::zero(),
            bitangent: glm::zero(),
            tex_coord: glm::zero(),
        });
        // 1
        vertices.push(Vertex {
            position: glm::vec3(1.0, -1.0, 1.0),
            normal: glm::zero(),
            tangent: glm::zero(),
            bitangent: glm::zero(),
            tex_coord: glm::zero(),
        });
        // 2
        vertices.push(Vertex {
            position: glm::vec3(-1.0, 1.0, 1.0),
            normal: glm::zero(),
            tangent: glm::zero(),
            bitangent: glm::zero(),
            tex_coord: glm::zero(),
        });
        // 3
        vertices.push(Vertex {
            position: glm::vec3(-1.0, -1.0, 1.0),
            normal: glm::zero(),
            tangent: glm::zero(),
            bitangent: glm::zero(),
            tex_coord: glm::zero(),
        });
        // 4
        vertices.push(Vertex {
            position: glm::vec3(1.0, 1.0, -1.0),
            normal: glm::zero(),
            tangent: glm::zero(),
            bitangent: glm::zero(),
            tex_coord: glm::zero(),
        });
        // 5
        vertices.push(Vertex {
            position: glm::vec3(1.0, -1.0, -1.0),
            normal: glm::zero(),
            tangent: glm::zero(),
            bitangent: glm::zero(),
            tex_coord: glm::zero(),
        });
        // 6
        vertices.push(Vertex {
            position: glm::vec3(-1.0, 1.0, -1.0),
            normal: glm::zero(),
            tangent: glm::zero(),
            bitangent: glm::zero(),
            tex_coord: glm::zero(),
        });
        // 7
        vertices.push(Vertex {
            position: glm::vec3(-1.0, -1.0, -1.0),
            normal: glm::zero(),
            tangent: glm::zero(),
            bitangent: glm::zero(),
            tex_coord: glm::zero(),
        });

        indices = vec![
            // -1.0f,  1.0f, -1.0f,
            // -1.0f, -1.0f, -1.0f,
            //  1.0f, -1.0f, -1.0f,
            //  1.0f, -1.0f, -1.0f,
            //  1.0f,  1.0f, -1.0f,
            // -1.0f,  1.0f, -1.0f,
            6, 7, 5, 5, 4, 6,
            // -1.0f, -1.0f,  1.0f,
            // -1.0f, -1.0f, -1.0f,
            // -1.0f,  1.0f, -1.0f,
            // -1.0f,  1.0f, -1.0f,
            // -1.0f,  1.0f,  1.0f,
            // -1.0f, -1.0f,  1.0f,
            3, 7, 6, 6, 2, 3,
            // 1.0f, -1.0f, -1.0f,
            // 1.0f, -1.0f,  1.0f,
            // 1.0f,  1.0f,  1.0f,
            // 1.0f,  1.0f,  1.0f,
            // 1.0f,  1.0f, -1.0f,
            // 1.0f, -1.0f, -1.0f,
            5, 1, 0, 0, 4, 5,
            // -1.0f, -1.0f,  1.0f,
            // -1.0f,  1.0f,  1.0f,
            //  1.0f,  1.0f,  1.0f,
            //  1.0f,  1.0f,  1.0f,
            //  1.0f, -1.0f,  1.0f,
            // -1.0f, -1.0f,  1.0f,
            3, 2, 0, 0, 1, 3,
            // -1.0f,  1.0f, -1.0f,
            //  1.0f,  1.0f, -1.0f,
            //  1.0f,  1.0f,  1.0f,
            //  1.0f,  1.0f,  1.0f,
            // -1.0f,  1.0f,  1.0f,
            // -1.0f,  1.0f, -1.0f,
            6, 4, 0, 0, 2, 6,
            // -1.0f, -1.0f, -1.0f,
            // -1.0f, -1.0f,  1.0f,
            //  1.0f, -1.0f, -1.0f,
            //  1.0f, -1.0f, -1.0f,
            // -1.0f, -1.0f,  1.0f,
            //  1.0f, -1.0f,  1.0f
            7, 3, 5, 5, 3, 1,
        ];

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
        let rot = glm::rotate(&glm::identity(), 4.78, &glm::vec3(0.0, 1.0, 0.0));
        let rot = glm::rotate(&rot, 1.571, &glm::vec3(0.0, 0.0, -1.0));
        drawable.borrow_mut().update_model(&rot);
        Ok(SkyBox { drawable: drawable })
    }
}
