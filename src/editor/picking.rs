//! Viewport picking via ray-AABB intersection.
//!
//! Converts a screen-space mouse click into a 3D ray, then tests the ray
//! against all node world-space AABBs to find the closest hit.

use crate::engine::geometry::AABB;
use crate::engine::scene_info::NodeInfo;

/// A ray defined by an origin and a direction.
pub struct Ray {
    pub origin: glm::Vec3,
    pub direction: glm::Vec3,
}

/// Result of a picking operation.
pub struct PickResult {
    /// Name of the picked node.
    pub node_name: String,
    /// Distance from the ray origin to the hit point.
    pub distance: f32,
}

/// Construct a world-space ray from screen coordinates.
///
/// `screen_x`, `screen_y`: mouse position in pixels (origin = top-left).
/// `viewport_width`, `viewport_height`: size of the viewport in pixels.
/// `view`: camera view matrix.
/// `proj`: camera projection matrix.
pub fn screen_to_ray(
    screen_x: f32,
    screen_y: f32,
    viewport_width: f32,
    viewport_height: f32,
    view: &glm::Mat4,
    proj: &glm::Mat4,
) -> Ray {
    // Convert screen coords to NDC [-1, 1]
    let ndc_x = (2.0 * screen_x / viewport_width) - 1.0;
    let ndc_y = 1.0 - (2.0 * screen_y / viewport_height); // flip Y

    // Unproject near and far points
    let inv_vp = glm::inverse(&(proj * view));

    // Near plane (z=0 in [0,1] depth range for perspective_zo)
    let near_ndc = glm::vec4(ndc_x, ndc_y, 0.0, 1.0);
    let near_world = inv_vp * near_ndc;
    let near_world = glm::vec3(
        near_world.x / near_world.w,
        near_world.y / near_world.w,
        near_world.z / near_world.w,
    );

    // Far plane (z=1)
    let far_ndc = glm::vec4(ndc_x, ndc_y, 1.0, 1.0);
    let far_world = inv_vp * far_ndc;
    let far_world = glm::vec3(
        far_world.x / far_world.w,
        far_world.y / far_world.w,
        far_world.z / far_world.w,
    );

    let direction = (far_world - near_world).normalize();

    Ray {
        origin: near_world,
        direction,
    }
}

/// Test a ray against an AABB using the slab method.
///
/// Returns `Some(t)` where `t` is the distance along the ray to the nearest
/// intersection point, or `None` if the ray misses.
pub fn ray_aabb_intersect(ray: &Ray, aabb: &AABB) -> Option<f32> {
    if aabb.is_empty() {
        return None;
    }

    let mut t_min = f32::NEG_INFINITY;
    let mut t_max = f32::INFINITY;

    for i in 0..3 {
        let origin = ray.origin[i];
        let dir = ray.direction[i];
        let box_min = aabb.min[i];
        let box_max = aabb.max[i];

        if dir.abs() < 1e-8 {
            // Ray is parallel to this slab
            if origin < box_min || origin > box_max {
                return None;
            }
        } else {
            let inv_dir = 1.0 / dir;
            let mut t1 = (box_min - origin) * inv_dir;
            let mut t2 = (box_max - origin) * inv_dir;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            t_min = t_min.max(t1);
            t_max = t_max.min(t2);
            if t_min > t_max {
                return None;
            }
        }
    }

    // If t_max < 0, the AABB is behind the ray
    if t_max < 0.0 {
        return None;
    }

    // Return the nearest positive t
    Some(if t_min >= 0.0 { t_min } else { t_max })
}

/// Pick the closest node that the ray intersects.
///
/// Recursively tests all nodes with drawables (non-empty AABBs) and returns
/// the closest hit. Nodes without drawables are skipped but their children
/// are still tested.
pub fn pick_node(ray: &Ray, root: &NodeInfo) -> Option<PickResult> {
    let mut best: Option<PickResult> = None;
    pick_node_recursive(ray, root, &mut best);
    best
}

fn pick_node_recursive(ray: &Ray, node: &NodeInfo, best: &mut Option<PickResult>) {
    // Only test nodes that have drawables (and thus a meaningful AABB)
    if node.num_drawables > 0 {
        if let Some(t) = ray_aabb_intersect(ray, &node.world_aabb) {
            let dominated = best.as_ref().map_or(false, |b| t >= b.distance);
            if !dominated {
                *best = Some(PickResult {
                    node_name: node.name.clone(),
                    distance: t,
                });
            }
        }
    }

    // Always recurse into children
    for child in &node.children {
        pick_node_recursive(ray, child, best);
    }
}
