use std::any::Any;
use std::sync::Arc;
use crate::scene::{Scene, SceneId, SceneUpdate};

pub struct VulkanScene {

}

impl Scene for VulkanScene {
    fn get_scene_id(&self) -> SceneId {
        todo!()
    }

    fn begin_update(&self) -> Result<Box<dyn SceneUpdate>, ()> {
        todo!()
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync + 'static) {
        todo!()
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync + 'static> {
        todo!()
    }
}