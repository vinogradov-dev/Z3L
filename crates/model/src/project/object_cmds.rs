//! Commands that author scene objects and their constructed geometry.

use std::collections::HashSet;

use crate::tree::{find_container_mut, find_node_mut};
use crate::*;

/// One tool body a constructive object's boolean steps name, resolved for evaluation.
///
/// The tool is never consumed: the payload is a copy for the evaluator and the document keeps the
/// node, so several bodies may name the same tool and every one of them follows it. The placement
/// is expressed in the body's own frame, so a compiler never composes world transforms itself.
#[derive(Clone, Debug, PartialEq)]
pub struct BooleanTool {
    id: Id,
    body: Constructive,
    placement: Placement,
}

impl BooleanTool {
    /// The node the step names.
    pub const fn id(&self) -> Id {
        self.id
    }

    /// The tool's own feature history, which may name tools of its own.
    pub const fn body(&self) -> &Constructive {
        &self.body
    }

    /// The tool's placement in the frame of the body that names it.
    pub const fn placement(&self) -> Placement {
        self.placement
    }
}

impl Project {
    /// The contour of a constructive object that a new panel can inherit.
    ///
    /// A missing node, a mesh object or a node that is not a constructive object is an error the
    /// user can read; nothing is derived when the payload has no extrusion.
    pub fn object_contour(&self, id: Id) -> Result<Contour, ProjectError> {
        let object = self.object(id).ok_or(ProjectError::NodeNotFound { id })?;
        let constructive = object.geometry().as_constructive().ok_or(ProjectError::NotConstructiveObject { id })?;
        constructive.contour().map_err(ConstructiveError::from).map_err(ProjectError::from)
    }

    /// Checks whether an object's contour can become a panel outline without changing the document.
    pub fn validate_panel_contour(&self, parent: Id, source: Id) -> Result<(), ProjectError> {
        if !self.is_container(parent) {
            return Err(ProjectError::ContainerNotFound { id: parent });
        }
        let anchored = Panel::anchored_contour(&self.object_contour(source)?)?;
        Panel::contour_size(&anchored)?;
        Ok(())
    }

    /// Adds a geometry object to an existing container and bumps the revision once.
    ///
    /// The object id is allocated from the document; callers must not invent one. The parent may
    /// be the root or any nested container. The object's local zero is the point its content was
    /// anchored at, and the pivot starts at zero: a creation command rebases its content first
    /// ([`crate::rebase_origin_at`]) and stores the resulting placement here. Every later geometry
    /// edit leaves the stored pivot alone (ADR 0026, ADR 0088).
    pub fn add_object(
        &mut self,
        parent: Id,
        name: impl Into<String>,
        placement: Placement,
        geometry: Geometry,
    ) -> Result<Id, ProjectError> {
        if !self.is_container(parent) {
            return Err(ProjectError::ContainerNotFound { id: parent });
        }
        let next_revision = self.next_revision()?;
        let id = self.allocate_id()?;
        let object = Object::new(id, name, placement, geometry);
        let Some((_depth, children)) = find_container_mut(&mut self.root, parent, 1) else {
            return Err(ProjectError::ContainerNotFound { id: parent });
        };
        children.push(Node::Object(object));
        self.revision = next_revision;
        Ok(id)
    }

    /// The path payload of an object that carries a flat drawing.
    ///
    /// A missing node or an object with a mesh or constructive payload is an error the user can
    /// read; no drawing is derived from those payloads.
    pub fn object_path(&self, id: Id) -> Result<&Path, ProjectError> {
        let object = self.object(id).ok_or(ProjectError::NodeNotFound { id })?;
        object.geometry().as_path().ok_or(ProjectError::NotAPathObject { id })
    }

    /// Replaces a path object's drawing and bumps the revision once.
    ///
    /// A node that is not a path object is [`ProjectError::NotAPathObject`]; the id, name,
    /// placement, pivot and properties are unchanged.
    pub fn set_object_path(&mut self, id: Id, path: Path) -> Result<Revision, ProjectError> {
        let next_revision = self.next_revision()?;
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotAPathObject { id });
        };
        let Some(stored) = object.geometry_mut().as_path_mut() else {
            return Err(ProjectError::NotAPathObject { id });
        };
        *stored = path;
        self.revision = next_revision;
        Ok(next_revision)
    }

    /// Replaces the path of a constructive object and bumps the revision once.
    ///
    /// A node that is not a constructive object is [`ProjectError::NotConstructiveObject`]; an
    /// invalid path is refused before the document changes. The body owns this path, so a later
    /// extrusion keeps it and no second ring is stored.
    pub fn set_object_body_path(&mut self, id: Id, path: Path) -> Result<Revision, ProjectError> {
        let next_revision = self.next_revision()?;
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotConstructiveObject { id });
        };
        let Some(constructive) = object.geometry_mut().as_constructive_mut() else {
            return Err(ProjectError::NotConstructiveObject { id });
        };
        constructive.set_path(path)?;
        self.revision = next_revision;
        Ok(next_revision)
    }

    /// Replaces the extrusion height of a constructive object and bumps the revision once.
    ///
    /// The path and every other feature are unchanged; a non-constructive node or an invalid
    /// height is refused before the document changes.
    pub fn set_object_extrusion_height(&mut self, id: Id, height: Length) -> Result<Revision, ProjectError> {
        let next_revision = self.next_revision()?;
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotConstructiveObject { id });
        };
        let Some(constructive) = object.geometry_mut().as_constructive_mut() else {
            return Err(ProjectError::NotConstructiveObject { id });
        };
        constructive.set_height(height)?;
        self.revision = next_revision;
        Ok(next_revision)
    }

    /// Appends a boolean step to a constructive object and bumps the revision once.
    ///
    /// The tool is a reference to another constructive body, which stays in the tree and may serve
    /// any number of bodies. Two references are refused before the document changes: one naming the
    /// body itself and one whose tool already reaches this body through its own steps, because
    /// either would close a loop the plan compiler cannot walk.
    pub fn set_object_boolean(&mut self, id: Id, op: BooleanOp, tool: Id) -> Result<Revision, ProjectError> {
        let next_revision = self.next_revision()?;
        if tool == id {
            return Err(ProjectError::BooleanToolIsSelf { id });
        }
        // The tool's own steps are walked first: the probe refuses a tool that is not a body, and a
        // body this one already names is the edge that would close the loop.
        let names_this_body = self
            .boolean_tools(tool)
            .map_err(|_| ProjectError::BooleanToolNotABody { object: id, tool })?
            .iter()
            .any(|named| named.id() == id);
        if names_this_body {
            return Err(ProjectError::BooleanToolCycle { id });
        }
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotConstructiveObject { id });
        };
        let Some(constructive) = object.geometry_mut().as_constructive_mut() else {
            return Err(ProjectError::NotConstructiveObject { id });
        };
        constructive.push_boolean(Boolean::new(op, tool))?;
        self.revision = next_revision;
        Ok(next_revision)
    }

    /// Removes one boolean step from a constructive object and bumps the revision once.
    ///
    /// The index names a position in the feature history; index zero is the body's own extrusion,
    /// which cannot be removed, and an index outside the history is
    /// [`ProjectError::NoSuchBoolean`].
    pub fn remove_object_boolean(&mut self, id: Id, index: usize) -> Result<Revision, ProjectError> {
        let next_revision = self.next_revision()?;
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotConstructiveObject { id });
        };
        let Some(constructive) = object.geometry_mut().as_constructive_mut() else {
            return Err(ProjectError::NotConstructiveObject { id });
        };
        if !matches!(constructive.features().get(index), Some(Feature::Boolean(_))) {
            return Err(ProjectError::NoSuchBoolean { id, index });
        }
        constructive.remove_feature(index).map_err(|_| ProjectError::NoSuchBoolean { id, index })?;
        self.revision = next_revision;
        Ok(next_revision)
    }

    /// The tool bodies one constructive object names, transitively, in the frame of that object.
    ///
    /// A tool may name tools of its own, so the answer is the whole closure: a compiler builds a
    /// plan from it and the mesh cache keys on it, and both need every body a target depends on,
    /// not only the ones it names directly. A missing tool, a node that is not a constructive body,
    /// a self-reference and a reference that closes a loop are readable errors; the document
    /// refuses all four when it loads, so this is the same answer for a file and for a live edit.
    pub fn boolean_tools(&self, id: Id) -> Result<Vec<BooleanTool>, ProjectError> {
        let object = self.object(id).ok_or(ProjectError::NodeNotFound { id })?;
        let body = object.geometry().as_constructive().ok_or(ProjectError::NotConstructiveObject { id })?;
        self.boolean_tools_of(id, body)
    }

    /// The tool bodies one feature history names, resolved in the frame of the node `id`.
    ///
    /// The history is handed in rather than read off the node, because a previewing operation
    /// evaluates a body the document does not hold yet: the step the ✓ would store is compiled from
    /// the same answer, so the preview cannot draw a body the command then refuses.
    pub fn boolean_tools_of(&self, id: Id, body: &Constructive) -> Result<Vec<BooleanTool>, ProjectError> {
        if self.object(id).is_none() {
            return Err(ProjectError::NodeNotFound { id });
        }
        let mut tools = Vec::new();
        let mut seen = HashSet::new();
        let mut pending: Vec<Id> = body.booleans().map(Boolean::tool).collect();
        while let Some(tool) = pending.pop() {
            if tool == id {
                return Err(ProjectError::BooleanToolIsSelf { id });
            }
            if !seen.insert(tool.get()) {
                continue;
            }
            let node = self.object(tool).ok_or(ProjectError::BooleanToolNotABody { object: id, tool })?;
            let tool_body =
                node.geometry().as_constructive().ok_or(ProjectError::BooleanToolNotABody { object: id, tool })?;
            let world = self.world_placement(tool).ok_or(ProjectError::BooleanToolNotABody { object: id, tool })?;
            let placement = self.placement_in_parent(id, world)?;
            tools.push(BooleanTool { id: tool, body: tool_body.clone(), placement });
            pending.extend(tool_body.booleans().map(Boolean::tool));
        }
        // The closure is compared as a value, so its order must follow the document rather than the
        // order the walk happened to pop its ids.
        tools.sort_by_key(|tool| tool.id.get());
        Ok(tools)
    }

    /// Turns a path object into the constructive body that owns that path, in place.
    ///
    /// The node keeps its id, name, placement, pivot and properties, so the selection, the undo
    /// entry and the scene tree keep pointing at one node: the drawing becomes the body instead of
    /// leaving a copy behind. An open chain encloses no ring and is refused before the document
    /// changes; a node with another payload is [`ProjectError::NotAPathObject`].
    pub fn extrude_object_path(&mut self, id: Id, height: Length) -> Result<Revision, ProjectError> {
        let next_revision = self.next_revision()?;
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotAPathObject { id });
        };
        let Some(path) = object.geometry().as_path().cloned() else {
            return Err(ProjectError::NotAPathObject { id });
        };
        let constructive = Constructive::extrude(path, height)?;
        *object.geometry_mut() = Geometry::Constructive(constructive);
        self.revision = next_revision;
        Ok(next_revision)
    }

    /// The contours of a contour-set object.
    ///
    /// A node with another payload is [`ProjectError::NotAContoursObject`].
    pub fn object_contours(&self, id: Id) -> Result<&Contours, ProjectError> {
        let object = self.object(id).ok_or(ProjectError::NodeNotFound { id })?;
        object.geometry().as_contours().ok_or(ProjectError::NotAContoursObject { id })
    }

    /// Marks or unmarks an object as a tool, and bumps the revision once.
    ///
    /// The mark decides whether a contour boolean consumes the object or leaves it in the tree; it
    /// changes nothing else about the object. A node that is not a geometry object is refused.
    pub fn set_object_tool(&mut self, id: Id, tool: bool) -> Result<Revision, ProjectError> {
        let next_revision = self.next_revision()?;
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotAnObject { id });
        };
        object.set_tool(tool);
        self.revision = next_revision;
        Ok(next_revision)
    }

    /// The parameters of a parametric primitive object.
    ///
    /// A missing node or an object with another payload is an error the user can read; no shape is
    /// invented from a mesh or a contour.
    pub fn object_primitive(&self, id: Id) -> Result<&Primitive, ProjectError> {
        let object = self.object(id).ok_or(ProjectError::NodeNotFound { id })?;
        object.geometry().as_primitive().ok_or(ProjectError::NotAPrimitiveObject { id })
    }

    /// Replaces a primitive's parameters and bumps the revision once.
    ///
    /// The id, name, placement, pivot and properties are unchanged. A node that is not a parametric
    /// primitive is [`ProjectError::NotAPrimitiveObject`]; invalid parameters are refused by the
    /// payload constructors before the document changes.
    pub fn set_object_primitive(&mut self, id: Id, primitive: Primitive) -> Result<Revision, ProjectError> {
        let next_revision = self.next_revision()?;
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotAPrimitiveObject { id });
        };
        let Some(stored) = object.geometry_mut().as_primitive_mut() else {
            return Err(ProjectError::NotAPrimitiveObject { id });
        };
        *stored = primitive;
        self.revision = next_revision;
        Ok(next_revision)
    }

    /// Replaces a derived payload with a baked mesh and bumps the revision once.
    ///
    /// The caller evaluates the payload and passes the resulting mesh; the id, name, placement,
    /// pivot and properties are unchanged and the parameters are dropped ([ADR 0037], [ADR 0054]).
    /// A node that is neither constructive nor a primitive is refused.
    ///
    /// [ADR 0037]: ../docs/decisions/0037-a-geometry-object-is-a-mesh-or-a-feature-history.md
    /// [ADR 0054]: ../docs/decisions/0054-a-primitive-keeps-the-parameters-that-built-it.md
    pub fn collapse_object(&mut self, id: Id, mesh: Mesh) -> Result<Revision, ProjectError> {
        let next_revision = self.next_revision()?;
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotDerivedObject { id });
        };
        if !matches!(object.geometry(), Geometry::Constructive(_) | Geometry::Primitive(_)) {
            return Err(ProjectError::NotDerivedObject { id });
        }
        *object.geometry_mut() = Geometry::Mesh(mesh);
        self.revision = next_revision;
        Ok(next_revision)
    }

    /// Moves mesh vertices to absolute local positions and bumps the revision once.
    ///
    /// The batch is validated before anything changes: an empty batch, a repeated index or an
    /// index outside the mesh rejects the whole call, so a partial move can never reach the
    /// document. A constructive or primitive object has no editable mesh and is refused; it
    /// becomes a mesh object only through `object.collapse`.
    pub fn move_mesh_vertices(&mut self, id: Id, moves: &[(u32, [Length; 3])]) -> Result<Revision, ProjectError> {
        if moves.is_empty() {
            return Err(ProjectError::NoVertexMoves);
        }
        let next_revision = self.next_revision()?;
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotAMeshObject { id });
        };
        let Some(mesh) = object.geometry_mut().as_mesh_mut() else {
            return Err(ProjectError::NotAMeshObject { id });
        };
        let count = mesh.positions().len();
        let mut seen = HashSet::with_capacity(moves.len());
        let mut resolved = Vec::with_capacity(moves.len());
        for (vertex, position) in moves {
            if !seen.insert(*vertex) {
                return Err(ProjectError::DuplicateVertex { object: id, vertex: *vertex });
            }
            let Ok(index) = usize::try_from(*vertex) else {
                return Err(MeshError::VertexIndexOutOfBounds { vertex: usize::MAX, count }.into());
            };
            if index >= count {
                return Err(MeshError::VertexIndexOutOfBounds { vertex: index, count }.into());
            }
            resolved.push((index, *position));
        }
        for (index, position) in resolved {
            mesh.set_position(index, position)?;
        }
        self.revision = next_revision;
        Ok(next_revision)
    }
}
