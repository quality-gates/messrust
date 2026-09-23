use std::sync::{LazyLock, Mutex};
static AUDIT: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static PLAIN: Mutex<Vec<String>> = Mutex::new(Vec::new());
pub fn audit(message: &str) {
    AUDIT.lock().unwrap().push(message.to_string());
}
pub fn audit_plain(message: &str) {
    PLAIN.lock().unwrap().push(message.to_string());
}
