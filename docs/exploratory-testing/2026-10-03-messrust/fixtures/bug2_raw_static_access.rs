pub struct r#Service;
pub struct r#Helper;

pub fn test_path() {
    r#Service::run();
}

pub fn test_qualified() {
    <r#Helper>::run();
}
