use std::collections::HashMap;
use std::rc::Rc;

use super::{ErrorCause, SceneGraphError};
use crate::engine::backend::{Drawable, GpuBackend, IndirectDrawable, RenderItem};
use crate::engine::geometry::AABB;

pub struct Node<B: GpuBackend> {
    uuid: u64,
    pub name: Option<String>,
    model: glm::Mat4,
    model_orig: glm::Mat4,
    children: HashMap<String, Node<B>>,

    data: NodeData<B>,
}

pub enum NodeData<B: GpuBackend> {
    StandardMesh(Vec<Drawable<B>>),
    ProceduralWorld {
        terrain: Drawable<B>,
        instanced_assets: Vec<IndirectDrawable<B>>,
        heightmap: Rc<B::Texture>,
    },
}

impl<B: GpuBackend> NodeData<B> {
    pub fn clear(&mut self) {
        match self {
            NodeData::StandardMesh(drawables) => drawables.clear(),
            NodeData::ProceduralWorld {
                terrain: _,
                instanced_assets: _,
                heightmap: _,
            } => (),
        }
    }

    fn update_model(&mut self, backend: &B, model: &glm::Mat4) {
        match self {
            NodeData::StandardMesh(drawables) => drawables
                .iter_mut()
                .for_each(|d| d.update_model(backend, model)),
            NodeData::ProceduralWorld {
                terrain: _,
                instanced_assets: _,
                heightmap: _,
            } => {
                () // no op for procedural terrain i think
            }
        }
    }
}

impl<B: GpuBackend> Clone for NodeData<B> {
    fn clone(&self) -> Self {
        match self {
            Self::StandardMesh(arg0) => Self::StandardMesh(arg0.clone()),
            Self::ProceduralWorld {
                terrain,
                instanced_assets,
                heightmap,
            } => Self::ProceduralWorld {
                terrain: terrain.clone(),
                instanced_assets: instanced_assets.clone(),
                heightmap: heightmap.clone(),
            },
        }
    }
}

// Manual Clone: derive would add unnecessary B: Clone bound.
// All fields are cloneable without B being Clone (Rc handles it).
impl<B: GpuBackend> Clone for Node<B> {
    fn clone(&self) -> Self {
        Node {
            uuid: self.uuid,
            name: self.name.clone(),
            model: self.model,
            model_orig: self.model_orig,
            children: self.children.clone(),
            data: self.data.clone(),
        }
    }
}

impl<B: GpuBackend> Node<B> {
    pub fn create_standard_mesh(
        name: Option<&str>,
        model: glm::Mat4,
        drawable: Option<Vec<Drawable<B>>>,
    ) -> Node<B> {
        let mut n = Node {
            uuid: 0, // TODO
            name: match name {
                Some(n) => Some(n.to_string()),
                None => None,
            },
            model,
            model_orig: model,
            data: NodeData::StandardMesh(Vec::new()),
            children: HashMap::new(),
        };
        if let Some(d) = drawable {
            n.data = NodeData::StandardMesh(d);
        }
        n
    }

    pub fn create_procedural_world(
        name: Option<&str>,
        terrain: Drawable<B>,
        instanced_assets: Vec<IndirectDrawable<B>>,
        heightmap: Rc<B::Texture>,
    ) -> Node<B> {
        Node {
            uuid: 0,
            name: name.map(String::from),
            model: glm::Mat4::identity(),
            model_orig: glm::Mat4::identity(),
            data: NodeData::ProceduralWorld {
                terrain,
                instanced_assets,
                heightmap,
            },
            children: HashMap::new(),
        }
    }

    pub fn destroy(&mut self) {
        self.data.clear();
        for (_, mut c) in self.children.drain() {
            c.destroy();
        }
        self.children.clear();
    }

    pub fn get_named(&self, name: &str) -> Result<&Node<B>, SceneGraphError> {
        if self.children.is_empty() {
            return Err(SceneGraphError::new(name, &ErrorCause::NotFound));
        }
        if let Some(c) = self.children.get(name) {
            return Ok(c);
        }
        for (_, node) in &self.children {
            match node.get_named(name) {
                Ok(c) => return Ok(c),
                Err(e) => return Err(e),
            }
        }
        Err(SceneGraphError::new(name, &ErrorCause::NotFound))
    }

    pub fn get_named_mut(&mut self, name: &str) -> Result<&mut Node<B>, SceneGraphError> {
        if self.children.is_empty() {
            return Err(SceneGraphError::new(name, &ErrorCause::NotFound));
        }
        if self.children.contains_key(name) {
            return Ok(self.children.get_mut(name).unwrap());
        }
        for (_, node) in &mut self.children {
            match node.get_named_mut(name) {
                Ok(c) => return Ok(c),
                Err(e) => return Err(e),
            }
        }
        Err(SceneGraphError::new(name, &ErrorCause::NotFound))
    }

    pub fn get_drawables(&self) -> Vec<RenderItem<'_, B>> {
        match &self.data {
            NodeData::StandardMesh(drawables) => drawables
                .iter()
                .map(|it| RenderItem::Standard(it))
                .collect(),
            NodeData::ProceduralWorld {
                terrain,
                instanced_assets,
                heightmap: _,
            } => {
                let mut drawables = vec![RenderItem::Standard(terrain)];
                for d in instanced_assets {
                    drawables.push(RenderItem::Indirect(d))
                }
                drawables
            }
        }
    }

    pub fn traverse(&self) -> Vec<&Node<B>> {
        let mut nodes: Vec<&Node<B>> = Vec::new();
        for (_, c) in &self.children {
            nodes.push(c);
            let mut others = c.traverse();
            nodes.append(&mut others);
        }
        nodes
    }

    pub fn add_child(&mut self, node: Node<B>) -> Result<(), SceneGraphError> {
        let key = match &node.name {
            Some(n) => n.clone(),
            None => node.uuid.to_string(),
        };
        if self.children.contains_key(&key) {
            return Err(SceneGraphError::new(
                "Duplicate Name",
                &ErrorCause::InvalidState,
            ));
        }
        self.children.insert(key, node);
        Ok(())
    }

    pub fn remove_node_named(&mut self, name: &str) -> bool {
        if self.children.is_empty() {
            return false;
        }
        if let Some(mut v) = self.children.remove(name) {
            v.destroy();
            return true;
        }
        for (_, c) in &mut self.children {
            if c.remove_node_named(name) {
                return true;
            }
        }
        false
    }

    pub fn remove_node_uuid(&mut self, uuid: u64) -> bool {
        if self.children.is_empty() {
            return false;
        }
        let mut key = String::default();
        for (k, c) in &self.children {
            if c.uuid == uuid {
                key = k.clone();
                break;
            }
        }
        if !key.is_empty() {
            let mut n = self.children.remove(&key).unwrap();
            n.destroy();
            return true;
        }
        for (_, c) in &mut self.children {
            if c.remove_node_uuid(uuid) {
                return true;
            }
        }
        false
    }

    pub fn add_drawable(&mut self, drawable: Drawable<B>) {
        if let NodeData::StandardMesh(drawables) = &mut self.data {
            drawables.push(drawable)
        } else {
            eprintln!("Add Drawable called on procedural data! Not adding..")
        }
    }

    pub fn translate(&mut self, t: glm::Vec3) {
        self.model = glm::translate(&self.model, &t);
    }

    pub fn rotate(&mut self, r: glm::Quat) {
        let rot = glm::quat_to_mat4(&r);
        self.model = rot * self.model;
    }

    pub fn scale(&mut self, s: f32) {
        self.model = glm::scale(&self.model, &glm::vec3(s, s, s));
    }

    /// Returns the local transform matrix (model_orig).
    pub fn local_transform(&self) -> glm::Mat4 {
        self.model_orig
    }

    /// Returns the computed world-space transform matrix.
    pub fn world_transform(&self) -> glm::Mat4 {
        self.model
    }

    /// Set the local transform matrix (model_orig).
    ///
    /// Call `build_model()` afterwards to propagate to children.
    pub fn set_local_transform(&mut self, mat: glm::Mat4) {
        self.model_orig = mat;
    }

    /// Returns a list of direct children as `Rc<RefCell<Node<B>>>`.
    ///
    /// Order is not guaranteed (HashMap iteration order).
    pub fn children_list(&self) -> Vec<&Node<B>> {
        self.children.values().collect()
    }

    /// Returns the number of drawables on this node.
    pub fn num_drawables(&self) -> usize {
        match &self.data {
            NodeData::StandardMesh(drawables) => drawables.len(),
            NodeData::ProceduralWorld {
                terrain: _,
                instanced_assets,
                heightmap: _,
            } => 1 + instanced_assets.len(),
        }
    }

    pub fn get_bounding_volume(&self) {}

    /// Compute the local-space AABB for this node by merging all drawable AABBs.
    ///
    /// Returns `AABB::empty()` if the node has no drawables.
    pub fn local_aabb(&self) -> AABB {
        let mut aabb = AABB::empty();
        match &self.data {
            NodeData::StandardMesh(drawables) => {
                drawables.iter().for_each(|d| aabb.merge(d.aabb()))
            }
            NodeData::ProceduralWorld {
                terrain: _,
                instanced_assets: _,
                heightmap: _,
            } => {
                // procedural terrain has no bounding box.
                ()
            }
        }
        aabb
    }

    /// Compute the world-space AABB by transforming the local AABB.
    pub fn world_aabb(&self) -> AABB {
        self.local_aabb().transformed(&self.model)
    }

    pub fn build_model(&mut self, backend: &B, model: &glm::Mat4) {
        self.model = model * self.model_orig;
        // for drawable in &mut self.drawables {
        //     drawable.update_model(backend, &self.model);
        // }
        self.data.update_model(backend, &self.model);
        for (_, c) in &mut self.children {
            c.build_model(backend, &self.model);
        }
    }
}

pub fn collect_drawables<B: GpuBackend>(node: &Node<B>, out: &mut Vec<Drawable<B>>) {
    if let NodeData::StandardMesh(drawables) = &node.data {
        out.extend(drawables.iter().cloned());
    }
    for child in node.children_list() {
        collect_drawables(child, out);
    }
}
