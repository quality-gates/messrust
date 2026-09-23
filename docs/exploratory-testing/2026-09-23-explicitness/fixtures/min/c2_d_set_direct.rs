thread_local! { static K: Cell<u32> = x(); }
fn f() { K.set(1); }
