use std::sync::Arc;

use crate::scene::Scene;

pub mod vulkan;
pub mod wsi;
pub mod debug;
pub mod output;
pub mod scene;
pub mod utils;
pub mod prelude;

pub trait Agnaji: Send + Sync {
    fn create_scene(&self) -> Arc<dyn Scene>;
}