static A: Mutex<u32> = x();
fn f() { A.lock(); }
