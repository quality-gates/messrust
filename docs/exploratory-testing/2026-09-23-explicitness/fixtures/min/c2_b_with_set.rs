thread_local! { static K: Cell<u32> = x(); }
fn f() { K.with(|s| s.set(1)); }
