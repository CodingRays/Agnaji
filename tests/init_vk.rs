extern crate agnaji;

mod common;

#[test]
fn run_test() {
    common::pre_init();

    let mut initializer = agnaji::vulkan::init::AgnajiVulkanInitializer::new(None, true);
    let device_reports = initializer.generate_device_reports().unwrap();

    let mut selected = None;
    for device in device_reports.iter() {
        println!("{:?}", device);
        if device.is_suitable() {
            selected = Some(device);
            break;
        }
    }

    if let Some(selected) = selected {
        let (_agnaji, _) = initializer.build(selected).unwrap();
    }
}