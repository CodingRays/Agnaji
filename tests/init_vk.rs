extern crate agnaji;

mod common;

#[test]
fn run_test() {
    common::pre_init();

    agnaji::vulkan::AgnajiVulkan::new(true, &[]);
}