//! A binned-SAH BVH2 over the frame's triangles, and the traversal that reads it.
//!
//! The build is Wald's, *On Fast Construction of SAH-based Bounding Volume Hierarchies* (2007): each
//! node bins its triangles by centroid into sixteen slabs on the axis with the widest centroid
//! spread, costs every split by surface area and keeps the cheapest, which is one pass over the range
//! and no sorting. A node that holds few enough triangles, or whose split costs more than testing
//! them, becomes a leaf. The tree is a binary layout depth-first, so a node's right child is its left
//! child plus one and a leaf is a contiguous run of the reordered triangle array.
//!
//! Why a two-wide tree and not a compressed one: a whole kitchen is tens of thousands of triangles,
//! and a wide BVH pays in the millions — `plan/notes/Y1.md` carries the measurement behind that
//! decision.
//!
//! Determinism is part of the contract: the same triangles in the same order give the same nodes, so
//! a cached tree can be compared rather than trusted, and a test that a revision keyed the tree
//! correctly has something exact to compare.
//!
//! Refuses: a triangle with a non-finite vertex is dropped by the build (no ray can hit it, and one
//! NaN box would poison every ancestor's); a range that would need a deeper tree than [`MAX_DEPTH`]
//! becomes a leaf, so traversal's fixed stack cannot overflow by construction.
//!
//! Changes when: the split heuristic, the node layout or the traversal order changes.

use geom::{Aabb, DVec3};

use crate::ray::{Hit, Ray, Triangle};

/// How many bins each axis is divided into while a split is chosen.
const BINS: usize = 16;

/// How many triangles a node may hold before it is split.
const MAX_LEAF: usize = 4;

/// How deep the tree may go, which is what bounds traversal's stack.
pub(crate) const MAX_DEPTH: u32 = 48;

/// The relative cost of stepping into a child, in the units the surface-area cost is measured in.
const TRAVERSAL_COST: f64 = 1.0;

/// The relative cost of one triangle test.
const INTERSECTION_COST: f64 = 1.0;

/// The axis byte of a leaf, which splits nothing.
const AXIS_NONE: u8 = 3;

/// The slots traversal may occupy: one per level plus the root, with room to spare.
const MAX_STACK: usize = MAX_DEPTH as usize + 16;

/// One node: a box, and either a run of triangles or the index of its left child.
#[derive(Clone, Copy, Debug)]
struct Node {
    /// The box around everything below this node.
    bounds: Aabb,
    /// A leaf's first triangle index, or an interior node's left child.
    first: usize,
    /// A leaf's triangle count, or zero for an interior node.
    count: usize,
    /// The axis an interior node was split on; [`AXIS_NONE`] for a leaf.
    axis: u8,
}

impl Node {
    /// Whether this node holds triangles rather than children.
    fn is_leaf(&self) -> bool {
        self.count > 0
    }
}

/// The split a node chose: an axis and the last bin that stays on the left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Split {
    /// The axis the centroids were binned on.
    axis: u8,
    /// The bin the left side ends at, inclusive.
    bin: usize,
}

/// A bounding volume hierarchy over a triangle list.
#[derive(Clone, Debug, Default)]
pub struct Bvh {
    /// The triangles, reordered so every leaf owns a contiguous run.
    triangles: Vec<Triangle>,
    /// The nodes, the root first, children of a node adjacent.
    nodes: Vec<Node>,
}

impl Bvh {
    /// Builds the tree over `triangles`, dropping any whose vertices are not finite.
    pub fn build(triangles: Vec<Triangle>) -> Self {
        // A triangle's box and centroid are read by the split of its node, by the bounds of its
        // children and by the traversal, and computing them once here is the difference between a
        // kitchen's tree in tens of milliseconds and in hundreds: one triangle's box used to be
        // rebuilt three times per tree level.
        let mut items: Vec<Item> = Vec::with_capacity(triangles.len());
        for triangle in triangles {
            let Some(bounds) = triangle.bounds() else { continue };
            items.push(Item { triangle, bounds, centroid: triangle.centroid() });
        }
        let count = items.len();
        let Some(root_bounds) = bounds_of(&items, 0, count) else {
            return Self::default();
        };
        let mut nodes = vec![Node { bounds: root_bounds, first: 0, count, axis: AXIS_NONE }];
        // An explicit work list rather than recursion: the depth is bounded anyway, and a session
        // thread's stack is not this crate's to spend.
        let mut work: Vec<(usize, usize, usize, u32)> = Vec::with_capacity(64);
        work.push((0, 0, count, 0));
        while let Some((node_index, first, count, depth)) = work.pop() {
            let Some(node) = nodes.get(node_index) else { continue };
            let bounds = node.bounds;
            if count <= MAX_LEAF || depth >= MAX_DEPTH {
                continue;
            }
            let spread = centroids_range(&items, first, count);
            let Some(split) = best_split(&items, first, count, bounds) else { continue };
            let mid = partition(&mut items, first, count, spread, split);
            if mid == first || mid == first + count {
                continue;
            }
            let (Some(left_bounds), Some(right_bounds)) =
                (bounds_of(&items, first, mid), bounds_of(&items, mid, first + count))
            else {
                continue;
            };
            let left = nodes.len();
            nodes.push(Node { bounds: left_bounds, first, count: mid - first, axis: AXIS_NONE });
            nodes.push(Node { bounds: right_bounds, first: mid, count: first + count - mid, axis: AXIS_NONE });
            if let Some(father) = nodes.get_mut(node_index) {
                father.first = left;
                father.count = 0;
                father.axis = split.axis;
            }
            work.push((left + 1, mid, first + count - mid, depth + 1));
            work.push((left, first, mid - first, depth + 1));
        }
        Self { triangles: items.into_iter().map(|item| item.triangle).collect(), nodes }
    }

    /// The triangles the tree holds, in tree order.
    pub fn triangles(&self) -> &[Triangle] {
        &self.triangles
    }

    /// How many triangles the tree holds.
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// How many nodes the tree holds; zero for an empty tree.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The depth of the deepest leaf, or zero for an empty tree.
    pub fn depth(&self) -> u32 {
        let mut deepest = 0;
        let mut work: Vec<(usize, u32)> = Vec::new();
        if !self.nodes.is_empty() {
            work.push((0, 0));
        }
        while let Some((node_index, depth)) = work.pop() {
            let Some(node) = self.nodes.get(node_index) else { continue };
            if node.is_leaf() {
                deepest = deepest.max(depth);
                continue;
            }
            work.push((node.first, depth + 1));
            work.push((node.first + 1, depth + 1));
        }
        deepest
    }

    /// The nearest triangle the ray meets inside `[t_min, t_max]`, and its index in tree order.
    ///
    /// Children are entered nearest first and a hit shortens the far end of every later box test, so
    /// the work falls away as soon as something is in front: the order is what makes a primary ray
    /// cheap, and it changes no answer.
    pub fn intersect(&self, ray: Ray, t_min: f64, t_max: f64) -> Option<(usize, Hit)> {
        let root = self.nodes.first()?;
        ray.bounds_entry(root.bounds, t_min, t_max)?;
        let mut best: Option<(usize, Hit)> = None;
        let mut far = t_max;
        let mut stack: [(usize, f64); MAX_STACK] = [(0, 0.0); MAX_STACK];
        let mut len = 0usize;
        stack[0] = (0, t_min);
        len += 1;
        while len > 0 {
            len -= 1;
            let Some(&(node_index, entry)) = stack.get(len) else { continue };
            if entry > far {
                continue;
            }
            let Some(node) = self.nodes.get(node_index) else { continue };
            if node.is_leaf() {
                for index in node.first..node.first + node.count {
                    let Some(triangle) = self.triangles.get(index) else { continue };
                    if let Some(hit) = triangle.intersect(ray, t_min, far) {
                        far = hit.t;
                        best = Some((index, hit));
                    }
                }
                continue;
            }
            // The two children, nearest first; the farther one is pushed first so the nearer pops.
            let mut children: [(usize, f64); 2] = [(node.first, 0.0), (node.first + 1, 0.0)];
            let mut entered = 0usize;
            for child in [node.first, node.first + 1] {
                let Some(child_node) = self.nodes.get(child) else { continue };
                let Some(entry) = ray.bounds_entry(child_node.bounds, t_min, far) else { continue };
                if let Some(slot) = children.get_mut(entered) {
                    *slot = (child, entry);
                    entered += 1;
                }
            }
            if entered == 2 && children[0].1 > children[1].1 {
                children.swap(0, 1);
            }
            for &(child, entry) in children.iter().take(entered) {
                if let Some(slot) = stack.get_mut(len) {
                    *slot = (child, entry);
                    len += 1;
                }
            }
        }
        best
    }
}

/// One triangle with the two numbers the build reads again and again.
#[derive(Clone, Copy, Debug)]
struct Item {
    /// The triangle itself.
    triangle: Triangle,
    /// The box around it, measured once.
    bounds: Aabb,
    /// The average of its vertices, which the split bins by.
    centroid: DVec3,
}

/// The box around a range of triangles, or `None` when the range holds none.
fn bounds_of(items: &[Item], first: usize, end: usize) -> Option<Aabb> {
    let mut bounds: Option<Aabb> = None;
    for item in items.get(first..end).unwrap_or_default() {
        bounds = Some(bounds.map_or(item.bounds, |current: Aabb| current.union(item.bounds)));
    }
    bounds
}

/// The full surface area of a box, which is the cost every split is measured in.
fn surface_area(bounds: Aabb) -> f64 {
    let extent = bounds.max() - bounds.min();
    2.0 * (extent.x * extent.y + extent.y * extent.z + extent.z * extent.x)
}

/// The box around the centroids of a range, which is the space a split divides.
fn centroids_range(items: &[Item], first: usize, count: usize) -> Option<Aabb> {
    Aabb::of_points(items.iter().skip(first).take(count).map(|item| item.centroid))
}

/// The cheapest split of a node, or `None` when no split beats testing the triangles where they are.
fn best_split(items: &[Item], first: usize, count: usize, bounds: Aabb) -> Option<Split> {
    let parent_area = surface_area(bounds);
    if parent_area <= 0.0 || parent_area.is_nan() {
        return None;
    }
    let spread = centroids_range(items, first, count)?;
    let extent = spread.max() - spread.min();
    let leaf_cost = INTERSECTION_COST * count as f64;
    let mut best: Option<(f64, Split)> = None;
    for axis in 0..3u8 {
        let axis_index = usize::from(axis);
        if extent[axis_index] <= 0.0 || extent[axis_index].is_nan() {
            continue;
        }
        let scale = BINS as f64 / extent[axis_index];
        let origin = spread.min()[axis_index];
        let mut bin_bounds: [Option<Aabb>; BINS] = [None; BINS];
        let mut bin_count: [usize; BINS] = [0; BINS];
        for item in items.iter().skip(first).take(count) {
            let bin = bin_of(item.centroid[axis_index], origin, scale);
            if let Some(slot) = bin_bounds.get_mut(bin) {
                *slot = Some(slot.map_or(item.bounds, |current: Aabb| current.union(item.bounds)));
                if let Some(count_slot) = bin_count.get_mut(bin) {
                    *count_slot = count_slot.saturating_add(1);
                }
            }
        }
        // The areas and counts either side of every bin boundary, built once on each side of the axis.
        let mut left_area = [0.0f64; BINS];
        let mut left_count = [0usize; BINS];
        let mut grown: Option<Aabb> = None;
        let mut running = 0usize;
        for bin in 0..BINS {
            if let Some(bin_box) = bin_bounds[bin] {
                grown = Some(grown.map_or(bin_box, |current: Aabb| current.union(bin_box)));
            }
            running = running.saturating_add(bin_count[bin]);
            left_area[bin] = grown.map_or(0.0, surface_area);
            left_count[bin] = running;
        }
        let mut right_area = [0.0f64; BINS];
        let mut right_count = [0usize; BINS];
        grown = None;
        running = 0;
        for bin in (0..BINS).rev() {
            if let Some(bin_box) = bin_bounds[bin] {
                grown = Some(grown.map_or(bin_box, |current: Aabb| current.union(bin_box)));
            }
            running = running.saturating_add(bin_count[bin]);
            right_area[bin] = grown.map_or(0.0, surface_area);
            right_count[bin] = running;
        }
        for bin in 0..BINS.saturating_sub(1) {
            let left = left_count[bin];
            let right = right_count[bin + 1];
            if left == 0 || right == 0 {
                continue;
            }
            let cost = TRAVERSAL_COST
                + INTERSECTION_COST * (left_area[bin] * left as f64 + right_area[bin + 1] * right as f64) / parent_area;
            if cost >= leaf_cost {
                continue;
            }
            let better = best.is_none_or(|(best_cost, _)| cost < best_cost);
            if better {
                best = Some((cost, Split { axis, bin }));
            }
        }
    }
    best.map(|(_, split)| split)
}

/// The bin a centroid falls into, clamped to the last one so the widest centroid is not out of range.
fn bin_of(value: f64, origin: f64, scale: f64) -> usize {
    let scaled = (value - origin) * scale;
    if scaled <= 0.0 || scaled.is_nan() {
        return 0;
    }
    (scaled as usize).min(BINS - 1)
}

/// Reorders the range so the triangles before the returned index are on the left of the split.
fn partition(items: &mut [Item], first: usize, count: usize, spread: Option<Aabb>, split: Split) -> usize {
    let axis_index = usize::from(split.axis);
    let Some(spread) = spread else { return first };
    let extent = spread.max()[axis_index] - spread.min()[axis_index];
    if extent <= 0.0 || extent.is_nan() {
        return first;
    }
    let origin = spread.min()[axis_index];
    let scale = BINS as f64 / extent;
    let mut left = first;
    let mut right = first + count;
    while left < right {
        let Some(item) = items.get(left) else { break };
        if bin_of(item.centroid[axis_index], origin, scale) <= split.bin {
            left += 1;
        } else {
            right -= 1;
            items.swap(left, right);
        }
    }
    left
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::time::{Duration, Instant};

    use super::*;

    /// A deterministic pseudo-random generator, so a failure is the same failure twice.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }

        fn range(&mut self, low: f64, high: f64) -> f64 {
            low + (high - low) * self.next()
        }

        fn point(&mut self, extent: f64) -> DVec3 {
            DVec3::new(self.range(-extent, extent), self.range(-extent, extent), self.range(-extent, extent))
        }
    }

    /// A kitchen's worth of quads: `quads` two-triangle faces spread over a 3000 × 2200 × 600 box.
    fn kitchen(quads: usize) -> Vec<Triangle> {
        let mut triangles = Vec::with_capacity(quads * 2);
        for index in 0..quads {
            let x = (index % 100) as f64 * 30.0;
            let y = ((index / 100) % 50) as f64 * 44.0;
            let z = (index / 5000) as f64 * 18.0;
            let a = DVec3::new(x, y, z);
            let b = DVec3::new(x + 28.0, y, z);
            let c = DVec3::new(x + 28.0, y + 42.0, z);
            let d = DVec3::new(x, y + 42.0, z + 6.0);
            triangles.push(Triangle::new(a, b, c));
            triangles.push(Triangle::new(a, c, d));
        }
        triangles
    }

    fn random_scene(triangles: usize, extent: f64, seed: u64) -> Vec<Triangle> {
        let mut rng = Lcg(seed);
        (0..triangles).map(|_| Triangle::new(rng.point(extent), rng.point(extent), rng.point(extent))).collect()
    }

    #[test]
    fn the_tree_answers_the_nearest_hit_a_brute_force_scan_finds() {
        let scene = random_scene(400, 1000.0, 7);
        let tree = Bvh::build(scene.clone());
        assert!(tree.node_count() > 1, "400 random triangles split: {}", tree.node_count());
        let mut rng = Lcg(11);
        for _ in 0..500 {
            let ray = Ray::new(rng.point(1500.0), rng.point(1.0)).expect("a ray with a direction");
            let expected = scene
                .iter()
                .filter_map(|triangle| triangle.intersect(ray, 0.0, f64::INFINITY))
                .map(|hit| hit.t)
                .fold(f64::INFINITY, f64::min);
            let answered = tree.intersect(ray, 0.0, f64::INFINITY).map(|(_, hit)| hit.t).unwrap_or(f64::INFINITY);
            assert!(
                (answered == expected) || (answered - expected).abs() <= 1e-9,
                "the tree answered {answered} and the scan {expected} for {ray:?}"
            );
        }
    }

    #[test]
    fn the_tree_is_deterministic_and_empty_input_is_empty() {
        let scene = random_scene(200, 500.0, 3);
        let first = Bvh::build(scene.clone());
        let second = Bvh::build(scene);
        assert_eq!(first.nodes.len(), second.nodes.len());
        assert_eq!(first.triangles, second.triangles, "the same input gives the same reorder");
        for (a, b) in first.nodes.iter().zip(&second.nodes) {
            assert_eq!(a.first, b.first);
            assert_eq!(a.count, b.count);
            assert_eq!(a.axis, b.axis);
        }
        let empty = Bvh::build(Vec::new());
        assert_eq!(empty.node_count(), 0);
        assert_eq!(empty.triangle_count(), 0);
        assert_eq!(empty.depth(), 0);
        assert!(empty.intersect(Ray::new(DVec3::ZERO, DVec3::Z).expect("a ray"), 0.0, 1.0).is_none());
    }

    #[test]
    fn a_triangle_with_a_non_finite_vertex_is_dropped_rather_than_poisoning_a_box() {
        let mut scene = random_scene(10, 100.0, 5);
        scene.push(Triangle::new(DVec3::NAN, DVec3::ZERO, DVec3::X));
        scene.push(Triangle::new(DVec3::ZERO, DVec3::new(f64::INFINITY, 0.0, 0.0), DVec3::X));
        let tree = Bvh::build(scene);
        assert_eq!(tree.triangle_count(), 10, "the two hostile triangles are not in the tree");
        for node in &tree.nodes {
            assert!(node.bounds.min().is_finite() && node.bounds.max().is_finite(), "{node:?}");
        }
    }

    #[test]
    fn a_kitchen_sized_frame_builds_fast_and_shallow() {
        let triangles = kitchen(25_000);
        assert_eq!(triangles.len(), 50_000);
        let start = Instant::now();
        let tree = Bvh::build(triangles);
        let elapsed = start.elapsed();
        println!(
            "BVH build: {} triangles, {} nodes, depth {}, {elapsed:?}",
            tree.triangle_count(),
            tree.node_count(),
            tree.depth()
        );
        assert!(elapsed < Duration::from_millis(500), "a kitchen's tree took {elapsed:?}");
        assert!(tree.depth() < MAX_DEPTH, "a kitchen's tree is not at the depth limit: {}", tree.depth());
        // A binary layout cannot exceed two nodes per leaf, and our leaf size keeps it far below.
        assert!(tree.node_count() < 2 * tree.triangle_count(), "{} nodes", tree.node_count());
        assert!(
            tree.node_count() * 8 < tree.triangle_count() * 12,
            "the tree holds more nodes than a coarse split needs: {}",
            tree.node_count()
        );
    }

    #[test]
    fn a_hit_shortens_the_search_and_a_far_hit_is_not_reported() {
        let scene = kitchen(4);
        let tree = Bvh::build(scene);
        let near = Ray::new(DVec3::new(10.0, 10.0, 100.0), -DVec3::Z).expect("a ray");
        let hit = tree.intersect(near, 0.0, f64::INFINITY).expect("the ray meets the first quad");
        assert!((hit.1.t - 100.0).abs() < 1e-9, "{hit:?}");
        assert!(tree.intersect(near, 0.0, 50.0).is_none(), "a far end short of the face reports nothing");
    }
}
