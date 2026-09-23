static A: LazyLock<u32> = x();
fn f() { A.lock(); }
