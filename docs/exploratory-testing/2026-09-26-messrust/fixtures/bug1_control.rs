//! Control for Bug 1: GlobalVariable with explicit qualified paths.

mod counter_mod {
    pub static mut COUNTER: usize = 0;
}

pub fn bump_counter() {
    unsafe {
        counter_mod::COUNTER += 1;
    }
}

pub static mut TOTAL: usize = 0;

mod worker_mod {
    pub fn bump_total() {
        unsafe {
            super::TOTAL += 1;
        }
    }
}
