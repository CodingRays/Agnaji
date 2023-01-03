use std::sync::Arc;
use crate::scene::CameraComponent;

/// A output target defines the ultimate destination of rendered images. To render a output target
/// uses a camera component which defines the scene and draw settings to be used for rendering. Any
/// rendering is ultimately initiated by a output target.
pub trait OutputTarget: Send {

    /// Configures the camera that should be used for rendering.
    ///
    /// If `camera` is [`None`] the camera is cleared.
    fn set_source_camera(&self, camera: Option<Arc<dyn CameraComponent>>);
}