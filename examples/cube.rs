use agnaji::prelude::Vec2u32;

fn main() {
    pretty_env_logger::init();

    agnaji::winit::run(|wsi| {
        let window = wsi.create_window("Cube example".to_string(), Some(Vec2u32::new(800, 600))).unwrap();

        while !window.is_close_requested() {
            std::thread::yield_now();
        }
    });
}