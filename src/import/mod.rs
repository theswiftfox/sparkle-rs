// gltf importer for sparkle-rs
use std::error::Error;
use std::cell::RefCell;
use std::rc::Rc;

use winapi::um::d3d11_1 as dx11_1;

use crate::drawing::geometry::Vertex;
use crate::drawing::scenegraph::node::Node;
use crate::drawing::scenegraph::drawable::Drawable;
use crate::drawing::d3d11::drawable::DxDrawable;
use crate::drawing::d3d11::sampler::Texture2D;

#[derive(Debug, Clone)]
pub struct ImportError {
	cause: String,
	description: String,
}

struct GltfImporter {
	buffers: Vec<gltf::buffer::Data>,
	images: Vec<gltf::image::Data>,
	device: *mut dx11_1::ID3D11Device1,
	context: *mut dx11_1::ID3D11DeviceContext1
}

impl ImportError {
	pub fn from(c: &str, d: &str) -> ImportError {
		ImportError { cause: c.to_string(), description: d.to_string() }
	}
	pub fn new() -> ImportError {
		ImportError { cause: "Sparkle: Import Failure".to_string(), description: "Unknown error occured during scene import...".to_string() }
	}
}

pub fn load_gltf(path: &str, device: *mut dx11_1::ID3D11Device1, context: *mut dx11_1::ID3D11DeviceContext1) -> Result<Rc<RefCell<Node>>, ImportError> {
	let (gltf, buffers, images) = match gltf::import(path) {
		Ok(g) => g,
		Err(e) => return Err(ImportError::from("GLTF Import Error", e.description())),
	};
	let importer = GltfImporter{ 
		buffers: buffers,
		images: images,
		device: device,
		context: context,
	};
	let root = Node::create(None, glm::identity(), None);
	// multiple scenes?
	for scene in gltf.scenes() {
		for node in scene.nodes() {
			importer.process_node(node, &root)?;
		}
	}
	Ok(root)
}

impl GltfImporter {
	fn process_node(&self, node: gltf::scene::Node<'_>, parent: &Rc<RefCell<Node>>) -> Result<(), ImportError> {
		let mut parent = parent.clone();
		if !node.camera().is_some() {
			let transform : glm::Mat4 = glm::make_mat4(&(node.transform().matrix().concat()));
			if let Some(mesh) = node.mesh() {
				let mut drawables : Vec<Rc<RefCell<dyn Drawable>>> = Vec::new();
				for primitive in mesh.primitives() {
					let mut positions : Vec<glm::Vec3> = Vec::new();
					let mut indices : Vec<u32> = Vec::new();
					let mut normals : Vec<glm::Vec3> = Vec::new();
					let mut tex_coords : Vec<glm::Vec2> = Vec::new();
					let reader = primitive.reader(|buffer| Some(&self.buffers[buffer.index()]));
					if let Some(it) = reader.read_positions() {
						for vtx_pos in it {
							positions.push(glm::vec3(vtx_pos[0], vtx_pos[1], vtx_pos[2]));
						}
					}
					if let Some(it) = reader.read_indices() {
						for idx in it.into_u32() {
							indices.push(idx);
						}
					}
					if let Some(it) = reader.read_normals() {
						for norm in it {
							normals.push(glm::vec3(norm[0], norm[1], norm[2]));
						}
					}
					
					let mat = primitive.material();
					let pbr = mat.pbr_metallic_roughness();
					let alb = pbr.base_color_texture();
					// if alb.is_some() -> use alb tex, else use missing_tex placeholder
					let tex_color = match alb {
						Some(info) => {
							if let Some(it) = reader.read_tex_coords(info.tex_coord()) {
								for uv in it.into_f32() {
									tex_coords.push(glm::vec2(uv[0], uv[1]));
								}
							}

							let tx = info.texture().source();
							let img_raw = &self.images[tx.index()];
							let mut image_data : Vec<u8> = Vec::new();
							// todo: sampler
							use winapi::shared::dxgiformat as dx_format;
							let (img_data, fmt, channels) = match img_raw.format {
								gltf::image::Format::R8 => (&img_raw.pixels, dx_format::DXGI_FORMAT_R8_UNORM, 1),
								gltf::image::Format::R8G8 => (&img_raw.pixels, dx_format::DXGI_FORMAT_R8G8_UNORM, 2),
								gltf::image::Format::R8G8B8 => { // pad a 255 alpha value to make it rgba
									let len = (img_raw.width * img_raw.height * 4) as usize;
									image_data = vec![255; len]; // 4channel vec
									let mut src_index = 0;
									let mut ch = 0;
									for i in 0..len {
										if ch == 3 {
											ch = 0;
										} else {
											image_data[i] = img_raw.pixels[src_index];
											src_index += 1;
											ch += 1;
										}
									}
									(&image_data, dx_format::DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, 4)
								},
								gltf::image::Format::R8G8B8A8 => (&img_raw.pixels, dx_format::DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, 4),
								gltf::image::Format::B8G8R8 => {
									let len = (img_raw.width * img_raw.height * 4) as usize;
									image_data = vec![255; len]; // 4channel vec
									let mut src_index = 0;
									let mut ch = 0;
									for i in 0..len {
										if ch == 3 {
											ch = 0;
										} else {
											image_data[i] = img_raw.pixels[src_index];
											src_index += 1;
											ch += 1;
										}
									}
									(&image_data, dx_format::DXGI_FORMAT_B8G8R8X8_UNORM_SRGB, 4)
								},
								gltf::image::Format::B8G8R8A8 => (&img_raw.pixels, dx_format::DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, 4),
							};
							Texture2D::create_from_image_data(img_data, img_raw.width, img_raw.height, fmt, channels, self.device).expect(&format!("Unable to load texture with index {}", tx.index()))
						}, 
						None => {
							let img = image::open("images/textures/missing_tex.png").unwrap();
							Texture2D::create_from_image_obj(img, self.device).expect("Unable to load default texture")
						}
					};

					let mut vertices : Vec<Vertex> = Vec::new();
					for i in 0..positions.len() {
						let p = positions[i];
						let n = match i < normals.len() {
							true => normals[i],
							false => glm::zero(),
						};
						let uv = match i < tex_coords.len() {
							true => tex_coords[i],
							false => glm::zero(),
						};
						vertices.push(Vertex {
							position: p,
							normal: n,
							tex_coord: uv,
						});
					}
					let dx_prim = DxDrawable::from_verts(self.device, self.context, vertices, indices).expect("Initialization for DxPrimitive failed");
					dx_prim.borrow_mut().add_texture(0, tex_color);
					drawables.push(dx_prim);
				}
				let n = Node::create(node.name(), transform, Some(drawables));
				parent.borrow_mut().add_child(n.clone()).expect("Unable to add child node to parent..");
				parent = n;
			}
			for c in node.children() {
				self.process_node(c, &parent)?
			}
		}
		Ok(())
	}
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[Import Error] - {}: {}", self.cause, self.description)
    }
}

impl std::error::Error for ImportError {
    fn description(&self) -> &str {
        &self.description
    }
}