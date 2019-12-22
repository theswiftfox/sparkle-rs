// gltf importer for sparkle-rs
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;

use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

use crate::engine::d3d11::drawable::{DxDrawable, ObjType};
use crate::engine::d3d11::textures::Texture2D;
use crate::engine::geometry::Vertex;
use crate::engine::scenegraph::node::Node;

#[derive(Debug, Clone)]
pub struct ImportError {
	cause: String,
	description: String,
}

struct GltfImporter {
	buffers: Vec<gltf::buffer::Data>,
	images: Vec<gltf::image::Data>,
	device: *mut dx11_1::ID3D11Device1,
	context: *mut dx11_1::ID3D11DeviceContext1,
	texture_buffer: HashMap<usize, (Rc<RefCell<Texture2D>>, bool)>,
	missing_tex: Rc<RefCell<Texture2D>>,
}

impl ImportError {
	pub fn from(c: &str, d: &str) -> ImportError {
		ImportError {
			cause: c.to_string(),
			description: d.to_string(),
		}
	}
	pub fn new() -> ImportError {
		ImportError {
			cause: "Sparkle: Import Failure".to_string(),
			description: "Unknown error occured during scene import...".to_string(),
		}
	}
}

pub fn load_gltf(
	path: &str,
	device: *mut dx11_1::ID3D11Device1,
	context: *mut dx11_1::ID3D11DeviceContext1,
) -> Result<Rc<RefCell<Node>>, ImportError> {
	let (gltf, buffers, images) = match gltf::import(path) {
		Ok(g) => g,
		Err(e) => return Err(ImportError::from("GLTF Import Error", e.description())),
	};
	let img = image::open("images/textures/missing_tex.png").unwrap();
	let missing_tex = Rc::from(RefCell::from(
		Texture2D::create_from_image_obj(
			img,
			dx11::D3D11_TEXTURE_ADDRESS_CLAMP,
			dx11::D3D11_TEXTURE_ADDRESS_CLAMP,
			dx11::D3D11_FILTER_MIN_MAG_MIP_POINT,
			0,
			device,
			context,
		)
		.expect("Unable to load default texture"),
	));
	let mut importer = GltfImporter {
		buffers: buffers,
		images: images,
		device: device,
		context: context,
		texture_buffer: HashMap::new(),
		missing_tex: missing_tex,
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
fn gltf_filter_to_dx(
	filter_min: gltf::texture::MinFilter,
	filter_mag: gltf::texture::MagFilter,
) -> u32 {
	match filter_mag {
		gltf::texture::MagFilter::Linear => match filter_min {
			gltf::texture::MinFilter::Nearest => dx11::D3D11_FILTER_MIN_POINT_MAG_LINEAR_MIP_POINT,
			gltf::texture::MinFilter::Linear => dx11::D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
			gltf::texture::MinFilter::NearestMipmapNearest => {
				dx11::D3D11_FILTER_MIN_POINT_MAG_LINEAR_MIP_POINT
			}
			gltf::texture::MinFilter::LinearMipmapNearest => {
				dx11::D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT
			}
			gltf::texture::MinFilter::NearestMipmapLinear => {
				dx11::D3D11_FILTER_MIN_POINT_MAG_MIP_LINEAR
			}
			gltf::texture::MinFilter::LinearMipmapLinear => dx11::D3D11_FILTER_MIN_MAG_MIP_LINEAR,
		},
		gltf::texture::MagFilter::Nearest => match filter_min {
			gltf::texture::MinFilter::Nearest => dx11::D3D11_FILTER_MIN_MAG_MIP_POINT,
			gltf::texture::MinFilter::Linear => dx11::D3D11_FILTER_MIN_LINEAR_MAG_MIP_POINT,
			gltf::texture::MinFilter::NearestMipmapNearest => dx11::D3D11_FILTER_MIN_MAG_MIP_POINT,
			gltf::texture::MinFilter::LinearMipmapNearest => {
				dx11::D3D11_FILTER_MIN_LINEAR_MAG_MIP_POINT
			}
			gltf::texture::MinFilter::NearestMipmapLinear => {
				dx11::D3D11_FILTER_MIN_MAG_POINT_MIP_LINEAR
			}
			gltf::texture::MinFilter::LinearMipmapLinear => {
				dx11::D3D11_FILTER_MIN_LINEAR_MAG_POINT_MIP_LINEAR
			}
		},
	}
}

impl GltfImporter {
	fn process_node(
		&mut self,
		node: gltf::scene::Node<'_>,
		parent: &Rc<RefCell<Node>>,
	) -> Result<(), ImportError> {
		let mut parent = parent.clone();
		if !node.camera().is_some() {
			let transform: glm::Mat4 = glm::make_mat4(&(node.transform().matrix().concat()));
			if let Some(mesh) = node.mesh() {
				let mut drawables: Vec<Rc<RefCell<DxDrawable>>> = Vec::new();
				for primitive in mesh.primitives() {
					let mat = primitive.material();
					let pbr = mat.pbr_metallic_roughness();
					let alb = pbr.base_color_texture();

					let mut positions: Vec<glm::Vec3> = Vec::new();
					let mut indices: Vec<u32> = Vec::new();
					let mut normals: Vec<glm::Vec3> = Vec::new();
					let mut tex_coords: Vec<glm::Vec2> = Vec::new();
					// let mut tex_coords_normalmap: Vec<glm::Vec2> = Vec::new();
					let mut tangents_raw: Vec<glm::Vec4> = Vec::new();
					{
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

						tangents_raw = match reader.read_tangents() {
							Some(it) => {
								let mut trvec: Vec<glm::Vec4> = Vec::new();
								for tang in it {
									trvec.push(glm::vec4(tang[0], tang[1], tang[2], tang[3]));
								}
								trvec
							}
							None => Vec::new(),
						};
						// if let Some(info) = mat.normal_texture() {
						// 	if let Some(it) = reader.read_tex_coords(info.tex_coord()) {
						// 		for uv in it.into_f32() {
						// 			tex_coords_normalmap.push(glm::vec2(uv[0], uv[1]));
						// 		}
						// 	}
						// }
						if let Some(info) = &alb {
							if let Some(it) = reader.read_tex_coords(info.tex_coord()) {
								for uv in it.into_f32() {
									tex_coords.push(glm::vec2(uv[0], uv[1]));
								}
							}
						}
					}

					// if alb.is_some() -> use alb tex, else use missing_tex placeholder
					let (tex_color, transparent) = match alb {
						Some(info) => {
							let tx = info.texture();
							self.dx_tex_from_gltf(tx, true)
						}
						None => (self.missing_tex.clone(), false),
					};

					let tex_mr = match pbr.metallic_roughness_texture() {
						Some(info) => {
							let tx = info.texture();
							self.dx_tex_from_gltf(tx, true).0
						}
						None => self.missing_tex.clone(),
					};

					let tex_norm = match mat.normal_texture() {
						Some(info) => {
							let tx = info.texture();
							self.dx_tex_from_gltf(tx, false).0
						}
						None => self.missing_tex.clone(),
					};

					let (tangents, bitangents) = match tangents_raw.len() > 0 {
						true => {
							let mut bts = Vec::<glm::Vec3>::new();
							let mut ts = Vec::<glm::Vec3>::new();
							for i in 0..tangents_raw.len() {
								let n = normals[i];
								let t = tangents_raw[i];
								let bt = n.cross(&t.xyz()) * t.w;
								bts.push(bt);
								ts.push(t.xyz());
							}
							(ts, bts)
						}
						false => {
							let mut trvec = Vec::<glm::Vec3>::new();
							let mut btvec = Vec::<glm::Vec3>::new();
							// calculate tangents
							if tex_coords.len() != positions.len() {
								panic!("No UV Coordinates provided!");
							}
							let mut index = 0;
							for _ in 0..(indices.len() / 3) {
								let i0 = indices[index] as usize;
								let i1 = indices[index + 1] as usize;
								let i2 = indices[index + 2] as usize;

								let v0 = positions[i0];
								let v1 = positions[i1];
								let v2 = positions[i2];

								let w0 = tex_coords[i0];
								let w1 = tex_coords[i1];
								let w2 = tex_coords[i2];

								let e1 = v1 - v0;
								let e2 = v2 - v0;
								let x1 = w1.x - w0.x;
								let x2 = w2.x - w0.x;
								let y1 = w1.y - w0.y;
								let y2 = w2.y - w0.y;

								let r = 1.0 / (x1 * y2 - x2 * y1);
								let t = (e1 * y2 - e2 * y1) * r;
								let b = (e2 * x1 - e1 * x2) * r;

								// let w = match t.cross(&b).dot(&n0) > 0.0 {
								// 	true => 1.0,
								// 	false => -1.0,
								// };

								// let tangent = glm::vec4(t.x, t.y, t.z, w);

								trvec.push(t);
								trvec.push(t);
								trvec.push(t);
								btvec.push(b);
								btvec.push(b);
								btvec.push(b);

								index = index + 3;
							}
							(trvec, btvec)
						}
					};

					let mut vertices: Vec<Vertex> = Vec::new();
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
						let t = match i < tangents.len() {
							true => tangents[i],
							false => glm::zero(),
						};
						let bt = match i < bitangents.len() {
							true => bitangents[i],
							false => glm::zero(),
						};
						// let uv_nm = match i < tex_coords_normalmap.len() {
						// 	true => tex_coords_normalmap[i],
						// 	false => glm::zero(),
						// };
						vertices.push(Vertex {
							position: p,
							normal: n,
							tangent: t,
							bitangent: bt,
							tex_coord: uv,
							//			tex_coord_normalmap: uv_nm,
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
						},
					)
					.expect("Initialization for DxPrimitive failed");
					dx_prim.borrow_mut().add_texture(0, tex_color);
					dx_prim.borrow_mut().add_texture(1, tex_mr);
					dx_prim.borrow_mut().add_texture(2, tex_norm);
					drawables.push(dx_prim);
				}
				let n = Node::create(node.name(), transform, Some(drawables));
				parent
					.borrow_mut()
					.add_child(n.clone())
					.expect("Unable to add child node to parent..");
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
	fn dx_tex_from_gltf(
		&mut self,
		gltf_tex: gltf::Texture,
		srgb: bool,
	) -> (Rc<RefCell<Texture2D>>, bool) {
		let index = gltf_tex.index();
		if let Some((tex, transparent)) = &self.texture_buffer.get(&index) {
			(tex.clone(), transparent.clone())
		} else {
			let img = gltf_tex.source();
			let img_raw = &self.images[img.index()];
			let mut image_data: Vec<u8> = Vec::new();
			let sampler = gltf_tex.sampler();
			let wrap_u = gltf_address_mode_to_dx(sampler.wrap_s());
			let wrap_v = gltf_address_mode_to_dx(sampler.wrap_t());
			// let filter = match (sampler.min_filter(), sampler.mag_filter()) {
			// 	(Some(min), Some(mag)) => gltf_filter_to_dx(min, mag),
			// 	_ => dx11::D3D11_FILTER_ANISOTROPIC, //dx11::D3D11_FILTER_MIN_MAG_MIP_LINEAR,
			// };
			let filter = dx11::D3D11_FILTER_ANISOTROPIC;
			let mut transparent = false;
			use winapi::shared::dxgiformat as dx_format;
			let (img_data, fmt, channels) = match img_raw.format {
				gltf::image::Format::R8 => (&img_raw.pixels, dx_format::DXGI_FORMAT_R8_UNORM, 1),
				gltf::image::Format::R8G8 => {
					(&img_raw.pixels, dx_format::DXGI_FORMAT_R8G8_UNORM, 2)
				}
				gltf::image::Format::R8G8B8 => {
					// pad a 255 alpha value to make it rgba
					image_data = convert_3ch_to_4ch_img(img_raw);
					let format = match srgb {
						true => dx_format::DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
						false => dx_format::DXGI_FORMAT_R8G8B8A8_UNORM,
					};
					(&image_data, format, 4)
				}
				gltf::image::Format::R8G8B8A8 => {
					for i in (3..(img_raw.width * img_raw.height * 4) as usize).step_by(4) {
						if img_raw.pixels[i] < 255 {
							transparent = true;
							break;
						}
					}
					// for i in (3..(img_raw.width * img_raw.height * 4) as usize).step_by(4) {
					// 	if img_raw.pixels[i] == 255 {
					// 		count_ones += 1;
					// 	} else if img_raw.pixels[i] == 0 {
					// 		count_zeroes += 1;
					// 	}
					// }
					// if count_zeroes == (img_raw.width * img_raw.height) as usize ||
					// 	count_ones == (img_raw.width * img_raw.height) as usize
					// {
					// 	transparent = false;
					// }
					let format = match srgb {
						true => dx_format::DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
						false => dx_format::DXGI_FORMAT_R8G8B8A8_UNORM,
					};
					(&img_raw.pixels, format, 4)
				}
				gltf::image::Format::B8G8R8 => {
					image_data = convert_3ch_to_4ch_img(img_raw);
					(&image_data, dx_format::DXGI_FORMAT_B8G8R8X8_UNORM, 4)
				}
				gltf::image::Format::B8G8R8A8 => {
					for i in (3..(img_raw.width * img_raw.height * 4) as usize).step_by(4) {
						if img_raw.pixels[i] < 255 {
							transparent = true;
							break;
						}
					}
					let format = match srgb {
						true => dx_format::DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
						false => dx_format::DXGI_FORMAT_B8G8R8A8_UNORM,
					};
					(&img_raw.pixels, format, 4)
				}
			};
			let tex = Rc::from(RefCell::from(
				Texture2D::create_from_image_data(
					img_data,
					img_raw.width,
					img_raw.height,
					fmt,
					channels,
					wrap_u,
					wrap_v,
					filter,
					0,
					self.device,
					self.context,
				)
				.expect(&format!(
					"Unable to load texture with index {}",
					gltf_tex.index()
				)),
			));
			self.texture_buffer
				.insert(index, (tex.clone(), transparent));
			(tex, transparent)
		}
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
