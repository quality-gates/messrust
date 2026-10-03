static mut COUNTER: i32 = 0;

pub fn step() {
    unsafe {
        COUNTER = 42;
    }
}
