use std::cell::RefCell;
thread_local! {
    static SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}
pub fn remember(byte: u8) {
    SCRATCH.with(|s| s.borrow_mut().push(byte));
}
pub fn remember_direct(byte: u8) {
    SCRATCH.with_borrow_mut(|s| s.push(byte));
}
