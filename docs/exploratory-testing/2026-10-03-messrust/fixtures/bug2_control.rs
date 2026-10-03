pub struct Service;
pub struct Helper;

pub fn test_path() {
    Service::run();
}

pub fn test_qualified() {
    <Helper>::run();
}
