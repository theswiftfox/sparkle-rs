// gltf importer for sparkle-rs
use std::error::Error;
use std::cell::RefCell;
use std::rc::Rc;

use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

use crate::engine::geometry::Vertex;
use crate::engine::scenegraph::node::Node;
use crate::engine::scenegraph::drawable::{Drawable,ObjType};
use crate::engine::d3d11::drawable::DxDrawable;
use crate::engine::d3d11::textures::Texture2D;

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

fn convert_3ch_to_4ch_img(image: &gltf::image::Data) -> Vec<u8> {
	let len = (image.width * image.height * 4) as usize;
	let mut image_data = vec![255; len]; // 4channel vec
	let mut src_index = 0;
	let mut ch = 0;
	for i in 0..len {
		if ch == 3 {
			ch = 0;
		} else {
			image_data[i] = image.pixels[src_index];
			src_index += 1;
			ch += 1;
		}
	}
	image_data
}

fn gltf_address_mode_to_dx(mode: gltf::texture::WrappingMode) -> u32 {
	match mode {
			gltf::texture::WrappingMode::ClampToEdge => dx11::D3D11_TEXTURE_ADDRESS_CLAMP,
			gltf::texture::WrappingMode::Repeat => dx11::D3D11_TEXTURE_ADDRESS_WRAP,
			gltf::texture::WrappingMode::MirroredRepeat => dx11::D3D11_TEXTURE_ADDRESS_MIRROR,
	}
}
fn gltf_filter_to_dx(filter_min: gltf::texture::MinFilter, filter_mag: gltf::texture::MagFilter) -> u32 {
	match filter_mag {
		gltf::texture::MagFilter::Linear => {
			match filter_min {
				gltf::texture::MinFilter::Nearest => dx11::D3D11_FILTER_MIN_POINT_MAG_LINEAR_MIP_POINT,
				gltf::texture::MinFilter::Linear => dx11::D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
				gltf::texture::MinFilter::NearestMipmapNearest => dx11::D3D11_FILTER_MIN_POINT_MAG_LINEAR_MIP_POINT,
				gltf::texture::MinFilter::LinearMipmapNearest => dx11::D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
				gltf::texture::MinFilter::NearestMipmapLinear => dx11::D3D11_FILTER_MIN_POINT_MAG_MIP_LINEAR,
				gltf::texture::MinFilter::LinearMipmapLinear => dx11::D3D11_FILTER_MIN_MAG_MIP_LINEAR,
			}
		},
		gltf::texture::MagFilter::Nearest => {
			match filter_min {
				gltf::texture::MinFilter::Nearest => dx11::D3D11_FILTER_MIN_MAG_MIP_POINT,
				gltf::texture::MinFilter::Linear => dx11::D3D11_FILTER_MIN_LINEAR_MAG_MIP_POINT,
				gltf::texture::MinFilter::NearestMipmapNearest => dx11::D3D11_FILTER_MIN_MAG_MIP_POINT,
				gltf::texture::MinFilter::LinearMipmapNearest => dx11::D3D11_FILTER_MIN_LINEAR_MAG_MIP_POINT,
				gltf::texture::MinFilter::NearestMipmapLinear => dx11::D3D11_FILTER_MIN_MAG_POINT_MIP_LINEAR,
				gltf::texture::MinFilter::LinearMipmapLinear => dx11::D3D11_FILTER_MIN_LINEAR_MAG_POINT_MIP_LINEAR,
			}
		}
	}
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
					let mut tex_coords_normalmap : Vec<glm::Vec2> = Vec::new();
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
					};

					let tangents_raw : Vec<glm::Vec4> = match reader.read_tangents() {
						Some(it) => {
							let mut trvec : Vec<glm::Vec4> = Vec::new();
							for tang in it {
								trvec.push(glm::vec4(tang[0], tang[1], tang[2], tang[3]));
							}
							trvec
						},
						None => Vec::new()
					};
			
					let mat = primitive.material();
					let pbr = mat.pbr_metallic_roughness();
					let alb = pbr.base_color_texture();

					let address_mode = dx11::D3D11_TEXTURE_ADDRESS_WRAP;
					let filter = dx11::D3D11_FILTER_MIN_MAG_MIP_LINEAR;

					// if alb.is_some() -> use alb tex, else use missing_tex placeholder
					let (tex_color, transparent) = match alb {
						Some(info) => {
							if let Some(it) = reader.read_tex_coords(info.tex_coord()) {
								for uv in it.into_f32() {
									tex_coords.push(glm::vec2(uv[0], uv[1]));
								}
							}
							let tx = info.texture();
							self.dx_tex_from_gltf(tx)
						}, 
						None => {
							let img = image::open("images/textures/missing_tex.png").unwrap();
							// let transparent = match img.color() {
							// 	image::ColorType::GrayA(_) | image::ColorType::RGBA(_) | image::ColorType::BGRA(_) => true,
							// 	_ => false,
							// };
							(Texture2D::create_from_image_obj(img, address_mode, address_mode, filter, self.device).expect("Unable to load default texture"), false)
						}
					};

					let tex_mr = match pbr.metallic_roughness_texture() {
						Some(info) => {
							let tx = info.texture();
							self.dx_tex_from_gltf(tx).0
						},
						None => {
							let img = image::open("images/textures/missing_mr_tex.png").unwrap();
							Texture2D::create_from_image_obj(img, address_mode, address_mode, filter, self.device).expect("Unable to load default texture")
						}
					};

					let tex_norm = match mat.normal_texture() {
						Some(info) => {
							if let Some(it) = reader.read_tex_coords(info.tex_coord()) {
								for uv in it.into_f32() {
									tex_coords_normalmap.push(glm::vec2(uv[0], uv[1]));
								}
							}
							let tx = info.texture();
							self.dx_tex_from_gltf(tx).0
						},
						None => {
							let img = image::open("images/textures/missing_tex.png").unwrap();
							Texture2D::create_from_image_obj(img, address_mode, address_mode, filter, self.device).expect("Unable to load default texture")
						}
					};

					let tangents_raw = match tangents_raw.len() > 0 {
						true => tangents_raw,
						false => {
							let mut trvec : Vec<glm::Vec4> = Vec::new(); 
							// calculate tangents - bugged
							if tex_coords.len() != positions.len() {
								panic!("No UV Coordinates provided!");
							}
							let mut index = 0;
							for _ in 0 .. (indices.len() / 3) {
								let i0 = indices[index] as usize;
								let i1 = indices[index+1] as usize;
								let i2 = indices[index+2] as usize;

								let v0 = positions[i0];
								let v1 = positions[i1];
								let v2 = positions[i2];

								let w0 = tex_coords[i0];
								let w1 = tex_coords[i1];
								let w2 = tex_coords[i2];

								let x0 = v1.x - v0.x;
								let x1 = v2.x - v0.x;
								let y0 = v1.y - v0.y;
								let y1 = v2.y - v0.y;
								let z0 = v1.z - v0.z;
								let z1 = v2.z - v0.z;

								let s0 = w1.x - w0.x;
								let s1 = w2.x - w0.x;
								let t0 = w1.y - w0.y;
								let t1 = w2.y - w0.y;

								let r = 1.0f32 / (s0 * t1 - s1 * t0);
								let sdir = glm::vec3(
									(t1 * x0 - t0 * x1) * r,
									(t1 * y0 - t0 * y1) * r,
									(t1 * z0 - t0 * z1) * r,
								);
								let tdir = glm::vec3(
									(s0 * x1 - s1 * x0) * r,
									(s0 * y1 - s1 * y0) * r,
									(s0 * z1 - s1 * z0) * r,
								);
								
								let normal = normals[i0];
								// Gram-Schmidt orthogonalize
								let t = (sdir - normal * normal.dot(&sdir)).normalize();
								let w = match normal.cross(&sdir).dot(&tdir) < 0.0f32 {
									true => -1.0f32,
									false => 1.0f32,
								};
								let tangent = glm::vec4(t.x, t.y, t.z, w);

								trvec.push(tangent);
								trvec.push(tangent);
								trvec.push(tangent);

								index = index + 3;
							}
							trvec
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
						let t = match i < tangents_raw.len() {
							true => tangents_raw[i],
							false => glm::zero(),
						};
						let bt = n.cross(&t.xyz()) * t.w;
						let uv_nm = match i < tex_coords_normalmap.len() {
							true => tex_coords_normalmap[i],
							false => glm::zero(),
						};
						vertices.push(Vertex {
							position: p,
							normal: n,
							tangent: t.xyz() * t.w,
							bitangent: bt,
							tex_coord: uv,
							tex_coord_normalmap: uv_nm,
						});
					}
					let dx_prim = DxDrawable::from_verts(
						self.device, 
						self.context, 
						vertices, 
						indices, 
						match transparent {
							true => ObjType::Transparent,
							false => ObjType::Opaque,
						}
					).expect("Initialization for DxPrimitive failed");
					dx_prim.borrow_mut().add_texture(0, tex_color);
					dx_prim.borrow_mut().add_texture(1, tex_mr);
					dx_prim.borrow_mut().add_texture(2, tex_norm);
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

	/***
	 * returns a tuple (texture, transparent)
	 */
	fn dx_tex_from_gltf(&self, gltf_tex: gltf::Texture) -> (Texture2D, bool) {
		let img = gltf_tex.source();
		let img_raw = &self.images[img.index()];
		let mut image_data : Vec<u8> = Vec::new();
		let sampler = gltf_tex.sampler();
		let wrap_u = gltf_address_mode_to_dx(sampler.wrap_s());
		let wrap_v = gltf_address_mode_to_dx(sampler.wrap_t());
		let filter = match (sampler.min_filter(), sampler.mag_filter()) {
			(Some(min), Some(mag)) => gltf_filter_to_dx(min, mag),
			_ => dx11::D3D11_FILTER_MIN_MAG_MIP_LINEAR,
		};
		let mut transparent = false;
		use winapi::shared::dxgiformat as dx_format;
		let (img_data, fmt, channels) = match img_raw.format {
			gltf::image::Format::R8 => (&img_raw.pixels, dx_format::DXGI_FORMAT_R8_UNORM, 1),
			gltf::image::Format::R8G8 => (&img_raw.pixels, dx_format::DXGI_FORMAT_R8G8_UNORM, 2),
			gltf::image::Format::R8G8B8 => { // pad a 255 alpha value to make it rgba
				image_data = convert_3ch_to_4ch_img(img_raw);
				(&image_data, dx_format::DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, 4)
			},
			gltf::image::Format::R8G8B8A8 => {
				transparent = true;
				(&img_raw.pixels, dx_format::DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, 4)
			},
			gltf::image::Format::B8G8R8 => {
				image_data = convert_3ch_to_4ch_img(img_raw);
				(&image_data, dx_format::DXGI_FORMAT_B8G8R8X8_UNORM_SRGB, 4)
			},
			gltf::image::Format::B8G8R8A8 => {
				transparent = true;
				(&img_raw.pixels, dx_format::DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, 4)
			},
		};
		(Texture2D::create_from_image_data(
			img_data, img_raw.width, 
			img_raw.height, 
			fmt, channels, 
			wrap_u, wrap_v, 
			filter, self.device).expect(&format!("Unable to load texture with index {}", gltf_tex.index())),
			transparent)
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