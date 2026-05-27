//! Generic skybox: a unit cube with a cubemap texture.
//!
//! Loads 6 face images from `assets/sky_box_*.png` and creates a
//! [`Drawable<B>`] with the cubemap bound at texture slot 0.
//! The skybox pass vertex shader uses vertex positions as 3D texture
//! coordinates for cubemap sampling.

use super::backend::*;

use std::cell::RefCell;
use std::rc::Rc;

pub(crate) struct Skybox<B: GpuBackend> {
    drawable: Rc<RefCell<Drawable<B>>>,
}

impl<B: GpuBackend> Skybox<B> {
    /// Load skybox face images and create the cubemap geometry.
    ///
    /// Face images are loaded from:
    /// - `assets/sky_box_x.png`     (+X)
    /// - `assets/sky_box_x_neg.png` (-X)
    /// - `assets/sky_box_y.png`     (+Y)
    /// - `assets/sky_box_y_neg.png` (-Y)
    /// - `assets/sky_box_z.png`     (+Z)
    /// - `assets/sky_box_z_neg.png` (-Z)
    pub fn load(backend: &B) -> Result<Skybox<B>, GpuError> {
        use super::geometry::Vertex;

        // Build unit cube geometry (8 vertices, 36 indices)
        let vertices = [
            // 0: (+1, +1, +1)
            Vertex { position: glm::vec3(1.0, 1.0, 1.0), ..Vertex::default() },
            // 1: (+1, -1, +1)
            Vertex { position: glm::vec3(1.0, -1.0, 1.0), ..Vertex::default() },
            // 2: (-1, +1, +1)
            Vertex { position: glm::vec3(-1.0, 1.0, 1.0), ..Vertex::default() },
            // 3: (-1, -1, +1)
            Vertex { position: glm::vec3(-1.0, -1.0, 1.0), ..Vertex::default() },
            // 4: (+1, +1, -1)
            Vertex { position: glm::vec3(1.0, 1.0, -1.0), ..Vertex::default() },
            // 5: (+1, -1, -1)
            Vertex { position: glm::vec3(1.0, -1.0, -1.0), ..Vertex::default() },
            // 6: (-1, +1, -1)
            Vertex { position: glm::vec3(-1.0, 1.0, -1.0), ..Vertex::default() },
            // 7: (-1, -1, -1)
            Vertex { position: glm::vec3(-1.0, -1.0, -1.0), ..Vertex::default() },
        ];

        #[rustfmt::skip]
        let indices: [u32; 36] = [
            // Back face (-Z)
            6, 7, 5, 5, 4, 6,
            // Left face (-X)
            3, 7, 6, 6, 2, 3,
            // Right face (+X)
            5, 1, 0, 0, 4, 5,
            // Front face (+Z)
            3, 2, 0, 0, 1, 3,
            // Top face (+Y)
            6, 4, 0, 0, 2, 6,
            // Bottom face (-Y)
            7, 3, 5, 5, 3, 1,
        ];

        let drawable = Drawable::from_verts(backend, &vertices, &indices, ObjType::Any)?;

        // Load cubemap face images
        let face_paths = [
            "assets/sky_box_x.png",
            "assets/sky_box_x_neg.png",
            "assets/sky_box_y.png",
            "assets/sky_box_y_neg.png",
            "assets/sky_box_z.png",
            "assets/sky_box_z_neg.png",
        ];

        let images: Vec<image::DynamicImage> = face_paths
            .iter()
            .map(|p| {
                image::open(p).map_err(|e| {
                    GpuError::new(
                        format!("Failed to load skybox face '{}': {}", p, e),
                        GpuErrorKind::ResourceCreation,
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Convert all faces to RGBA and validate dimensions
        let rgba_images: Vec<image::RgbaImage> =
            images.iter().map(|img| img.to_rgba8()).collect();
        let face_width = rgba_images[0].width();
        let face_height = rgba_images[0].height();

        for (i, img) in rgba_images.iter().enumerate() {
            if img.width() != face_width || img.height() != face_height {
                return Err(GpuError::new(
                    format!(
                        "Skybox face {} has dimensions {}x{}, expected {}x{}",
                        face_paths[i],
                        img.width(),
                        img.height(),
                        face_width,
                        face_height,
                    ),
                    GpuErrorKind::ResourceCreation,
                ));
            }
        }

        let face_data: Vec<&[u8]> = rgba_images.iter().map(|img| img.as_ref() as &[u8]).collect();
        let faces: [&[u8]; 6] = [
            face_data[0],
            face_data[1],
            face_data[2],
            face_data[3],
            face_data[4],
            face_data[5],
        ];

        let cubemap = backend.create_cubemap(
            faces,
            face_width,
            face_height,
            TextureFormat::Rgba8UnormSrgb,
            &SamplerDesc {
                address_u: AddressMode::Clamp,
                address_v: AddressMode::Clamp,
                filter: FilterMode::Linear,
                compare: None,
            },
        )?;

        drawable
            .borrow_mut()
            .add_texture(0, Rc::new(cubemap));

        // Apply initial rotation to match skybox face orientation
        let rot = glm::rotate(&glm::identity(), 4.78, &glm::vec3(0.0, 1.0, 0.0));
        let rot = glm::rotate(&rot, 1.571, &glm::vec3(0.0, 0.0, -1.0));
        drawable.borrow_mut().update_model(backend, &rot);

        Ok(Skybox { drawable })
    }

    /// Update the skybox model matrix.
    pub fn update_model(&self, backend: &B, model: &glm::Mat4) {
        self.drawable.borrow_mut().update_model(backend, model);
    }

    /// Draw the skybox cube. Binds the cubemap texture via the drawable's material.
    pub fn draw(&self, backend: &mut B) {
        self.drawable.borrow().draw(backend, true);
    }
}
