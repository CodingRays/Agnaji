use std::any::Any;
use std::fmt::Formatter;
use std::num::NonZeroU64;
use std::sync::Arc;
use crate::utils::define_counting_id_type;

define_counting_id_type!(SceneId);
define_counting_id_type!(ComponentId);

impl std::fmt::Debug for SceneId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SceneId").field(&self.value).finish()
    }
}

/// A scene is a collection of components defining a world to be rendered. [`SceneComponent`]s are
/// organized into a hierarchy which is called the scene graph.
///
/// The scene graph is purely to define a transformation hierarchy for components. It should not be
/// used to organize components logically. For example in some cases components which are direct
/// children of the scene root can be optimized since they cannot move.
///
/// All modifications to the scene happen during a scene update. To start a scene update call
/// [`Scene::begin_update`]. The returned [`SceneUpdate`] can then be used to modify the scene by
/// either creating new components or modifying existing components. When the [`SceneUpdate`]
/// instance is dropped the modified state gets submitted and can be used for rendering. Since
/// rendering is asynchronous this prevents rendering of a scene that is in a incomplete state. Only
/// 1 scene update may happen concurrently.
pub trait Scene: Send + Sync {
    fn get_scene_id(&self) -> SceneId;

    /// Starts a new scene update. The scene update is complete once the returned [`SceneUpdate`]
    /// instance is dropped.
    fn begin_update(&self) -> Result<Box<dyn SceneUpdate>, ()>;

    fn as_any(&self) -> &(dyn Any + Send + Sync + 'static);

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync + 'static>;
}

impl PartialEq for dyn Scene {
    fn eq(&self, other: &Self) -> bool {
        self.get_scene_id() == other.get_scene_id()
    }
}
impl Eq for dyn Scene {
}

/// Trait that is used to modify a [`Scene`]. Once a instance of this trait is dropped the update is
/// considered complete and the state of the scene can be used for rendering. After drop returns the
/// scene is ready to begin a new update.
///
/// **Performance Note:** Because the update is submitted on drop. Dropping this struct may block
/// for a long time. A [`SceneUpdate`] is usually provided in boxed form to make it easy to control
/// when a drop happens.
pub trait SceneUpdate: Send + Sync {
    fn get_scene_id(&self) -> SceneId;

    // fn create_transform_component(&self) -> Arc<dyn TransformComponent>;

    fn create_camera_component(&self) -> Arc<dyn CameraComponent>;

    fn as_any(&self) -> &(dyn Any + Send + Sync + 'static);

    fn as_any_box(self: Box<Self>) -> Box<dyn Any + Send + Sync + 'static>;
}

/// A component that is part of a [`Scene`].
///
/// A [`SceneComponent`] always keeps its parent alive but not its children. Thus typically calling
/// code should not drop any reference to a component it still needs.
pub trait SceneComponent: Send + Sync {
    fn get_component_id(&self) -> ComponentId;

    /// Returns the [`Scene`] this component is a part of.
    fn get_scene(&self) -> Arc<dyn Scene>;

    /*
    /// Sets the parent of this component in the scene graph. If `parent` is [`None`] the parent
    /// will be set to the scene root.
    ///
    /// # Safety
    /// `parent` must be part of the same [`Scene`] as this component otherwise this function will
    /// panic.
    fn set_parent(&self, update: &dyn SceneUpdate, parent: Option<Arc<dyn TransformComponent>>);*/

    /// Explicitly destroys this component removing it from the scene graph. Future calls to any
    /// function will be behave
    fn destroy(&self, update: &dyn SceneUpdate);

    fn as_any(&self) -> &(dyn Any + Send + Sync + 'static);

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync + 'static>;
}

/*
pub trait TransformComponent: SceneComponent {
    fn set_translation(&self, update: &dyn SceneUpdate, translation: ());

    fn set_rotation(&self, update: &dyn SceneUpdate, rotation: ());

    fn set_scale(&self, update: &dyn SceneUpdate, scale: ());
}*/

pub trait CameraComponent: SceneComponent {
}