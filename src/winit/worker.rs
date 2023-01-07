use std::collections::HashMap;
use std::panic::{catch_unwind, UnwindSafe};
use std::sync::{Arc, Condvar, Mutex, Weak};
use winit::dpi::PhysicalSize;
use winit::error::OsError;
use winit::event::{Event, WindowEvent};
use winit::event::VirtualKeyCode::M;
use winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder};
use winit::window::{WindowBuilder, WindowId};
use crate::prelude::Vec2u32;
use crate::winit::{AgnajiEvent, DEFAULT_LOG_TARGET, WinitBackend};
use crate::winit::window::Window;

const EVENT_LOOP_LOG_TARGET: &'static str = "agnaji::winit::EventLoop";

pub(in crate::winit) fn run<F>(post_init: F) where F: FnOnce(Arc<WinitBackend>) + Send + UnwindSafe + 'static {
    let event_loop: EventLoop<AgnajiEvent> = EventLoopBuilder::with_user_event().build();

    let backend = Arc::new(WinitBackend::new(
        event_loop.create_proxy()
    ));

    let backend_clone = backend.clone();
    let mut engine_thread = Some(std::thread::spawn(move || {
        log::debug!(target: EVENT_LOOP_LOG_TARGET, "Starting main application thread");
        let backend = backend_clone.clone();
        if let Err(_) = catch_unwind(move || {
            post_init(backend_clone)
        }) {
            log::error!(target: EVENT_LOOP_LOG_TARGET, "Main application thread panicked. Quitting winit backend");
        };
        backend.quit();
    }));

    let mut window_table: HashMap<WindowId, Weak<Window>> = HashMap::new();

    log::debug!(target: EVENT_LOOP_LOG_TARGET, "Starting winit event loop");
    event_loop.run(move |event, window_target, control_flow| {
        *control_flow = ControlFlow::Wait;

        log::trace!(target: EVENT_LOOP_LOG_TARGET, "Processing winit event: {:?}", event);
        match event {
            Event::NewEvents(_) => {}
            Event::WindowEvent { window_id, event } => {
                match event {
                    WindowEvent::Resized(_) => {}
                    WindowEvent::Moved(_) => {}
                    WindowEvent::CloseRequested => {
                        log::debug!(target: EVENT_LOOP_LOG_TARGET, "Window {:?} close requested", &window_id);
                        if let Some(window) = window_table.get(&window_id).map(Weak::upgrade).flatten() {
                            window.signal_close_requested();
                        }
                    }
                    WindowEvent::Destroyed => {
                        log::debug!(target: EVENT_LOOP_LOG_TARGET, "Window {:?} destroyed", &window_id);
                        window_table.remove(&window_id);
                    }
                    WindowEvent::DroppedFile(_) => {}
                    WindowEvent::HoveredFile(_) => {}
                    WindowEvent::HoveredFileCancelled => {}
                    WindowEvent::ReceivedCharacter(_) => {}
                    WindowEvent::Focused(_) => {}
                    WindowEvent::KeyboardInput { .. } => {}
                    WindowEvent::ModifiersChanged(_) => {}
                    WindowEvent::Ime(_) => {}
                    WindowEvent::CursorMoved { .. } => {}
                    WindowEvent::CursorEntered { .. } => {}
                    WindowEvent::CursorLeft { .. } => {}
                    WindowEvent::MouseWheel { .. } => {}
                    WindowEvent::MouseInput { .. } => {}
                    WindowEvent::TouchpadPressure { .. } => {}
                    WindowEvent::AxisMotion { .. } => {}
                    WindowEvent::Touch(_) => {}
                    WindowEvent::ScaleFactorChanged { .. } => {}
                    WindowEvent::ThemeChanged(_) => {}
                    WindowEvent::Occluded(_) => {}
                }
            }
            Event::DeviceEvent { .. } => {}
            Event::UserEvent(event) => {
                match event {
                    AgnajiEvent::CreateWindow {
                        id, title, initial_size
                    } => {
                        log::debug!(target: EVENT_LOOP_LOG_TARGET, "Received create window request: {:?} size: {:?} (RequestID: {})", title, initial_size, id);
                        let size = if let Some(initial_size) = initial_size {
                            initial_size
                        } else {
                            Vec2u32::new(800, 600)
                        };

                        let window = WindowBuilder::new()
                            .with_title(title)
                            .with_inner_size(PhysicalSize::new(size.x, size.y))
                            .build(&window_target);

                        match window {
                            Ok(window) => {
                                let window_id = window.id();
                                log::debug!(target: EVENT_LOOP_LOG_TARGET, "Window creation successful. Id: {:?}", window_id);

                                let window = Arc::new(Window::new(window, size));
                                window_table.insert(window_id, Arc::downgrade(&window));

                                backend.window_channel.push(id, Ok(window));
                            },
                            Err(error) => {
                                log::error!(target: EVENT_LOOP_LOG_TARGET, "Failed to create window: {:?}", &error);
                                backend.window_channel.push(id, Err(error));
                            }
                        }
                    }
                    AgnajiEvent::Quit => {
                        *control_flow = ControlFlow::ExitWithCode(0);
                        log::debug!(target: EVENT_LOOP_LOG_TARGET,"Received quit order");
                    }
                }
            }
            Event::Suspended => {}
            Event::Resumed => {}
            Event::MainEventsCleared => {}
            Event::RedrawRequested(_) => {}
            Event::RedrawEventsCleared => {}
            Event::LoopDestroyed => {
                log::debug!(target: EVENT_LOOP_LOG_TARGET, "Event loop destroyed");
                engine_thread.take().unwrap().join().unwrap();
            }
        }
    });
}

pub(in crate::winit) struct WindowChannel {
    guarded: Mutex<WindowChannelGuarded>,
    condvar: Condvar,
}

impl WindowChannel {
    pub(in crate::winit) fn new() -> Self {
        Self {
            guarded: Mutex::new(WindowChannelGuarded {
                next_id: 1,
                available_windows: Vec::with_capacity(4),
            }),
            condvar: Condvar::new(),
        }
    }

    pub(in crate::winit) fn allocate_id(&self) -> u64 {
        let mut guard = self.guarded.lock().unwrap();
        let id = guard.next_id;
        guard.next_id += 1;
        drop(guard);

        id
    }

    pub(in crate::winit) fn wait_ready(&self, id: u64) -> Result<Arc<Window>, OsError> {
        let mut guard = self.guarded.lock().unwrap();
        loop {
            let mut found = None;
            for (index, (slot_id, _)) in guard.available_windows.iter().enumerate() {
                if *slot_id == id {
                    found = Some(index);
                    break;
                }
            }

            if let Some(index) = found {
                log::debug!(target: DEFAULT_LOG_TARGET, "Window creation request fulfilled. RequestID: {}", id);
                return guard.available_windows.swap_remove(index).1;
            }

            log::debug!(target: DEFAULT_LOG_TARGET, "Waiting for window creation request fulfillment. RequestID: {}", id);
            guard = self.condvar.wait(guard).unwrap();
        }
    }

    fn push(&self, id: u64, window: Result<Arc<Window>, OsError>) {
        let mut guard = self.guarded.lock().unwrap();
        guard.available_windows.push((id, window));
        drop(guard);

        self.condvar.notify_all();
    }
}

struct WindowChannelGuarded {
    next_id: u64,
    available_windows: Vec<(u64, Result<Arc<Window>, OsError>)>,
}