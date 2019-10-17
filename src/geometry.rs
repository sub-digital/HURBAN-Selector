use std::cmp;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use arrayvec::ArrayVec;

use nalgebra as na;
use nalgebra::base::Vector3;
use nalgebra::geometry::Point3;

use crate::convert::{cast_i32, cast_u32, cast_usize};

#[derive(Debug, Clone, Copy)]
pub enum NormalStrategy {
    Sharp,
    // FIXME: add `Smooth`
}

pub type Vertices = Vec<Point3<f32>>;
pub type Normals = Vec<Vector3<f32>>;

/// Geometric data containing multiple possibly _variable-length_
/// lists of geometric data, such as vertices and normals, and faces -
/// a single list containing the index topology that describes the
/// structure of data in those lists.
///
/// Currently only `Face::Triangle` is supported. It binds vertices
/// and normals in triangular faces. `Face::Triangle` is always
/// ensured to have counter-clockwise winding. Quad or polygonal faces
/// are not supported currently, but might be in the future.
///
/// The geometry data lives in right-handed coordinate space with the
/// XY plane being the ground and Z axis growing upwards.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    faces: Vec<Face>,
    vertices: Vertices,
    normals: Normals,
}

impl Geometry {
    /// Create new triangle face geometry from provided faces and
    /// vertices, and compute normals based on `normal_strategy`.
    ///
    /// # Panics
    /// Panics if faces refer to out-of-bounds vertices.
    pub fn from_triangle_faces_with_vertices_and_computed_normals(
        faces: Vec<(u32, u32, u32)>,
        vertices: Vertices,
        normal_strategy: NormalStrategy,
    ) -> Self {
        let mut normals = Vec::with_capacity(faces.len());
        let vertices_range = 0..cast_u32(vertices.len());
        for &(v1, v2, v3) in &faces {
            assert!(
                vertices_range.contains(&v1),
                "Faces reference out of bounds position data"
            );
            assert!(
                vertices_range.contains(&v2),
                "Faces reference out of bounds position data"
            );
            assert!(
                vertices_range.contains(&v3),
                "Faces reference out of bounds position data"
            );

            // FIXME: computing smooth normals in the future won't be
            // so simple as just computing a normal per face, we will
            // need to analyze larger parts of the geometry
            let face_normal = match normal_strategy {
                NormalStrategy::Sharp => compute_triangle_normal(
                    &vertices[cast_usize(v1)],
                    &vertices[cast_usize(v2)],
                    &vertices[cast_usize(v3)],
                ),
            };

            normals.push(face_normal);
        }

        assert_eq!(normals.len(), faces.len());
        assert_eq!(normals.capacity(), faces.len());

        Self {
            faces: faces
                .into_iter()
                .enumerate()
                .map(|(i, (i1, i2, i3))| {
                    let normal_index = cast_u32(i);
                    TriangleFace::new_separate(i1, i2, i3, normal_index, normal_index, normal_index)
                })
                .map(Face::from)
                .collect(),
            vertices,
            normals,
        }
    }

    /// Create new triangle face geometry from provided faces and
    /// vertices, remove orphan vertices and
    /// compute normals based on `normal_strategy`.
    ///
    /// # Panics
    /// Panics if faces refer to out-of-bounds vertices.
    pub fn from_triangle_faces_with_vertices_and_computed_normals_remove_orphans(
        faces: Vec<(u32, u32, u32)>,
        vertices: Vertices,
        normal_strategy: NormalStrategy,
    ) -> Self {
        let (faces_purged, vertices_purged) = remove_orphan_vertices(faces, vertices);
        Geometry::from_triangle_faces_with_vertices_and_computed_normals(
            faces_purged,
            vertices_purged,
            normal_strategy,
        )
    }

    /// Create new triangle face geometry from provided faces,
    /// vertices, and normals.
    ///
    /// # Panics
    /// Panics if faces refer to out-of-bounds vertices or normals.
    pub fn from_triangle_faces_with_vertices_and_normals(
        faces: Vec<TriangleFace>,
        vertices: Vertices,
        normals: Normals,
    ) -> Self {
        let vertices_range = 0..cast_u32(vertices.len());
        let normals_range = 0..cast_u32(normals.len());
        for face in &faces {
            let v = face.vertices;
            let n = face.normals;
            assert!(
                vertices_range.contains(&v.0),
                "Faces reference out of bounds position data"
            );
            assert!(
                vertices_range.contains(&v.1),
                "Faces reference out of bounds position data"
            );
            assert!(
                vertices_range.contains(&v.2),
                "Faces reference out of bounds position data"
            );
            assert!(
                normals_range.contains(&n.0),
                "Faces reference out of bounds normal data"
            );
            assert!(
                normals_range.contains(&n.1),
                "Faces reference out of bounds normal data"
            );
            assert!(
                normals_range.contains(&n.2),
                "Faces reference out of bounds normal data"
            );
        }

        Self {
            faces: faces.into_iter().map(Face::Triangle).collect(),
            vertices,
            normals,
        }
    }

    /// Create new triangle face geometry from provided faces,
    /// vertices, and normals and remove orphan vertices and normals.
    ///
    /// # Panics
    /// Panics if faces refer to out-of-bounds vertices or normals.
    pub fn from_triangle_faces_with_vertices_and_normals_remove_orphans(
        faces: Vec<TriangleFace>,
        vertices: Vertices,
        normals: Normals,
    ) -> Self {
        let (faces_purged, vertices_purged, normals_purged) =
            remove_orphan_vertices_and_normals(faces, vertices, normals);

        Geometry::from_triangle_faces_with_vertices_and_normals(
            faces_purged,
            vertices_purged,
            normals_purged,
        )
    }

    /// Return a view of all triangle faces in this geometry. Skip all
    /// other types of faces.
    pub fn triangle_faces_iter<'a>(&'a self) -> impl Iterator<Item = TriangleFace> + 'a {
        self.faces.iter().copied().map(|index| match index {
            Face::Triangle(f) => f,
        })
    }

    /// Return count of all triangle faces in this geometry. Skip all
    /// other types of faces.
    pub fn triangle_faces_len(&self) -> usize {
        self.faces
            .iter()
            .filter(|index| match index {
                Face::Triangle(_) => true,
            })
            .count()
    }

    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    pub fn vertices(&self) -> &[Point3<f32>] {
        &self.vertices
    }

    pub fn vertices_mut(&mut self) -> &mut [Point3<f32>] {
        &mut self.vertices
    }

    pub fn normals(&self) -> &[Vector3<f32>] {
        &self.normals
    }

    /// Extracts oriented edges from all mesh faces
    pub fn oriented_edges_iter<'a>(&'a self) -> impl Iterator<Item = OrientedEdge> + 'a {
        self.triangle_faces_iter()
            .flat_map(|face| ArrayVec::from(face.to_oriented_edges()).into_iter())
    }

    /// Extracts unoriented edges from all mesh faces
    pub fn unoriented_edges_iter<'a>(&'a self) -> impl Iterator<Item = UnorientedEdge> + 'a {
        self.triangle_faces_iter()
            .flat_map(|face| ArrayVec::from(face.to_unoriented_edges()).into_iter())
    }

<<<<<<< master
=======
    /// Genus of a mesh is the number of holes in topology / conectivity
    /// The mesh must be triangular and watertight
    /// V - E + F = 2 (1 - G)
    pub fn mesh_genus(&self, edges: &HashSet<UnorientedEdge>) -> i32 {
        let vertex_count = cast_i32(self.vertices.len());
        let edge_count = cast_i32(edges.len());
        let face_count = cast_i32(self.faces.len());

        1 - (vertex_count - edge_count + face_count) / 2
    }

>>>>>>> Calculate element to element topologies
    /// Does the mesh contain unused (not referenced in faces) vertices
    pub fn has_no_orphan_vertices(&self) -> bool {
        let mut used_vertices = HashSet::new();
        for face in self.triangle_faces_iter() {
            used_vertices.insert(face.vertices.0);
            used_vertices.insert(face.vertices.1);
            used_vertices.insert(face.vertices.2);
        }
        used_vertices.len() == self.vertices().len()
    }

    /// Does the mesh contain unused (not referenced in faces) normals
    pub fn has_no_orphan_normals(&self) -> bool {
        let mut used_normals = HashSet::new();
        for face in self.triangle_faces_iter() {
            used_normals.insert(face.normals.0);
            used_normals.insert(face.normals.1);
            used_normals.insert(face.normals.2);
        }
        used_normals.len() == self.normals().len()
    }

    /// Calculates topological relations (neighborhood) of mesh face -> faces.
    /// Returns a Map (key: face index, value: list of its neighboring faces indices)
    pub fn face_to_face_topology(&self) -> HashMap<usize, HashSet<usize>> {
        let mut f2f: HashMap<usize, HashSet<usize>> = HashMap::new();
        for (from_counter, f) in self.triangle_faces_iter().enumerate() {
            let [f_e_0, f_e_1, f_e_2] = f.to_unoriented_edges();
            for (to_counter, t_f) in self.triangle_faces_iter().enumerate() {
                if from_counter != to_counter && t_f.contains_unoriented_edge(f_e_0)
                    || t_f.contains_unoriented_edge(f_e_1)
                    || t_f.contains_unoriented_edge(f_e_2)
                {
                    let neighbors = f2f.entry(from_counter).or_insert_with(HashSet::new);
                    neighbors.insert(to_counter);
                }
            }
        }
        f2f
    }

    /// Calculates topological relations (neighborhood) of mesh edge -> faces.
    /// Returns a Map (key: edge index, value: list of its neighboring faces indices)
    pub fn edge_to_face_topology(
        &self,
        edges: &HashSet<UnorientedEdge>,
    ) -> HashMap<usize, HashSet<usize>> {
        let mut e2f: HashMap<usize, HashSet<usize>> = HashMap::new();
        for (from_counter, e) in edges.iter().enumerate() {
            for (to_counter, t_f) in self.triangle_faces_iter().enumerate() {
                if t_f.contains_unoriented_edge(*e) {
                    let neighbors = e2f.entry(from_counter).or_insert_with(HashSet::new);
                    neighbors.insert(to_counter);
                }
            }
        }
        e2f
    }

    /// Calculates topological relations (neighborhood) of mesh vertex -> faces.
    /// Returns a Map (key: vertex index, value: list of its neighboring faces indices)
    pub fn vertex_to_face_topology(&self) -> HashMap<usize, HashSet<usize>> {
        let mut v2f: HashMap<usize, HashSet<usize>> = HashMap::new();
        for from_counter in 0..self.vertices.len() {
            for (to_counter, t_f) in self.triangle_faces_iter().enumerate() {
                if t_f.contains_vertex(cast_u32(from_counter)) {
                    let neighbors = v2f.entry(from_counter).or_insert_with(HashSet::new);
                    neighbors.insert(to_counter);
                }
            }
        }
        v2f
    }

    /// Calculates topological relations (neighborhood) of mesh vertex -> edge.
    /// Returns a Map (key: vertex index, value: list of its neighboring edge indices)
    pub fn vertex_to_edge_topology(
        &self,
        edges: &HashSet<UnorientedEdge>,
    ) -> HashMap<usize, HashSet<usize>> {
        let mut v2e: HashMap<usize, HashSet<usize>> = HashMap::new();
        for from_counter in 0..self.vertices.len() {
            for (to_counter, e) in edges.iter().enumerate() {
                if e.0.contains_vertex(cast_u32(from_counter)) {
                    let neighbors = v2e.entry(from_counter).or_insert_with(HashSet::new);
                    neighbors.insert(to_counter);
                }
            }
        }
        v2e
    }

    /// Calculates topological relations (neighborhood) of mesh vertex -> vertex.
    /// Returns a Map (key: vertex index, value: list of its neighboring vertices indices)
    pub fn vertex_to_vertex_topology(&self) -> HashMap<usize, HashSet<usize>> {
        let mut v2v: HashMap<usize, HashSet<usize>> = HashMap::new();
        for f in self.triangle_faces_iter() {
            let neighbors_0 = v2v
                .entry(cast_usize(f.vertices.0))
                .or_insert_with(HashSet::new);
            neighbors_0.insert(cast_usize(f.vertices.1));
            neighbors_0.insert(cast_usize(f.vertices.2));
            let neighbors_1 = v2v
                .entry(cast_usize(f.vertices.1))
                .or_insert_with(HashSet::new);
            neighbors_1.insert(cast_usize(f.vertices.0));
            neighbors_1.insert(cast_usize(f.vertices.2));
            let neighbors_2 = v2v
                .entry(cast_usize(f.vertices.2))
                .or_insert_with(HashSet::new);
            neighbors_2.insert(cast_usize(f.vertices.0));
            neighbors_2.insert(cast_usize(f.vertices.1));
        }
        v2v
    }
}

/// A geometry index. Describes topology of geometry data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Triangle(TriangleFace),
}

impl From<TriangleFace> for Face {
    fn from(triangle_face: TriangleFace) -> Face {
        Face::Triangle(triangle_face)
    }
}

/// A triangular face. Contains indices to other geometry data, such
/// as vertices and normals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriangleFace {
    pub vertices: (u32, u32, u32),
    pub normals: (u32, u32, u32),
}

impl TriangleFace {
    pub fn new(i1: u32, i2: u32, i3: u32) -> TriangleFace {
        assert!(
            i1 != i2 && i1 != i3 && i2 != i3,
            "One or more face edges consists of the same vertex"
        );
        TriangleFace {
            vertices: (i1, i2, i3),
            normals: (i1, i2, i3),
        }
    }

    pub fn new_separate(
        vi1: u32,
        vi2: u32,
        vi3: u32,
        ni1: u32,
        ni2: u32,
        ni3: u32,
    ) -> TriangleFace {
        assert!(
            vi1 != vi2 && vi1 != vi3 && vi2 != vi3,
            "One or more face edges consists of the same vertex"
        );
        TriangleFace {
            vertices: (vi1, vi2, vi3),
            normals: (ni1, ni2, ni3),
        }
    }

    /// Generates 3 oriented edges from the respective triangular face
    pub fn to_oriented_edges(&self) -> [OrientedEdge; 3] {
        [
            OrientedEdge::new(self.vertices.0, self.vertices.1),
            OrientedEdge::new(self.vertices.1, self.vertices.2),
            OrientedEdge::new(self.vertices.2, self.vertices.0),
        ]
    }

    /// Generates 3 unoriented edges from the respective triangular face
    pub fn to_unoriented_edges(&self) -> [UnorientedEdge; 3] {
        [
            UnorientedEdge(OrientedEdge::new(self.vertices.0, self.vertices.1)),
            UnorientedEdge(OrientedEdge::new(self.vertices.1, self.vertices.2)),
            UnorientedEdge(OrientedEdge::new(self.vertices.2, self.vertices.0)),
        ]
    }

    /// Does the face contain the specific vertex
    pub fn contains_vertex(&self, vertex_index: u32) -> bool {
        self.vertices.0 == vertex_index
            || self.vertices.1 == vertex_index
            || self.vertices.2 == vertex_index
    }

    /// Does the face contain the specific unoriented edge
    pub fn contains_unoriented_edge(self, unoriented_edge: UnorientedEdge) -> bool {
        let [o_e_0, o_e_1, o_e_2] = self.to_unoriented_edges();
        o_e_0 == unoriented_edge || o_e_1 == unoriented_edge || o_e_2 == unoriented_edge
    }
}

impl From<(u32, u32, u32)> for TriangleFace {
    fn from((i1, i2, i3): (u32, u32, u32)) -> TriangleFace {
        TriangleFace::new(i1, i2, i3)
    }
}

/// Oriented face edge. Contains indices to other geometry data - vertices
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OrientedEdge {
    pub vertices: (u32, u32),
}

impl OrientedEdge {
    pub fn new(i1: u32, i2: u32) -> Self {
        assert!(
            i1 != i2,
            "The oriented edge is constituted of the same vertex"
        );
        OrientedEdge { vertices: (i1, i2) }
    }

    pub fn is_reverted(self, other: OrientedEdge) -> bool {
        self.vertices.0 == other.vertices.1 && self.vertices.1 == other.vertices.0
    }

    pub fn contains_vertex(self, vertex_index: u32) -> bool {
        self.vertices.0 == vertex_index || self.vertices.1 == vertex_index
    }
}

#[derive(Debug, Clone, Copy, Eq)]
pub struct UnorientedEdge(pub OrientedEdge);

impl PartialEq for UnorientedEdge {
    fn eq(&self, other: &Self) -> bool {
        (self.0.vertices.0 == other.0.vertices.0 && self.0.vertices.1 == other.0.vertices.1)
            || (self.0.vertices.0 == other.0.vertices.1 && self.0.vertices.1 == other.0.vertices.0)
    }
}

impl Hash for UnorientedEdge {
    fn hash<H: Hasher>(&self, state: &mut H) {
        cmp::min(self.0.vertices.0, self.0.vertices.1).hash(state);
        cmp::max(self.0.vertices.0, self.0.vertices.1).hash(state);
    }
}

fn remove_orphan_vertices(
    faces: Vec<(u32, u32, u32)>,
    vertices: Vertices,
) -> (Vec<(u32, u32, u32)>, Vertices) {
    let mut vertices_reduced: Vertices = Vec::with_capacity(vertices.len());
    let original_vertex_len = vertices.len();
    let unused_vertex_marker = vertices.len();
    let mut old_new_vertex_map: Vec<usize> = vec![unused_vertex_marker; original_vertex_len];
    let mut faces_renumbered: Vec<(u32, u32, u32)> = Vec::with_capacity(faces.len());

    for face in faces {
        let old_vertex_index_0 = cast_usize(face.0);
        let new_vertex_index_0 = if old_new_vertex_map[old_vertex_index_0] == unused_vertex_marker {
            let new_index = vertices_reduced.len();
            vertices_reduced.push(vertices[old_vertex_index_0]);
            old_new_vertex_map[old_vertex_index_0] = new_index;
            new_index
        } else {
            old_new_vertex_map[old_vertex_index_0]
        };

        let old_vertex_index_1 = cast_usize(face.1);
        let new_vertex_index_1 = if old_new_vertex_map[old_vertex_index_1] == unused_vertex_marker {
            let new_index = vertices_reduced.len();
            vertices_reduced.push(vertices[old_vertex_index_1]);
            old_new_vertex_map[old_vertex_index_1] = new_index;
            new_index
        } else {
            old_new_vertex_map[old_vertex_index_1]
        };

        let old_vertex_index_2 = cast_usize(face.2);
        let new_vertex_index_2 = if old_new_vertex_map[old_vertex_index_2] == unused_vertex_marker {
            let new_index = vertices_reduced.len();
            vertices_reduced.push(vertices[old_vertex_index_2]);
            old_new_vertex_map[old_vertex_index_2] = new_index;
            new_index
        } else {
            old_new_vertex_map[old_vertex_index_2]
        };

        faces_renumbered.push((
            cast_u32(new_vertex_index_0),
            cast_u32(new_vertex_index_1),
            cast_u32(new_vertex_index_2),
        ));
    }

    faces_renumbered.shrink_to_fit();
    vertices_reduced.shrink_to_fit();

    (faces_renumbered, vertices_reduced)
}

#[allow(dead_code)]
fn remove_orphan_normals(
    faces: Vec<TriangleFace>,
    normals: Normals,
) -> (Vec<TriangleFace>, Normals) {
    let mut normals_reduced: Normals = Vec::with_capacity(normals.len());
    let original_normal_len = normals.len();
    let unused_normal_marker = normals.len();
    let mut old_new_normal_map: Vec<usize> = vec![unused_normal_marker; original_normal_len];
    let mut faces_renumbered: Vec<TriangleFace> = Vec::with_capacity(faces.len());

    for face in faces {
        let old_normal_index_0 = cast_usize(face.normals.0);
        let new_normal_index_0 = if old_new_normal_map[old_normal_index_0] == unused_normal_marker {
            let new_index = normals_reduced.len();
            normals_reduced.push(normals[old_normal_index_0]);
            old_new_normal_map[old_normal_index_0] = new_index;
            new_index
        } else {
            old_new_normal_map[old_normal_index_0]
        };

        let old_normal_index_1 = cast_usize(face.normals.1);
        let new_normal_index_1 = if old_new_normal_map[old_normal_index_1] == unused_normal_marker {
            let new_index = normals_reduced.len();
            normals_reduced.push(normals[old_normal_index_1]);
            old_new_normal_map[old_normal_index_1] = new_index;
            new_index
        } else {
            old_new_normal_map[old_normal_index_1]
        };

        let old_normal_index_2 = cast_usize(face.normals.2);
        let new_normal_index_2 = if old_new_normal_map[old_normal_index_2] == unused_normal_marker {
            let new_index = normals_reduced.len();
            normals_reduced.push(normals[old_normal_index_2]);
            old_new_normal_map[old_normal_index_2] = new_index;
            new_index
        } else {
            old_new_normal_map[old_normal_index_2]
        };

        faces_renumbered.push(TriangleFace::new_separate(
            face.vertices.0,
            face.vertices.1,
            face.vertices.2,
            cast_u32(new_normal_index_0),
            cast_u32(new_normal_index_1),
            cast_u32(new_normal_index_2),
        ));
    }

    faces_renumbered.shrink_to_fit();
    normals_reduced.shrink_to_fit();

    (faces_renumbered, normals_reduced)
}

fn remove_orphan_vertices_and_normals(
    faces: Vec<TriangleFace>,
    vertices: Vertices,
    normals: Normals,
) -> (Vec<TriangleFace>, Vertices, Normals) {
    let mut vertices_reduced: Vertices = Vec::with_capacity(vertices.len());
    let original_vertex_len = vertices.len();
    let unused_vertex_marker = vertices.len();
    let mut old_new_vertex_map: Vec<usize> = vec![unused_vertex_marker; original_vertex_len];

    let mut normals_reduced: Normals = Vec::with_capacity(normals.len());
    let original_normal_len = normals.len();
    let unused_normal_marker = normals.len();
    let mut old_new_normal_map: Vec<usize> = vec![unused_normal_marker; original_normal_len];

    let mut faces_renumbered: Vec<TriangleFace> = Vec::with_capacity(faces.len());

    for face in faces {
        let old_vertex_index_0 = cast_usize(face.vertices.0);
        let new_vertex_index_0 = if old_new_vertex_map[old_vertex_index_0] == unused_vertex_marker {
            let new_index = vertices_reduced.len();
            vertices_reduced.push(vertices[old_vertex_index_0]);
            old_new_vertex_map[old_vertex_index_0] = new_index;
            new_index
        } else {
            old_new_vertex_map[old_vertex_index_0]
        };

        let old_vertex_index_1 = cast_usize(face.vertices.1);
        let new_vertex_index_1 = if old_new_vertex_map[old_vertex_index_1] == unused_vertex_marker {
            let new_index = vertices_reduced.len();
            vertices_reduced.push(vertices[old_vertex_index_1]);
            old_new_vertex_map[old_vertex_index_1] = new_index;
            new_index
        } else {
            old_new_vertex_map[old_vertex_index_1]
        };

        let old_vertex_index_2 = cast_usize(face.vertices.2);
        let new_vertex_index_2 = if old_new_vertex_map[old_vertex_index_2] == unused_vertex_marker {
            let new_index = vertices_reduced.len();
            vertices_reduced.push(vertices[old_vertex_index_2]);
            old_new_vertex_map[old_vertex_index_2] = new_index;
            new_index
        } else {
            old_new_vertex_map[old_vertex_index_2]
        };

        let old_normal_index_0 = cast_usize(face.normals.0);
        let new_normal_index_0 = if old_new_normal_map[old_normal_index_0] == unused_normal_marker {
            let new_index = normals_reduced.len();
            normals_reduced.push(normals[old_normal_index_0]);
            old_new_normal_map[old_normal_index_0] = new_index;
            new_index
        } else {
            old_new_normal_map[old_normal_index_0]
        };

        let old_normal_index_1 = cast_usize(face.normals.1);
        let new_normal_index_1 = if old_new_normal_map[old_normal_index_1] == unused_normal_marker {
            let new_index = normals_reduced.len();
            normals_reduced.push(normals[old_normal_index_1]);
            old_new_normal_map[old_normal_index_1] = new_index;
            new_index
        } else {
            old_new_normal_map[old_normal_index_1]
        };

        let old_normal_index_2 = cast_usize(face.normals.2);
        let new_normal_index_2 = if old_new_normal_map[old_normal_index_2] == unused_normal_marker {
            let new_index = normals_reduced.len();
            normals_reduced.push(normals[old_normal_index_2]);
            old_new_normal_map[old_normal_index_2] = new_index;
            new_index
        } else {
            old_new_normal_map[old_normal_index_2]
        };

        faces_renumbered.push(TriangleFace::new_separate(
            cast_u32(new_vertex_index_0),
            cast_u32(new_vertex_index_1),
            cast_u32(new_vertex_index_2),
            cast_u32(new_normal_index_0),
            cast_u32(new_normal_index_1),
            cast_u32(new_normal_index_2),
        ));
    }

    faces_renumbered.shrink_to_fit();
    vertices_reduced.shrink_to_fit();
    normals_reduced.shrink_to_fit();

    (faces_renumbered, vertices_reduced, normals_reduced)
}

pub fn plane_same_len(position: [f32; 3], scale: f32) -> Geometry {
    #[rustfmt::skip]
    let vertex_positions = vec![
        v(-1.0, -1.0,  0.0, position, scale),
        v( 1.0, -1.0,  0.0, position, scale),
        v( 1.0,  1.0,  0.0, position, scale),
        v( 1.0,  1.0,  0.0, position, scale),
        v(-1.0,  1.0,  0.0, position, scale),
        v(-1.0, -1.0,  0.0, position, scale),
    ];

    #[rustfmt::skip]
    let vertex_normals = vec![
        n( 0.0,  0.0,  1.0),
        n( 0.0,  0.0,  1.0),
        n( 0.0,  0.0,  1.0),
        n( 0.0,  0.0,  1.0),
        n( 0.0,  0.0,  1.0),
        n( 0.0,  0.0,  1.0),
    ];

    #[rustfmt::skip]
    let faces = vec![
        TriangleFace::new(0, 1, 2),
        TriangleFace::new(3, 4, 5),
    ];

    Geometry::from_triangle_faces_with_vertices_and_normals(faces, vertex_positions, vertex_normals)
}

pub fn plane_var_len(position: [f32; 3], scale: f32) -> Geometry {
    #[rustfmt::skip]
    let vertex_positions = vec![
        v(-1.0, -1.0,  0.0, position, scale),
        v( 1.0, -1.0,  0.0, position, scale),
        v( 1.0,  1.0,  0.0, position, scale),
        v( 1.0,  1.0,  0.0, position, scale),
        v(-1.0,  1.0,  0.0, position, scale),
        v(-1.0, -1.0,  0.0, position, scale),
    ];

    #[rustfmt::skip]
    let vertex_normals = vec![
        n( 0.0,  0.0,  1.0),
    ];

    #[rustfmt::skip]
    let faces = vec![
        TriangleFace::new_separate(0, 1, 2, 0, 0, 0),
        TriangleFace::new_separate(3, 4, 5, 0, 0, 0),
    ];

    Geometry::from_triangle_faces_with_vertices_and_normals(faces, vertex_positions, vertex_normals)
}

pub fn cube_smooth_same_len(position: [f32; 3], scale: f32) -> Geometry {
    #[rustfmt::skip]
    let vertex_positions = vec![
        // back
        v(-1.0,  1.0, -1.0, position, scale),
        v(-1.0,  1.0,  1.0, position, scale),
        v( 1.0,  1.0,  1.0, position, scale),
        v( 1.0,  1.0, -1.0, position, scale),
        // front
        v(-1.0, -1.0, -1.0, position, scale),
        v( 1.0, -1.0, -1.0, position, scale),
        v( 1.0, -1.0,  1.0, position, scale),
        v(-1.0, -1.0,  1.0, position, scale),
    ];

    // FIXME: make const once float arithmetic is stabilized in const fns
    // let sqrt_3 = 3.0f32.sqrt();
    let frac_1_sqrt_3 = 1.0 / 3.0_f32.sqrt();

    #[rustfmt::skip]
    let vertex_normals = vec![
        // back
        n(-frac_1_sqrt_3,  frac_1_sqrt_3, -frac_1_sqrt_3),
        n(-frac_1_sqrt_3,  frac_1_sqrt_3,  frac_1_sqrt_3),
        n( frac_1_sqrt_3,  frac_1_sqrt_3,  frac_1_sqrt_3),
        n( frac_1_sqrt_3,  frac_1_sqrt_3, -frac_1_sqrt_3),
        // front
        n(-frac_1_sqrt_3, -frac_1_sqrt_3, -frac_1_sqrt_3),
        n( frac_1_sqrt_3, -frac_1_sqrt_3, -frac_1_sqrt_3),
        n( frac_1_sqrt_3, -frac_1_sqrt_3,  frac_1_sqrt_3),
        n(-frac_1_sqrt_3, -frac_1_sqrt_3,  frac_1_sqrt_3),
    ];

    #[rustfmt::skip]
    let faces = vec![
        // back
        TriangleFace::new(0, 1, 2),
        TriangleFace::new(2, 3, 0),
        // front
        TriangleFace::new(4, 5, 6),
        TriangleFace::new(6, 7, 4),
        // top
        TriangleFace::new(7, 6, 2),
        TriangleFace::new(2, 1, 7),
        // bottom
        TriangleFace::new(4, 0, 3),
        TriangleFace::new(3, 5, 4),
        // right
        TriangleFace::new(5, 3, 2),
        TriangleFace::new(2, 6, 5),
        // left
        TriangleFace::new(4, 7, 1),
        TriangleFace::new(1, 0, 4),
    ];

    Geometry::from_triangle_faces_with_vertices_and_normals(faces, vertex_positions, vertex_normals)
}

#[deprecated(note = "Don't use, generates open geometry")]
// FIXME: Remove eventually
pub fn cube_sharp_same_len(position: [f32; 3], scale: f32) -> Geometry {
    #[rustfmt::skip]
    let vertex_positions = vec![
        // back
        v(-1.0,  1.0, -1.0, position, scale),
        v(-1.0,  1.0,  1.0, position, scale),
        v( 1.0,  1.0,  1.0, position, scale),
        v( 1.0,  1.0, -1.0, position, scale),
        // front
        v(-1.0, -1.0, -1.0, position, scale),
        v( 1.0, -1.0, -1.0, position, scale),
        v( 1.0, -1.0,  1.0, position, scale),
        v(-1.0, -1.0,  1.0, position, scale),
        // top
        v(-1.0,  1.0,  1.0, position, scale),
        v(-1.0, -1.0,  1.0, position, scale),
        v( 1.0, -1.0,  1.0, position, scale),
        v( 1.0,  1.0,  1.0, position, scale),
        // bottom
        v(-1.0,  1.0, -1.0, position, scale),
        v( 1.0,  1.0, -1.0, position, scale),
        v( 1.0, -1.0, -1.0, position, scale),
        v(-1.0, -1.0, -1.0, position, scale),
        // right
        v( 1.0,  1.0, -1.0, position, scale),
        v( 1.0,  1.0,  1.0, position, scale),
        v( 1.0, -1.0,  1.0, position, scale),
        v( 1.0, -1.0, -1.0, position, scale),
        // left
        v(-1.0,  1.0, -1.0, position, scale),
        v(-1.0, -1.0, -1.0, position, scale),
        v(-1.0, -1.0,  1.0, position, scale),
        v(-1.0,  1.0,  1.0, position, scale),
    ];

    #[rustfmt::skip]
    let vertex_normals = vec![
        // back
        n( 0.0,  1.0,  0.0),
        n( 0.0,  1.0,  0.0),
        n( 0.0,  1.0,  0.0),
        n( 0.0,  1.0,  0.0),
        // front
        n( 0.0, -1.0,  0.0),
        n( 0.0, -1.0,  0.0),
        n( 0.0, -1.0,  0.0),
        n( 0.0, -1.0,  0.0),
        // top
        n( 0.0,  0.0,  1.0),
        n( 0.0,  0.0,  1.0),
        n( 0.0,  0.0,  1.0),
        n( 0.0,  0.0,  1.0),
        // bottom
        n( 0.0,  0.0, -1.0),
        n( 0.0,  0.0, -1.0),
        n( 0.0,  0.0, -1.0),
        n( 0.0,  0.0, -1.0),
        // right
        n( 1.0,  0.0,  0.0),
        n( 1.0,  0.0,  0.0),
        n( 1.0,  0.0,  0.0),
        n( 1.0,  0.0,  0.0),
        // left
        n(-1.0,  0.0,  0.0),
        n(-1.0,  0.0,  0.0),
        n(-1.0,  0.0,  0.0),
        n(-1.0,  0.0,  0.0),
    ];

    #[rustfmt::skip]
    let faces = vec![
        // back
        TriangleFace::new(0, 1, 2),
        TriangleFace::new(2, 3, 0),
        // front
        TriangleFace::new(4, 5, 6),
        TriangleFace::new(6, 7, 4),
        // top
        TriangleFace::new(8, 9, 10),
        TriangleFace::new(10, 11, 8),
        // bottom
        TriangleFace::new(12, 13, 14),
        TriangleFace::new(14, 15, 12),
        // right
        TriangleFace::new(16, 17, 18),
        TriangleFace::new(18, 19, 16),
        // left
        TriangleFace::new(20, 21, 22),
        TriangleFace::new(22, 23, 20),
    ];

    Geometry::from_triangle_faces_with_vertices_and_normals(faces, vertex_positions, vertex_normals)
}

pub fn cube_sharp_var_len(position: [f32; 3], scale: f32) -> Geometry {
    #[rustfmt::skip]
    let vertex_positions = vec![
        // back
        v(-1.0,  1.0, -1.0, position, scale),
        v(-1.0,  1.0,  1.0, position, scale),
        v( 1.0,  1.0,  1.0, position, scale),
        v( 1.0,  1.0, -1.0, position, scale),
        // front
        v(-1.0, -1.0, -1.0, position, scale),
        v( 1.0, -1.0, -1.0, position, scale),
        v( 1.0, -1.0,  1.0, position, scale),
        v(-1.0, -1.0,  1.0, position, scale),
    ];

    #[rustfmt::skip]
    let vertex_normals = vec![
        // back
        n( 0.0,  1.0,  0.0),
        // front
        n( 0.0, -1.0,  0.0),
        // top
        n( 0.0,  0.0,  1.0),
        // bottom
        n( 0.0,  0.0, -1.0),
        // right
        n( 1.0,  0.0,  0.0),
        // left
        n(-1.0,  0.0,  0.0),
    ];

    #[rustfmt::skip]
    let faces = vec![
        // back
        TriangleFace::new_separate(0, 1, 2, 0, 0, 0),
        TriangleFace::new_separate(2, 3, 0, 0, 0, 0),
        // front
        TriangleFace::new_separate(4, 5, 6, 1, 1, 1),
        TriangleFace::new_separate(6, 7, 4, 1, 1, 1),
        // top
        TriangleFace::new_separate(7, 6, 2, 2, 2, 2),
        TriangleFace::new_separate(2, 1, 7, 2, 2, 2),
        // bottom
        TriangleFace::new_separate(4, 0, 3, 3, 3, 3),
        TriangleFace::new_separate(3, 5, 4, 3, 3, 3),
        // right
        TriangleFace::new_separate(5, 3, 2, 4, 4, 4),
        TriangleFace::new_separate(2, 6, 5, 4, 4, 4),
        // left
        TriangleFace::new_separate(4, 7, 1, 5, 5, 5),
        TriangleFace::new_separate(1, 0, 4, 5, 5, 5),
    ];

    Geometry::from_triangle_faces_with_vertices_and_normals(faces, vertex_positions, vertex_normals)
}

/// Create UV Sphere primitive at `position` with `scale`,
/// `n_parallels` and `n_meridians`.
///
/// # Panics
/// Panics if number of parallels is less than 2 or number of
/// meridians is less than 3.
pub fn uv_sphere(position: [f32; 3], scale: f32, n_parallels: u32, n_meridians: u32) -> Geometry {
    assert!(n_parallels >= 2, "Need at least 2 paralells");
    assert!(n_meridians >= 3, "Need at least 3 meridians");

    // Add the poles
    let lat_line_max = n_parallels + 2;
    // Add the last, wrapping meridian
    let lng_line_max = n_meridians + 1;

    use std::f32::consts::PI;
    const TWO_PI: f32 = 2.0 * PI;

    // 1 North pole + 1 South pole + `n_parallels` * `n_meridians`
    let vertex_data_count = cast_usize(2 + n_parallels * n_meridians);
    let mut vertex_positions = Vec::with_capacity(vertex_data_count);

    // Produce vertex data for bands in between parallels

    for lat_line in 0..n_parallels {
        for lng_line in 0..n_meridians {
            let polar_t = (lat_line + 1) as f32 / (lat_line_max - 1) as f32;
            let azimuthal_t = lng_line as f32 / (lng_line_max - 1) as f32;

            let x = (PI * polar_t).sin() * (TWO_PI * azimuthal_t).cos();
            let y = (PI * polar_t).sin() * (TWO_PI * azimuthal_t).sin();
            let z = (PI * polar_t).cos();

            vertex_positions.push(v(x, y, z, position, scale));
        }
    }

    // Triangles from North and South poles to the nearest band + 2 * quads in bands
    let faces_count = cast_usize(2 * n_meridians + 2 * n_meridians * (n_parallels - 1));
    let mut faces = Vec::with_capacity(faces_count);

    // Produce faces for bands in-between parallels

    for i in 1..n_parallels {
        for j in 0..n_meridians {
            // Produces 2 CCW wound triangles: (p1, p2, p3) and (p3, p4, p1)

            let p1 = i * n_meridians + j;
            let p2 = i * n_meridians + ((j + 1) % n_meridians);

            let p4 = (i - 1) * n_meridians + j;
            let p3 = (i - 1) * n_meridians + ((j + 1) % n_meridians);

            faces.push((p1, p2, p3));
            faces.push((p3, p4, p1));
        }
    }

    // Add vertex data and band-connecting faces for North and South poles

    let north_pole = cast_u32(vertex_positions.len());
    vertex_positions.push(v(0.0, 0.0, 1.0, position, scale));

    let south_pole = cast_u32(vertex_positions.len());
    vertex_positions.push(v(0.0, 0.0, -1.0, position, scale));

    for i in 0..n_meridians {
        let north_p1 = i;
        let north_p2 = (i + 1) % n_meridians;

        let south_p1 = (n_parallels - 1) * n_meridians + i;
        let south_p2 = (n_parallels - 1) * n_meridians + ((i + 1) % n_meridians);

        faces.push((north_p1, north_p2, north_pole));
        faces.push((south_p2, south_p1, south_pole));
    }

    assert_eq!(vertex_positions.len(), vertex_data_count);
    assert_eq!(vertex_positions.capacity(), vertex_data_count);
    assert_eq!(faces.len(), faces_count);
    assert_eq!(faces.capacity(), faces_count);

    Geometry::from_triangle_faces_with_vertices_and_computed_normals(
        faces,
        vertex_positions,
        NormalStrategy::Sharp,
    )
}

pub fn compute_bounding_sphere(geometries: &[Geometry]) -> (Point3<f32>, f32) {
    let centroid = compute_centroid(geometries);
    let mut max_distance_squared = 0.0;

    for geometry in geometries {
        for vertex in &geometry.vertices {
            let distance_squared = na::distance_squared(&centroid, vertex);
            if distance_squared > max_distance_squared {
                max_distance_squared = distance_squared;
            }
        }
    }

    (centroid, max_distance_squared.sqrt())
}

pub fn compute_centroid(geometries: &[Geometry]) -> Point3<f32> {
    let mut vertex_count = 0;
    let mut centroid = Point3::origin();
    for geometry in geometries {
        vertex_count += geometry.vertices.len();
        for vertex in &geometry.vertices {
            let v = vertex - Point3::origin();
            centroid += v;
        }
    }

    centroid / (vertex_count as f32)
}

pub fn find_closest_point(position: &Point3<f32>, geometry: &Geometry) -> Option<Point3<f32>> {
    let vertices = geometry.vertices();
    if vertices.is_empty() {
        return None;
    }

    let mut closest = vertices[0];
    let mut closest_distance_squared = na::distance_squared(position, &closest);
    for point in &vertices[1..] {
        let distance_squared = na::distance_squared(position, &point);
        if distance_squared < closest_distance_squared {
            closest = *point;
            closest_distance_squared = distance_squared;
        }
    }

    Some(closest)
}

fn v(x: f32, y: f32, z: f32, translation: [f32; 3], scale: f32) -> Point3<f32> {
    Point3::new(
        scale * x + translation[0],
        scale * y + translation[1],
        scale * z + translation[2],
    )
}

fn n(x: f32, y: f32, z: f32) -> Vector3<f32> {
    Vector3::new(x, y, z)
}

fn compute_triangle_normal(p1: &Point3<f32>, p2: &Point3<f32>, p3: &Point3<f32>) -> Vector3<f32> {
    let u = p2 - p1;
    let v = p3 - p1;

    Vector3::cross(&u, &v)
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;

    use crate::test_geometry_fixtures::{double_torus, torus, triple_torus};

    use super::*;

    fn quad() -> (Vec<(u32, u32, u32)>, Vertices) {
        #[rustfmt::skip]
        let vertices = vec![
            v(-1.0, -1.0,  0.0, [0.0, 0.0, 0.0], 1.0),
            v( 1.0, -1.0,  0.0, [0.0, 0.0, 0.0], 1.0),
            v( 1.0,  1.0,  0.0, [0.0, 0.0, 0.0], 1.0),
            v(-1.0,  1.0,  0.0, [0.0, 0.0, 0.0], 1.0),
        ];

        #[rustfmt::skip]
        let faces = vec![
            (0, 1, 2),
            (2, 3, 0),
        ];

        (faces, vertices)
    }

    fn quad_with_normals() -> (Vec<TriangleFace>, Vertices, Normals) {
        #[rustfmt::skip]
        let vertices = vec![
            v(-1.0, -1.0,  0.0, [0.0, 0.0, 0.0], 1.0),
            v( 1.0, -1.0,  0.0, [0.0, 0.0, 0.0], 1.0),
            v( 1.0,  1.0,  0.0, [0.0, 0.0, 0.0], 1.0),
            v(-1.0,  1.0,  0.0, [0.0, 0.0, 0.0], 1.0),
        ];

        #[rustfmt::skip]
        let normals = vec![
            n( 0.0,  0.0,  1.0),
            n( 0.0,  0.0,  1.0),
            n( 0.0,  0.0,  1.0),
            n( 0.0,  0.0,  1.0),
        ];

        #[rustfmt::skip]
        let faces = vec![
            TriangleFace::new(0, 1, 2),
            TriangleFace::new(2, 3, 0),
        ];

        (faces, vertices, normals)
    }

    fn tessellated_triangle() -> (Vec<(u32, u32, u32)>, Vec<Point3<f32>>) {
        #[rustfmt::skip]
            let vertices = vec![
            Point3::new(-2.0, -2.0, 0.0),
            Point3::new(0.0, -2.0, 0.0),
            Point3::new(2.0, -2.0, 0.0),
            Point3::new(-1.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 2.0, 0.0),
        ];

        #[rustfmt::skip]
            let faces = vec![
            (0, 3, 1),
            (1, 3, 4),
            (1, 4, 2),
            (3, 5, 4),
        ];

        (faces, vertices)
    }

    #[test]
    fn test_geometry_from_triangle_faces_with_vertices_and_computed_normals() {
        let (faces, vertices) = quad();
        let geometry = Geometry::from_triangle_faces_with_vertices_and_computed_normals(
            faces.clone(),
            vertices.clone(),
            NormalStrategy::Sharp,
        );
        let geometry_faces: Vec<_> = geometry.triangle_faces_iter().collect();

        assert_eq!(vertices.as_slice(), geometry.vertices());
        assert_eq!(
            faces,
            geometry_faces
                .into_iter()
                .map(|triangle_face| triangle_face.vertices)
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    #[should_panic(expected = "Faces reference out of bounds position data")]
    fn test_geometry_from_triangle_faces_with_vertices_and_computed_normals_bounds_check() {
        let (_, vertices) = quad();
        #[rustfmt::skip]
        let faces = vec![
            (0, 1, 2),
            (2, 3, 4),
        ];

        Geometry::from_triangle_faces_with_vertices_and_computed_normals(
            faces,
            vertices,
            NormalStrategy::Sharp,
        );
    }

    #[test]
    fn test_geometry_from_triangle_faces_with_vertices_and_normals() {
        let (faces, vertices, normals) = quad_with_normals();
        let geometry = Geometry::from_triangle_faces_with_vertices_and_normals(
            faces.clone(),
            vertices.clone(),
            normals.clone(),
        );
        let geometry_faces: Vec<_> = geometry.triangle_faces_iter().collect();

        assert_eq!(vertices.as_slice(), geometry.vertices());
        assert_eq!(normals.as_slice(), geometry.normals());
        assert_eq!(faces.as_slice(), geometry_faces.as_slice());
    }

    #[test]
    #[should_panic(expected = "Faces reference out of bounds position data")]
    fn test_geometry_from_triangle_faces_with_vertices_and_normals_bounds_check() {
        let (_, vertices, normals) = quad_with_normals();
        #[rustfmt::skip]
        let faces = vec![
            TriangleFace::new(0, 1, 2),
            TriangleFace::new(2, 3, 4),
        ];

        Geometry::from_triangle_faces_with_vertices_and_normals(
            faces.clone(),
            vertices.clone(),
            normals.clone(),
        );
    }

    #[test]
    fn test_oriented_edge_eq_returns_true() {
        let oriented_edge_one_way = OrientedEdge::new(0, 1);
        let oriented_edge_other_way = OrientedEdge::new(0, 1);
        assert_eq!(oriented_edge_one_way, oriented_edge_other_way);
    }

    #[test]
    fn test_oriented_edge_eq_returns_false_because_different() {
        let oriented_edge_one_way = OrientedEdge::new(0, 1);
        let oriented_edge_other_way = OrientedEdge::new(2, 1);
        assert_ne!(oriented_edge_one_way, oriented_edge_other_way);
    }

    #[test]
    fn test_oriented_edge_eq_returns_false_because_reverted() {
        let oriented_edge_one_way = OrientedEdge::new(0, 1);
        let oriented_edge_other_way = OrientedEdge::new(1, 0);
        assert_ne!(oriented_edge_one_way, oriented_edge_other_way);
    }

    #[test]
    #[should_panic(expected = "The oriented edge is constituted of the same vertex")]
    fn test_oriented_edge_constructor_consists_of_the_same_vertex_should_panic() {
        OrientedEdge::new(0, 0);
    }

    #[test]
    fn test_oriented_edge_constructor_does_not_consist_of_the_same_vertex_should_pass() {
        OrientedEdge::new(0, 1);
    }

    #[test]
    fn test_oriented_edge_is_reverted_returns_true_because_same() {
        let oriented_edge_one_way = OrientedEdge::new(0, 1);
        let oriented_edge_other_way = OrientedEdge::new(1, 0);
        assert!(oriented_edge_one_way.is_reverted(oriented_edge_other_way));
    }

    #[test]
    fn test_oriented_edge_is_reverted_returns_false_because_is_same() {
        let oriented_edge_one_way = OrientedEdge::new(0, 1);
        let oriented_edge_other_way = OrientedEdge::new(0, 1);
        assert!(!oriented_edge_one_way.is_reverted(oriented_edge_other_way));
    }

    #[test]
    fn test_oriented_edge_is_reverted_returns_false_because_is_different() {
        let oriented_edge_one_way = OrientedEdge::new(0, 1);
        let oriented_edge_other_way = OrientedEdge::new(2, 1);
        assert!(!oriented_edge_one_way.is_reverted(oriented_edge_other_way));
    }

    #[test]
    fn test_unoriented_edge_eq_returns_true_because_same() {
        let unoriented_edge_one_way = UnorientedEdge(OrientedEdge::new(0, 1));
        let unoriented_edge_other_way = UnorientedEdge(OrientedEdge::new(0, 1));
        assert_eq!(unoriented_edge_one_way, unoriented_edge_other_way);
    }

    #[test]
    fn test_unoriented_edge_eq_returns_true_because_reverted() {
        let unoriented_edge_one_way = UnorientedEdge(OrientedEdge::new(0, 1));
        let unoriented_edge_other_way = UnorientedEdge(OrientedEdge::new(1, 0));
        assert_eq!(unoriented_edge_one_way, unoriented_edge_other_way);
    }

    #[test]
    fn test_unoriented_edge_eq_returns_false_because_different() {
        let unoriented_edge_one_way = UnorientedEdge(OrientedEdge::new(0, 1));
        let unoriented_edge_other_way = UnorientedEdge(OrientedEdge::new(2, 1));
        assert_ne!(unoriented_edge_one_way, unoriented_edge_other_way);
    }

    #[test]
    fn test_unoriented_edge_hash_returns_true_because_same() {
        let unoriented_edge_one_way = UnorientedEdge(OrientedEdge::new(0, 1));
        let unoriented_edge_other_way = UnorientedEdge(OrientedEdge::new(0, 1));
        let mut hasher_1 = DefaultHasher::new();
        let mut hasher_2 = DefaultHasher::new();
        unoriented_edge_one_way.hash(&mut hasher_1);
        unoriented_edge_other_way.hash(&mut hasher_2);
        assert_eq!(hasher_1.finish(), hasher_2.finish());
    }

    #[test]
    fn test_unoriented_edge_hash_returns_true_because_reverted() {
        let unoriented_edge_one_way = UnorientedEdge(OrientedEdge::new(0, 1));
        let unoriented_edge_other_way = UnorientedEdge(OrientedEdge::new(1, 0));
        let mut hasher_1 = DefaultHasher::new();
        let mut hasher_2 = DefaultHasher::new();
        unoriented_edge_one_way.hash(&mut hasher_1);
        unoriented_edge_other_way.hash(&mut hasher_2);
        assert_eq!(hasher_1.finish(), hasher_2.finish());
    }

    #[test]
    fn test_triangle_face_to_oriented_edges() {
        let face = TriangleFace::new(0, 1, 2);

        let oriented_edges_correct: [OrientedEdge; 3] = [
            OrientedEdge::new(0, 1),
            OrientedEdge::new(1, 2),
            OrientedEdge::new(2, 0),
        ];

        let oriented_edges_to_check: [OrientedEdge; 3] = face.to_oriented_edges();

        assert_eq!(oriented_edges_to_check[0], oriented_edges_correct[0]);
        assert_eq!(oriented_edges_to_check[1], oriented_edges_correct[1]);
        assert_eq!(oriented_edges_to_check[2], oriented_edges_correct[2]);
    }

    #[test]
    fn test_triangle_face_to_unoriented_edges() {
        let face = TriangleFace::new(0, 1, 2);

        let unoriented_edges_correct: [UnorientedEdge; 3] = [
            UnorientedEdge(OrientedEdge::new(0, 1)),
            UnorientedEdge(OrientedEdge::new(1, 2)),
            UnorientedEdge(OrientedEdge::new(2, 0)),
        ];

        let unoriented_edges_to_check: [UnorientedEdge; 3] = face.to_unoriented_edges();

        assert_eq!(unoriented_edges_to_check[0], unoriented_edges_correct[0]);
        assert_eq!(unoriented_edges_to_check[1], unoriented_edges_correct[1]);
        assert_eq!(unoriented_edges_to_check[2], unoriented_edges_correct[2]);
    }

    #[test]
    #[should_panic(expected = "One or more face edges consists of the same vertex")]
    fn test_triangle_face_new_with_invalid_vertex_indices_0_1_should_panic() {
        TriangleFace::new(0, 0, 2);
    }

    #[test]
    #[should_panic(expected = "One or more face edges consists of the same vertex")]
    fn test_triangle_face_new_with_invalid_vertex_indices_1_2_should_panic() {
        TriangleFace::new(0, 2, 2);
    }

    #[test]
    #[should_panic(expected = "One or more face edges consists of the same vertex")]
    fn test_triangle_face_new_with_invalid_vertex_indices_0_2_should_panic() {
        TriangleFace::new(0, 2, 0);
    }

    #[test]
    #[should_panic(expected = "One or more face edges consists of the same vertex")]
    fn test_triangle_face_new_separate_with_invalid_vertex_indices_0_1_should_panic() {
        TriangleFace::new_separate(0, 0, 2, 0, 0, 0);
    }

    #[test]
    #[should_panic(expected = "One or more face edges consists of the same vertex")]
    fn test_triangle_face_new_separate_with_invalid_vertex_indices_1_2_should_panic() {
        TriangleFace::new_separate(0, 2, 2, 0, 0, 0);
    }

    #[test]
    #[should_panic(expected = "One or more face edges consists of the same vertex")]
    fn test_triangle_face_new_separate_with_invalid_vertex_indices_0_2_should_panic() {
        TriangleFace::new_separate(0, 2, 0, 0, 0, 0);
    }

    #[test]
    fn test_has_no_orphan_vertices_returns_true_if_there_are_some() {
        let (faces, vertices, normals) = quad_with_normals();

        let geometry_without_orphans = Geometry::from_triangle_faces_with_vertices_and_normals(
            faces.clone(),
            vertices.clone(),
            normals.clone(),
        );

        assert!(geometry_without_orphans.has_no_orphan_vertices());
    }

    #[test]
    fn test_has_no_orphan_vertices_returns_false_if_there_are_none() {
        let (faces, vertices, normals) = quad_with_normals();
        let extra_vertex = vec![v(0.0, 0.0, 0.0, [0.0, 0.0, 0.0], 1.0)];
        let vertices_extended = [&vertices[..], &extra_vertex[..]].concat();

        let geometry_with_orphans = Geometry::from_triangle_faces_with_vertices_and_normals(
            faces.clone(),
            vertices_extended.clone(),
            normals.clone(),
        );

        assert!(!geometry_with_orphans.has_no_orphan_vertices());
    }

    #[test]
    fn test_has_no_orphan_normals_returns_true_if_there_are_some() {
        let (faces, vertices, normals) = quad_with_normals();

        let geometry_without_orphans = Geometry::from_triangle_faces_with_vertices_and_normals(
            faces.clone(),
            vertices.clone(),
            normals.clone(),
        );

        assert!(geometry_without_orphans.has_no_orphan_normals());
    }

    #[test]
    fn test_geometry_unoriented_edges_iter() {
        let (faces, vertices, normals) = quad_with_normals();
        let geometry = Geometry::from_triangle_faces_with_vertices_and_normals(
            faces.clone(),
            vertices.clone(),
            normals.clone(),
        );
        let unoriented_edges_correct = vec![
            UnorientedEdge(OrientedEdge::new(0, 1)),
            UnorientedEdge(OrientedEdge::new(1, 2)),
            UnorientedEdge(OrientedEdge::new(2, 0)),
            UnorientedEdge(OrientedEdge::new(2, 3)),
            UnorientedEdge(OrientedEdge::new(3, 0)),
            UnorientedEdge(OrientedEdge::new(0, 2)),
        ];
        let unoriented_edges_to_check: Vec<UnorientedEdge> =
            geometry.unoriented_edges_iter().collect();

        assert!(unoriented_edges_correct
            .iter()
            .all(|u_e| unoriented_edges_to_check.iter().any(|e| e == u_e)));

        let len_1 = unoriented_edges_to_check.len();
        let len_2 = unoriented_edges_correct.len();
        assert_eq!(len_1, len_2);
    }

    #[test]
    fn test_geometry_oriented_edges_iter() {
        let (faces, vertices, normals) = quad_with_normals();
        let geometry = Geometry::from_triangle_faces_with_vertices_and_normals(
            faces.clone(),
            vertices.clone(),
            normals.clone(),
        );

        let oriented_edges_correct = vec![
            OrientedEdge::new(0, 1),
            OrientedEdge::new(1, 2),
            OrientedEdge::new(2, 0),
            OrientedEdge::new(2, 3),
            OrientedEdge::new(3, 0),
            OrientedEdge::new(0, 2),
        ];
        let oriented_edges_to_check: Vec<OrientedEdge> = geometry.oriented_edges_iter().collect();

        assert!(oriented_edges_correct
            .iter()
            .all(|o_e| oriented_edges_to_check.iter().any(|e| e == o_e)));

        let len_1 = oriented_edges_to_check.len();
        let len_2 = oriented_edges_correct.len();

        assert_eq!(len_1, len_2);
    }

    #[test]
    fn test_has_no_orphan_normals_returns_false_if_there_are_none() {
        let (faces, vertices, normals) = quad_with_normals();
        let extra_normal = vec![n(0.0, 0.0, 0.0)];
        let normals_extended = [&normals[..], &extra_normal[..]].concat();

        let geometry_with_orphans = Geometry::from_triangle_faces_with_vertices_and_normals(
            faces.clone(),
            vertices.clone(),
            normals_extended.clone(),
        );

        assert!(!geometry_with_orphans.has_no_orphan_normals());
    }

    #[test]
    fn test_remove_orphan_vertices() {
        let (faces, vertices) = quad();
        let extra_vertex = vec![v(0.0, 0.0, 0.0, [0.0, 0.0, 0.0], 1.0)];
        let vertices_extended = [&extra_vertex[..], &vertices[..]].concat();
        let faces_renumbered_to_match_extend_vertices: Vec<_> =
            faces.iter().map(|f| (f.0 + 1, f.1 + 1, f.2 + 1)).collect();

        let faces_length = &faces.len();

        let (faces_purged, vertices_purged) =
            remove_orphan_vertices(faces_renumbered_to_match_extend_vertices, vertices_extended);

        let faces_purged_length = &faces_purged.len();

        assert_eq!(faces_length, faces_purged_length);
        assert_eq!(vertices_purged, vertices);
    }

    #[test]
    fn test_remove_orphan_normals() {
        let (faces, _vertices, normals) = quad_with_normals();
        let extra_normal = vec![n(0.0, 0.0, 0.0)];
        let normals_extended = [&extra_normal[..], &normals[..]].concat();

        let faces_renumbered_to_match_extend_normals: Vec<_> = faces
            .iter()
            .map(|f| {
                TriangleFace::new_separate(
                    f.vertices.0,
                    f.vertices.1,
                    f.vertices.2,
                    f.normals.0 + 1,
                    f.normals.1 + 1,
                    f.normals.2 + 1,
                )
            })
            .collect();

        let faces_length = &faces.len();

        let (faces_purged, normals_purged) =
            remove_orphan_normals(faces_renumbered_to_match_extend_normals, normals_extended);

        let faces_purged_length = &faces_purged.len();

        assert_eq!(faces_length, faces_purged_length);
        assert_eq!(normals_purged, normals);
    }

    #[test]
    fn test_remove_orphan_vertices_and_normals() {
        let (faces, vertices, normals) = quad_with_normals();

        let extra_vertex = vec![v(0.0, 0.0, 0.0, [0.0, 0.0, 0.0], 1.0)];
        let vertices_extended = [&extra_vertex[..], &vertices[..]].concat();

        let extra_normal = vec![n(0.0, 0.0, 0.0)];
        let normals_extended = [&extra_normal[..], &normals[..]].concat();

        let faces_renumbered_to_match_extend_data: Vec<_> = faces
            .iter()
            .map(|f| {
                TriangleFace::new_separate(
                    f.vertices.0 + 1,
                    f.vertices.1 + 1,
                    f.vertices.2 + 1,
                    f.normals.0 + 1,
                    f.normals.1 + 1,
                    f.normals.2 + 1,
                )
            })
            .collect();

        let faces_length = &faces.len();

        let (faces_purged, vertices_purged, normals_purged) = remove_orphan_vertices_and_normals(
            faces_renumbered_to_match_extend_data,
            vertices_extended,
            normals_extended,
        );

        let faces_purged_length = &faces_purged.len();

        assert_eq!(faces_length, faces_purged_length);
        assert_eq!(vertices_purged, vertices);
        assert_eq!(normals_purged, normals);
    }

    #[test]
    fn test_geometry_from_triangle_faces_with_vertices_and_computed_normals_remove_orphans() {
        let (faces, vertices) = quad();
        let extra_vertex = vec![v(0.0, 0.0, 0.0, [0.0, 0.0, 0.0], 1.0)];
        let vertices_extended = [&extra_vertex[..], &vertices[..]].concat();
        let faces_renumbered_to_match_extend_vertices: Vec<_> =
            faces.iter().map(|f| (f.0 + 1, f.1 + 1, f.2 + 1)).collect();

        let geometry =
            Geometry::from_triangle_faces_with_vertices_and_computed_normals_remove_orphans(
                faces_renumbered_to_match_extend_vertices,
                vertices_extended,
                NormalStrategy::Sharp,
            );

        assert!(geometry.has_no_orphan_vertices());
    }

    #[test]
    fn test_geometry_from_triangle_faces_with_vertices_and_normals_remove_orphans() {
        let (faces, vertices, normals) = quad_with_normals();
        let extra_vertex = vec![v(0.0, 0.0, 0.0, [0.0, 0.0, 0.0], 1.0)];
        let vertices_extended = [&extra_vertex[..], &vertices[..]].concat();
        let extra_normal = vec![n(0.0, 0.0, 0.0)];
        let normals_extended = [&extra_normal[..], &normals[..]].concat();

        let faces_renumbered_to_match_extend_vertices_and_normals: Vec<_> = faces
            .iter()
            .map(|f| {
                TriangleFace::new_separate(
                    f.vertices.0 + 1,
                    f.vertices.1 + 1,
                    f.vertices.2 + 1,
                    f.normals.0 + 1,
                    f.normals.1 + 1,
                    f.normals.2 + 1,
                )
            })
            .collect();

        let geometry = Geometry::from_triangle_faces_with_vertices_and_normals_remove_orphans(
            faces_renumbered_to_match_extend_vertices_and_normals,
            vertices_extended,
            normals_extended,
        );

        assert!(geometry.has_no_orphan_vertices());
    }
}
