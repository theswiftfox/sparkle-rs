// gltf importer for sparkle-rs
use std::error::Error;
use std::cell::RefCell;
use std::rc::Rc as shared_ptr;
use gltf::Gltf;

use crate::drawing::scenegraph::node::Node;

pub struct ImportError {
	cause: String,
	description: String,
}

impl ImportError {
	pub fn from(c: &str, d: &str) -> ImportError {
		ImportError { cause: c.to_string(), description: d.to_string() }
	}
	pub fn new() -> ImportError {
		ImportError { cause: "Sparkle: Import Failure".to_string(), description: "Unknown error occured during scene import...".to_string() }
	}
}

pub fn load_gltf(path: &str) -> Result<shared_ptr<RefCell<Node>>, ImportError> {
	let gltf = match Gltf::open(path) {
		Ok(g) => g,
		Err(e) => return Err(ImportError::from("GLTF Import Error", e.description())),
	};
	let root = Node::create(None, glm::identity(), None);
	// multiple scenes?
	for scene in gltf.scenes() {
		for node in scene.nodes() {

		}
	}
	Err(ImportError::new())
}

fn process_node(node: gltf::scene::Node<'_>, parent: shared_ptr<RefCell<Node>>) -> Result<(), ImportError> {


	Ok(())
}