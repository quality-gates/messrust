static mut COUNTER: i32 = 0;

pub fn step() {
    unsafe {
        *&raw mut COUNTER = 42;
    }
}

pub fn step_paren() {
    unsafe {
        *(&raw mut COUNTER) = 43;
    }
}
