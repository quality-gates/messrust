static mut TOTAL: usize = 0;

mod worker {
    use super::TOTAL;

    pub fn bump() {
        unsafe {
            super::TOTAL += 1;
        }
    }
}
