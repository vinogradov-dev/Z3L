//! Commands that author scene objects and their constructed geometry.

use crate::tree::{find_container_mut, find_node_mut, remove_child};
use crate::*;

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

    /// Appends a boolean step to a solid body and bumps the revision once.
    ///
    /// The step carries the shape it cuts with, so the document resolves nothing: the caller — the
    /// command, which had the tree in front of it — froze the cutter's body and its placement into
    /// the step first. `consume` names the cutter's node to remove in the same revision, which is
    /// how a boolean leaves one body: the tool is spent unless it was marked a tool ([`Object::is_tool`]).
    ///
    /// A solid body is a drawn body — a ring swept on Z and its history — or a parametric primitive,
    /// which a step cuts exactly as readily (a block turns a ball into a ball with a rectangular
    /// notch). A node that is neither is [`ProjectError::NotASolidBody`], and a node that is not in
    /// the tree is [`ProjectError::NodeNotFound`]; neither has happened by the time the step is
    /// stored, because the shape is already a value.
    pub fn apply_boolean(&mut self, id: Id, boolean: Boolean, consume: Option<Id>) -> Result<Revision, ProjectError> {
        let next_revision = self.next_revision()?;
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotASolidBody { id });
        };
        match object.geometry_mut() {
            Geometry::Constructive(body) => body.push_boolean(boolean)?,
            Geometry::Primitive(body) => body.push_boolean(boolean)?,
            _ => return Err(ProjectError::NotASolidBody { id }),
        }
        if let Some(tool) = consume {
            // The cutter is spent by the operation, so it leaves the tree in the same revision and
            // the same undo entry: one command, one body. A cutter the maker marked as a tool is
            // kept, because a tool is what a workshop owns and reuses.
            if tool == id {
                return Err(ProjectError::BooleanToolIsTheTarget { id });
            }
            if !remove_child(&mut self.root, tool) {
                return Err(ProjectError::NodeNotFound { id: tool });
            }
            if self.active_camera.is_some_and(|camera| self.camera(camera).is_none()) {
                self.active_camera = None;
            }
        }
        self.revision = next_revision;
        Ok(next_revision)
    }

    /// Removes one boolean step from a solid body and bumps the revision once.
    ///
    /// The index names a position in the body's steps; on a drawn body index zero is the extrusion
    /// the body grew from, which cannot be removed, and an index outside the list is
    /// [`ProjectError::NoSuchBoolean`]. Removing a step does not bring its cutter back: the shape was
    /// frozen into the step and the node was spent when the step was applied (ADR 0197).
    pub fn remove_object_boolean(&mut self, id: Id, index: usize) -> Result<Revision, ProjectError> {
        let next_revision = self.next_revision()?;
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotASolidBody { id });
        };
        match object.geometry_mut() {
            Geometry::Constructive(constructive) => {
                if !matches!(constructive.features().get(index), Some(Feature::Boolean(_))) {
                    return Err(ProjectError::NoSuchBoolean { id, index });
                }
                constructive.remove_feature(index).map_err(|_| ProjectError::NoSuchBoolean { id, index })?;
            }
            Geometry::Primitive(primitive) => {
                primitive.remove_step(index).map_err(|_| ProjectError::NoSuchBoolean { id, index })?;
            }
            _ => return Err(ProjectError::NotASolidBody { id }),
        }
        self.revision = next_revision;
        Ok(next_revision)
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

    /// Turns a contour set into one body per ring, in place, and bumps the revision once.
    ///
    /// A set's rings are separate profiles — a boolean that left two pieces has two of them — and a
    /// body carries one ring, so each ring becomes a body of its own instead of one solid of two
    /// disconnected pieces (X100). The first ring takes the node it grew from: the id, name,
    /// placement, pivot and properties stay, exactly as [`Project::extrude_object_path`] keeps them,
    /// so the selection, the undo entry and a batch's name keep pointing at one node. The other
    /// rings land beside it in the same container and in the drawing's own frame, named after it
    /// (`Union`, `Union 2`), with the pivot a new object starts at — zero (ADR 0088).
    ///
    /// The answer is every body the call left behind, the node's own id first. Every ring is
    /// validated by [`Constructive::extrude`] before the document changes, so a ring the document
    /// cannot hold writes nothing; a node with another payload is
    /// [`ProjectError::NotAContoursObject`].
    pub fn extrude_object_contours(&mut self, id: Id, height: Length) -> Result<Vec<Id>, ProjectError> {
        let next_revision = self.next_revision()?;
        let Some(object) = self.object(id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(contours) = object.geometry().as_contours().cloned() else {
            return Err(ProjectError::NotAContoursObject { id });
        };
        let parent = self.parent_of(id).ok_or(ProjectError::NodeNotFound { id })?;
        let placement = object.placement();
        let base_name = object.name().to_owned();
        let mut rings = contours.paths().iter();
        // A set holds at least one ring, and a ring that cannot become a body is refused before the
        // tree moves: the first body is the node's own payload and the rest are new objects.
        let first =
            Constructive::extrude(rings.next().ok_or(ProjectError::NotAContoursObject { id })?.clone(), height)?;
        let mut rest = Vec::new();
        for ring in rings {
            rest.push(Constructive::extrude(ring.clone(), height)?);
        }
        // The ids are reserved before the tree moves: one per further ring, and the allocation
        // fails here rather than leaving half a set expanded.
        let count = u64::try_from(rest.len()).map_err(|_| ProjectError::IdSpaceExhausted)?;
        let ids = self.allocate_ids(count)?;
        let Some(node) = find_node_mut(&mut self.root, id) else {
            return Err(ProjectError::NodeNotFound { id });
        };
        let Some(object) = node.as_object_mut() else {
            return Err(ProjectError::NotAContoursObject { id });
        };
        *object.geometry_mut() = Geometry::Constructive(first);
        let mut bodies = vec![id];
        for (child_id, body) in ids.into_iter().zip(rest) {
            // The name is read against the children the container holds now, so two rings of one
            // set never take one name.
            let name = self.unique_child_name(parent, &base_name);
            let Some((_depth, children)) = find_container_mut(&mut self.root, parent, 1) else {
                return Err(ProjectError::ContainerNotFound { id: parent });
            };
            children.push(Node::Object(Object::new(child_id, name, placement, Geometry::Constructive(body))));
            bodies.push(child_id);
        }
        self.revision = next_revision;
        Ok(bodies)
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

    /// Replaces a primitive's shape and bumps the revision once, keeping the steps cut from it.
    ///
    /// The id, name, placement, pivot and properties are unchanged. A node that is not a parametric
    /// primitive is [`ProjectError::NotAPrimitiveObject`]; invalid parameters are refused by the
    /// payload constructors before the document changes.
    pub fn set_object_primitive(&mut self, id: Id, shape: PrimitiveShape) -> Result<Revision, ProjectError> {
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
        stored.set_base(shape);
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
