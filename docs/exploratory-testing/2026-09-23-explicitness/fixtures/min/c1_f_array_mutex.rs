static A: [Mutex<u32>; 2] = x();
fn f() { A.lock(); }
