thread_local! { static K: RefCell<u32> = x(); }
fn f() { K.with(|s| s.borrow_mut()); }
