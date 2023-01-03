use std::sync::Arc;
use rand::RngCore;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SceneId(u64);

impl SceneId {
    pub fn new() -> Self {
        Self(rand::thread_rng().next_u64())
    }
}

pub trait Scene: Send + Sync {
    fn get_scene_id(&self) -> SceneId;

    /// Signals that all necessary scene updates have completed and the current state is ready for a
    /// draw. This function may take a (comparatively) long time to return. Once the function
    /// returns the scene is ready to be modified for the next update.
    ///
    /// It is undefined if any modifications to the scene during a in progress update will be part
    /// of the current update or the next update. However the modifications must be visible in the
    /// next call to update after said modifications have completed (i.e. no modifications will ever
    /// be ignored or otherwise corrupted).
    fn update(&self);

    fn create_transform_component(&self) -> Arc<dyn TransformComponent>;

    fn create_camera_component(&self) -> Arc<dyn CameraComponent>;
}

impl PartialEq for dyn Scene {
    fn eq(&self, other: &Self) -> bool {
        self.get_scene_id() == other.get_scene_id()
    }
}
impl Eq for dyn Scene {
}

pub trait SceneComponent: Send + Sync {
    /// Returns the [`Scene`] this component is a part of.
    fn get_scene(&self) -> Arc<dyn Scene>;

    /// Sets the parent of this component in the scene graph. If `parent` is [`None`] the parent
    /// will be set to the scene root.
    ///
    /// # Safety
    /// `parent` must be part of the same [`Scene`] as this component otherwise this function will
    /// panic.
    fn set_parent(&self, parent: Option<Arc<dyn SceneComponent>>);
}

pub trait TransformComponent: SceneComponent {
    fn set_translation(&self, translation: ());

    fn set_rotation(&self, rotation: ());

    fn set_scale(&self, scale: ());
}

pub trait CameraComponent: SceneComponent {
}