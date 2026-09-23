static A: LazyLock<Mutex<u32>> = x();
fn f() { A.lock(); }
