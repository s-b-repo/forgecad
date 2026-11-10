//! Semantic primitive scenes and deterministic keyframe evaluation.
//!
//! Keyframes retain translation, Euler rotation and positive scale separately.
//! Translation and scale interpolate linearly; each Euler angle follows its
//! shortest arc (an exact 180 degree tie takes the negative direction). Meshes
//! are derived at a requested time without changing the stored scene. There is
//! a checked transform-parent hierarchy, without rigs, dynamics or vendor formats.

use crate::{
    Solid, SolidKind,
    mesh::{TessellationOptions, TriangleMesh, tessellate_solid},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

pub const MAX_SCENE_OBJECTS: usize = 1_000;
pub const MAX_SCENE_KEYFRAMES: usize = 10_000;
pub const MAX_SCENE_FRAMES: usize = 100_000;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Keyframe {
    pub time: f64,
    pub translation: [f64; 3],
    /// Rotation in degrees, applied around the local origin in X, Y, Z order.
    pub rotation_deg: [f64; 3],
    pub scale: [f64; 3],
}

impl Default for Keyframe {
    fn default() -> Self {
        Self {
            time: 0.0,
            translation: [0.0; 3],
            rotation_deg: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}

impl Keyframe {
    pub fn validate(&self) -> io::Result<()> {
        if !self.time.is_finite()
            || self.time < 0.0
            || !self
                .translation
                .iter()
                .chain(self.rotation_deg.iter())
                .all(|value| value.is_finite())
            || !self
                .scale
                .iter()
                .all(|value| value.is_finite() && *value > 0.0)
        {
            return Err(invalid(
                "Keyframes require nonnegative finite time, finite translation/rotation, and positive finite scale",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SceneObject {
    pub id: u64,
    /// Keys are local to this parent; None means world space.
    #[serde(default)]
    pub parent_id: Option<u64>,
    pub name: String,
    pub shape: SolidKind,
    /// Local X/Y/Z dimensions. Sphere dimensions describe an ellipsoid.
    pub size: [f64; 3],
    pub color: [u8; 3],
    pub keyframes: Vec<Keyframe>,
}

impl SceneObject {
    pub fn new(id: u64, name: impl Into<String>, shape: SolidKind) -> Self {
        Self {
            id,
            parent_id: None,
            name: name.into(),
            shape,
            size: match shape {
                SolidKind::Box => [80.0, 60.0, 48.0],
                SolidKind::Cylinder => [60.0, 60.0, 80.0],
                SolidKind::Sphere => [60.0; 3],
            },
            color: [105, 190, 255],
            keyframes: vec![Keyframe::default()],
        }
    }

    pub fn validate(&self) -> io::Result<()> {
        if self.id == 0 || self.name.trim().is_empty() || self.name.contains(['\r', '\n', '\0']) {
            return Err(invalid(
                "Scene objects require a nonzero ID and a nonempty single-line name",
            ));
        }
        if !self
            .size
            .iter()
            .all(|value| value.is_finite() && *value > 0.0)
        {
            return Err(invalid(
                "Scene object dimensions must be finite and positive",
            ));
        }
        if self.keyframes.is_empty() || self.keyframes.len() > MAX_SCENE_KEYFRAMES {
            return Err(invalid("A scene object requires 1 to 10,000 keyframes"));
        }
        let mut previous = None;
        for keyframe in &self.keyframes {
            keyframe.validate()?;
            if previous.is_some_and(|time| time >= keyframe.time) {
                return Err(invalid(
                    "Scene keyframes must have distinct, strictly increasing times",
                ));
            }
            previous = Some(keyframe.time);
            // A conservative transformed radius detects overflow before mesh
            // generation without tessellating every frame during validation.
            let radius = self
                .size
                .iter()
                .zip(keyframe.scale)
                .map(|(size, scale)| size / 2.0 * scale)
                .sum::<f64>();
            if !radius.is_finite()
                || keyframe
                    .translation
                    .iter()
                    .any(|value| !(value - radius).is_finite() || !(value + radius).is_finite())
            {
                return Err(invalid("Scene transform extents exceed numeric range"));
            }
        }
        Ok(())
    }

    /// Insert a keyframe in order or replace the key at the exact same time.
    /// Invalid edits leave the complete object unchanged. The containing Scene
    /// must additionally validate that this time lies within its duration.
    pub fn set_keyframe(&mut self, keyframe: Keyframe) -> io::Result<()> {
        keyframe.validate()?;
        let mut candidate = self.clone();
        match candidate
            .keyframes
            .binary_search_by(|item| item.time.total_cmp(&keyframe.time))
        {
            Ok(index) => candidate.keyframes[index] = keyframe,
            Err(index) => candidate.keyframes.insert(index, keyframe),
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    /// Evaluate a finite time, clamped to the first and last authored keys.
    pub fn pose_at(&self, time: f64) -> io::Result<Keyframe> {
        self.validate()?;
        if !time.is_finite() {
            return Err(invalid("Scene evaluation time must be finite"));
        }
        let first = self.keyframes.first().expect("validated keyframes");
        let last = self.keyframes.last().expect("validated keyframes");
        if time <= first.time {
            return Ok(first.clone());
        }
        if time >= last.time {
            return Ok(last.clone());
        }
        let right = self.keyframes.partition_point(|item| item.time < time);
        if self.keyframes[right].time == time {
            return Ok(self.keyframes[right].clone());
        }
        let a = &self.keyframes[right - 1];
        let b = &self.keyframes[right];
        let factor = (time - a.time) / (b.time - a.time);
        let pose = Keyframe {
            time,
            translation: std::array::from_fn(|axis| {
                lerp(a.translation[axis], b.translation[axis], factor)
            }),
            rotation_deg: std::array::from_fn(|axis| {
                interpolate_angle(a.rotation_deg[axis], b.rotation_deg[axis], factor)
            }),
            scale: std::array::from_fn(|axis| lerp(a.scale[axis], b.scale[axis], factor)),
        };
        pose.validate()?;
        Ok(pose)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub objects: Vec<SceneObject>,
    pub duration: f64,
    pub fps: u32,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            objects: Vec::new(),
            duration: 5.0,
            fps: 24,
        }
    }
}

pub type SceneMesh = (u64, String, [u8; 3], TriangleMesh);

impl Scene {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn validate(&self) -> io::Result<()> {
        frame_count(self.duration, self.fps)?;
        if self.objects.len() > MAX_SCENE_OBJECTS {
            return Err(invalid("Scene exceeds the 1,000 object limit"));
        }
        let mut ids = BTreeSet::new();
        let mut keys = 0usize;
        for object in &self.objects {
            object.validate()?;
            if !ids.insert(object.id) {
                return Err(invalid("Scene object IDs must be unique"));
            }
            keys = keys
                .checked_add(object.keyframes.len())
                .ok_or_else(|| invalid("Scene keyframe count overflow"))?;
            if keys > MAX_SCENE_KEYFRAMES {
                return Err(invalid("Scene exceeds the 10,000 total keyframe limit"));
            }
            if object.keyframes.last().expect("validated keyframes").time > self.duration {
                return Err(invalid(
                    "Scene keyframes must lie within the scene duration",
                ));
            }
        }
        let objects: BTreeMap<_, _> = self.objects.iter().map(|o| (o.id, o)).collect();
        for object in &self.objects {
            let mut visited = BTreeSet::from([object.id]);
            let mut parent = object.parent_id;
            while let Some(id) = parent {
                if !visited.insert(id) {
                    return Err(invalid("Scene parenting cannot contain cycles"));
                }
                if visited.len() > 64 {
                    return Err(invalid("Scene hierarchy exceeds 64 levels"));
                }
                parent = objects
                    .get(&id)
                    .ok_or_else(|| invalid("Scene parent refers to a missing object"))?
                    .parent_id;
            }
        }
        Ok(())
    }

    /// Change the parent without rewriting local animation. Invalid references
    /// or cycles leave the whole scene unchanged.
    pub fn set_parent(&mut self, id: u64, parent_id: Option<u64>) -> io::Result<()> {
        let mut candidate = self.clone();
        candidate
            .objects
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or_else(|| invalid("Scene object does not exist"))?
            .parent_id = parent_id;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    /// Delete a node and all descendants as one edit.
    pub fn remove_subtree(&mut self, id: u64) -> io::Result<Vec<u64>> {
        self.validate()?;
        if !self.objects.iter().any(|o| o.id == id) {
            return Err(invalid("Scene object does not exist"));
        }
        let mut removed = BTreeSet::from([id]);
        loop {
            let before = removed.len();
            for object in &self.objects {
                if object
                    .parent_id
                    .is_some_and(|parent| removed.contains(&parent))
                {
                    removed.insert(object.id);
                }
            }
            if removed.len() == before {
                break;
            }
        }
        self.objects.retain(|o| !removed.contains(&o.id));
        Ok(removed.into_iter().collect())
    }

    /// Derive colored object meshes for a finite time clamped to scene duration.
    /// Local scale precedes X/Y/Z rotations, followed by world translation.
    /// Stored objects, keyframes and IDs remain unchanged.
    pub fn meshes_at(&self, time: f64) -> io::Result<Vec<SceneMesh>> {
        self.validate()?;
        if !time.is_finite() {
            return Err(invalid("Scene evaluation time must be finite"));
        }
        let time = time.clamp(0.0, self.duration);
        let lookup: BTreeMap<_, _> = self.objects.iter().map(|o| (o.id, o)).collect();
        let mut matrices = BTreeMap::new();
        let mut output = Vec::with_capacity(self.objects.len());
        for object in &self.objects {
            let matrix = world_matrix(object.id, time, &lookup, &mut matrices)?;
            let solid = Solid {
                id: object.id,
                name: object.name.clone(),
                kind: object.shape,
                center: [0.0; 3],
                size: object.size,
            };
            let mut mesh = tessellate_solid(&solid, TessellationOptions::default())?;
            for vertex in &mut mesh.vertices {
                let v = *vertex;
                *vertex = std::array::from_fn(|axis| {
                    matrix[axis][0] * v[0]
                        + matrix[axis][1] * v[1]
                        + matrix[axis][2] * v[2]
                        + matrix[axis][3]
                });
            }
            mesh.validate()?;
            output.push((object.id, object.name.clone(), object.color, mesh));
        }
        Ok(output)
    }

    /// Sample at 1/fps, including zero and the exact final duration. A
    /// fractional last frame is shorter so the final authored pose is retained.
    pub fn frame_times(&self) -> io::Result<Vec<f64>> {
        self.validate()?;
        let count = frame_count(self.duration, self.fps)?;
        Ok((0..count)
            .map(|index| (index as f64 / self.fps as f64).min(self.duration))
            .collect())
    }

    pub fn example() -> Self {
        let mut box_object = SceneObject::new(1, "Animated box", SolidKind::Box);
        box_object.size = [50.0; 3];
        box_object.keyframes = vec![
            Keyframe {
                translation: [-80.0, 0.0, 25.0],
                ..Keyframe::default()
            },
            Keyframe {
                time: 2.5,
                translation: [0.0, 40.0, 60.0],
                rotation_deg: [0.0, 0.0, 90.0],
                ..Keyframe::default()
            },
            Keyframe {
                time: 5.0,
                translation: [80.0, 0.0, 25.0],
                rotation_deg: [0.0, 0.0, 180.0],
                ..Keyframe::default()
            },
        ];
        let mut cylinder = SceneObject::new(2, "Animated cylinder", SolidKind::Cylinder);
        cylinder.size = [35.0, 35.0, 70.0];
        cylinder.color = [255, 174, 92];
        cylinder.keyframes = vec![
            Keyframe {
                translation: [60.0, 0.0, 35.0],
                ..Keyframe::default()
            },
            Keyframe {
                time: 2.5,
                translation: [0.0, -40.0, 65.0],
                rotation_deg: [0.0, 90.0, 0.0],
                ..Keyframe::default()
            },
            Keyframe {
                time: 5.0,
                translation: [-60.0, 0.0, 35.0],
                rotation_deg: [0.0, 180.0, 0.0],
                ..Keyframe::default()
            },
        ];
        Self {
            objects: vec![box_object, cylinder],
            ..Self::default()
        }
    }
}

type Matrix = [[f64; 4]; 3];
fn world_matrix(
    id: u64,
    time: f64,
    objects: &BTreeMap<u64, &SceneObject>,
    cache: &mut BTreeMap<u64, Matrix>,
) -> io::Result<Matrix> {
    if let Some(matrix) = cache.get(&id) {
        return Ok(*matrix);
    }
    let object = objects[&id];
    let pose = object.pose_at(time)?;
    let trig = pose
        .rotation_deg
        .map(|v| v.rem_euclid(360.0).to_radians().sin_cos());
    // Transform basis vectors without translation to avoid cancellation at a
    // large origin. Affine composition retains parent-induced shear.
    let mut local_pose = pose.clone();
    local_pose.translation = [0.0; 3];
    let basis: [[f64; 3]; 3] = std::array::from_fn(|axis| {
        let mut v = [0.0; 3];
        v[axis] = 1.0;
        transform(v, &local_pose, trig)
    });
    let mut matrix: Matrix =
        std::array::from_fn(|r| [basis[0][r], basis[1][r], basis[2][r], pose.translation[r]]);
    if let Some(parent) = object.parent_id {
        let p = world_matrix(parent, time, objects, cache)?;
        let child = matrix;
        matrix = std::array::from_fn(|r| {
            std::array::from_fn(|c| {
                (0..3).map(|k| p[r][k] * child[k][c]).sum::<f64>()
                    + if c == 3 { p[r][3] } else { 0.0 }
            })
        });
    }
    if matrix.iter().flatten().any(|v| !v.is_finite()) {
        return Err(invalid("Scene hierarchy transform exceeds numeric range"));
    }
    cache.insert(id, matrix);
    Ok(matrix)
}

fn frame_count(duration: f64, fps: u32) -> io::Result<usize> {
    if !duration.is_finite() || duration <= 0.0 || duration > 3600.0 || !(1..=240).contains(&fps) {
        return Err(invalid(
            "Scene duration must be positive and at most 3,600 seconds; fps must be 1..240",
        ));
    }
    let count = (duration * fps as f64).ceil() + 1.0;
    if count > MAX_SCENE_FRAMES as f64 {
        return Err(invalid("Scene exceeds the 100,000 frame limit"));
    }
    Ok(count as usize)
}

fn lerp(a: f64, b: f64, factor: f64) -> f64 {
    // Weighted endpoints avoid overflowing b-a for opposite large values.
    a * (1.0 - factor) + b * factor
}

fn interpolate_angle(a: f64, b: f64, factor: f64) -> f64 {
    // Normalize before subtracting: finite, very large degree values can have
    // an overflowing difference even though their rotation is well defined.
    let a = a.rem_euclid(360.0);
    let delta = (b.rem_euclid(360.0) - a + 180.0).rem_euclid(360.0) - 180.0;
    (a + delta * factor).rem_euclid(360.0)
}

fn transform(vertex: [f64; 3], pose: &Keyframe, trig: [(f64, f64); 3]) -> [f64; 3] {
    let [x, y, z] = std::array::from_fn(|axis| vertex[axis] * pose.scale[axis]);
    let [(sx, cx), (sy, cy), (sz, cz)] = trig;
    let (x, y, z) = (x, y * cx - z * sx, y * sx + z * cx);
    let (x, y, z) = (x * cy + z * sy, y, -x * sy + z * cy);
    [
        x * cz - y * sz + pose.translation[0],
        x * sz + y * cz + pose.translation[1],
        z + pose.translation[2],
    ]
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-8, "{a} != {b}");
    }

    #[test]
    fn keyframes_interpolate_channels_shortest_angles_and_clamp_endpoints() {
        let mut object = SceneObject::new(1, "Box", SolidKind::Box);
        object.keyframes = vec![
            Keyframe {
                translation: [0.0, -2.0, 5.0],
                rotation_deg: [350.0, 170.0, 0.0],
                ..Keyframe::default()
            },
            Keyframe {
                time: 4.0,
                translation: [20.0, 6.0, 9.0],
                rotation_deg: [10.0, -170.0, 180.0],
                scale: [3.0, 5.0, 7.0],
            },
        ];
        let middle = object.pose_at(2.0).unwrap();
        assert_eq!(middle.translation, [10.0, 2.0, 7.0]);
        assert_eq!(middle.scale, [2.0, 3.0, 4.0]);
        assert_eq!(middle.rotation_deg, [0.0, 180.0, 270.0]);
        assert_eq!(object.pose_at(-10.0).unwrap(), object.keyframes[0]);
        assert_eq!(object.pose_at(100.0).unwrap(), object.keyframes[1]);
        assert!(object.pose_at(f64::NAN).is_err());
        assert_eq!(object.pose_at(4.0).unwrap(), object.keyframes[1]);
    }

    #[test]
    fn mesh_vertices_obey_local_scale_xyz_rotation_then_world_translation() {
        let mut object = SceneObject::new(1, "Transformed box", SolidKind::Box);
        object.size = [2.0, 4.0, 6.0];
        object.keyframes[0] = Keyframe {
            translation: [10.0, 20.0, 30.0],
            rotation_deg: [0.0, 0.0, 90.0],
            scale: [2.0, 3.0, 4.0],
            ..Keyframe::default()
        };
        let scene = Scene {
            objects: vec![object],
            ..Scene::default()
        };
        let before = scene.clone();
        let meshes = scene.meshes_at(0.0).unwrap();
        let (min, max) = meshes[0].3.bounds().unwrap();
        for (actual, expected) in min.into_iter().zip([4.0, 18.0, 18.0]) {
            near(actual, expected);
        }
        for (actual, expected) in max.into_iter().zip([16.0, 22.0, 42.0]) {
            near(actual, expected);
        }
        near(meshes[0].3.signed_volume().unwrap(), 1152.0);
        assert_eq!(scene, before);
        let pose = Keyframe {
            translation: [10.0, 20.0, 30.0],
            rotation_deg: [90.0; 3],
            ..Keyframe::default()
        };
        let trig = pose.rotation_deg.map(|value| value.to_radians().sin_cos());
        for (actual, expected) in transform([-1.0, -2.0, -3.0], &pose, trig)
            .into_iter()
            .zip([7.0, 18.0, 31.0])
        {
            near(actual, expected);
        }
    }

    #[test]
    fn evaluated_meshes_move_over_time_and_keep_object_identity_color_and_shape() {
        let scene = Scene::example();
        let first = scene.meshes_at(-1.0).unwrap();
        let middle = scene.meshes_at(2.5).unwrap();
        let last = scene.meshes_at(500.0).unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].0, 1);
        assert_eq!(first[0].2, [105, 190, 255]);
        assert_eq!(first[1].2, [255, 174, 92]);
        near(first[0].3.bounds().unwrap().0[0], -105.0);
        near(middle[0].3.bounds().unwrap().0[0], -25.0);
        near(last[0].3.bounds().unwrap().0[0], 55.0);
        assert!(first[1].3.triangles.len() > first[0].3.triangles.len());
        assert!(scene.meshes_at(f64::INFINITY).is_err());
    }

    #[test]
    fn sorted_keyframe_upsert_is_atomic_on_invalid_data() {
        let mut object = SceneObject::new(1, "Part", SolidKind::Cylinder);
        object
            .set_keyframe(Keyframe {
                time: 4.0,
                ..Keyframe::default()
            })
            .unwrap();
        object
            .set_keyframe(Keyframe {
                time: 2.0,
                translation: [1.0; 3],
                ..Keyframe::default()
            })
            .unwrap();
        object
            .set_keyframe(Keyframe {
                time: 2.0,
                translation: [2.0; 3],
                ..Keyframe::default()
            })
            .unwrap();
        assert_eq!(
            object
                .keyframes
                .iter()
                .map(|key| key.time)
                .collect::<Vec<_>>(),
            vec![0.0, 2.0, 4.0]
        );
        assert_eq!(object.keyframes[1].translation, [2.0; 3]);
        let before = object.clone();
        for frame in [
            Keyframe {
                time: -1.0,
                ..Keyframe::default()
            },
            Keyframe {
                scale: [0.0, 1.0, 1.0],
                ..Keyframe::default()
            },
            Keyframe {
                rotation_deg: [f64::NAN; 3],
                ..Keyframe::default()
            },
        ] {
            assert!(object.set_keyframe(frame).is_err());
            assert_eq!(object, before);
        }
    }

    #[test]
    fn scene_rejects_duplicate_ids_unsorted_keys_and_out_of_duration_keys() {
        let object = SceneObject::new(1, "Box", SolidKind::Box);
        let mut scene = Scene {
            objects: vec![object.clone(), object],
            ..Scene::default()
        };
        assert!(scene.validate().is_err());
        scene.objects.pop();
        scene.objects[0].keyframes.push(Keyframe::default());
        assert!(scene.validate().is_err());
        scene.objects[0].keyframes[1].time = 6.0;
        assert!(scene.validate().is_err());
        scene.objects[0].keyframes[1].time = 4.0;
        scene.validate().unwrap();
        scene.objects[0].id = 0;
        assert!(scene.validate().is_err());
    }

    #[test]
    fn frame_sampling_is_bounded_and_includes_exact_final_pose() {
        assert_eq!(
            Scene {
                duration: 1.0,
                fps: 2,
                ..Scene::default()
            }
            .frame_times()
            .unwrap(),
            vec![0.0, 0.5, 1.0]
        );
        assert_eq!(
            Scene {
                duration: 1.1,
                fps: 2,
                ..Scene::default()
            }
            .frame_times()
            .unwrap(),
            vec![0.0, 0.5, 1.0, 1.1]
        );
        for (duration, fps) in [
            (0.0, 24),
            (-1.0, 24),
            (f64::NAN, 24),
            (3601.0, 24),
            (5.0, 0),
            (5.0, 241),
            (3600.0, 240),
        ] {
            assert!(
                Scene {
                    duration,
                    fps,
                    ..Scene::default()
                }
                .validate()
                .is_err()
            );
        }
        let scene = Scene::default();
        assert!(scene.is_empty());
        assert!(scene.meshes_at(0.0).unwrap().is_empty());
        assert_eq!(scene.frame_times().unwrap().len(), 121);
    }

    #[test]
    fn serde_preserves_animation_and_rejects_unknown_scene_fields() {
        let scene = Scene::example();
        let source = serde_json::to_string(&scene).unwrap();
        let restored: Scene = serde_json::from_str(&source).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored, scene);
        assert!(
            serde_json::from_str::<Scene>(
                "{\"objects\":[],\"duration\":5,\"fps\":24,\"unrecognized\":true}"
            )
            .is_err()
        );
    }

    #[test]
    fn overflowing_transforms_fail_without_mutating_scene() {
        let mut object = SceneObject::new(1, "Huge", SolidKind::Box);
        object.keyframes[0].scale = [f64::MAX; 3];
        assert!(object.validate().is_err());
        let mut scene = Scene::example();
        scene.objects[0].keyframes[0].translation = [1e100; 3];
        let before = scene.clone();
        assert!(scene.meshes_at(0.0).is_err());
        assert_eq!(scene, before);
    }
}
