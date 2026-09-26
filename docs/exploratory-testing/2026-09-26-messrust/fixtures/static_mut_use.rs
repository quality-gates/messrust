mod state {
    pub static mut COUNT: usize = 0;
}

use state::COUNT;

pub fn bump() {
    unsafe {
        state::COUNT += 1;
    }
}
