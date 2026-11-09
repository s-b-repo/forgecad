use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum SolidKind { Box, Cylinder, Sphere }
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Solid { pub id:u64, pub name:String, pub kind:SolidKind, pub center:[f64;3], pub size:[f64;3] }
#[path = "../../../crates/forge-core/src/mesh.rs"]
pub mod mesh;
