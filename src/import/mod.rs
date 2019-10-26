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
	Err(ImportError::new())
}

impl GltfImporter {
	fn process_node(&self, node: gltf::scene::Node<'_>, parent: &Rc<RefCell<Node>>) -> Result<(), ImportError> {
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
				if let Some(it) = reader.read_tex_coords(0) { // todo: what is "set" param in read_tex_coords here??
					for uv in it.into_f32() {
						tex_coords.push(glm::vec2(uv[0], uv[1]));
					}
				}
				
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
				let mat = primitive.material();
				let pbr = mat.pbr_metallic_roughness();
				let alb = pbr.base_color_texture();
				// if alb.is_some() -> use alb tex, else use missing_tex placeholder
				let tex_color = match alb {
					Some(info) => {
						let tx = info.texture().source();
						let img = image::load_from_memory(self.images[tx.index()].pixels.as_ref()).unwrap();
						/*match tx.source() {
							gltf::image::Source::View { view, .. } => {
								// init image from existing data in view
								image::load_from_memory(view.)
							},
							gltf::image::Source::Uri { uri, .. } => {
								image::open(uri).unwrap()
							}
						}; */
						// todo: sampler
						Texture2D::create_from_image(img, self.device).expect(&format!("Unable to load texture with index {}", tx.index()))
					}, 
					None => {
						let img = image::open("images/textures/missing_tex.png").unwrap();
						Texture2D::create_from_image(img, self.device).expect("Unable to load default texture")
					}
				};
				dx_prim.borrow_mut().add_texture(0, tex_color);
				drawables.push(dx_prim);
			}
			// let drawable = DxDrawable::from_verts();
		} 


		Ok(())
	}
}