static A: std::sync::Mutex<u32> = x();
fn f() { A.lock(); }
