static A: Option<Mutex<u32>> = x();
fn f() { A.lock(); }
