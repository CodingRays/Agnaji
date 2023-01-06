extern crate agnaji;

mod common;

#[test]
fn run_test() {
    common::pre_init();

    let agnaji = agnaji::vulkan::AgnajiVulkan::new(true, &[]);
    let device_reports = agnaji.generate_main_device_report();
    for device in device_reports.iter() {
        println!("{:?}", &device);
        if device.is_suitable() {
            agnaji.set_main_device(device);
        }
    }
}