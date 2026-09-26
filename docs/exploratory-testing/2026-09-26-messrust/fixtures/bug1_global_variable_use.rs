//! Reproducer for Bug 1: GlobalVariable misses mutations through `use` imports.

// Case 1: static mut in submodule, imported into root via `use`
mod counter_mod {
    pub static mut COUNTER: usize = 0;
}

use counter_mod::COUNTER;

pub fn bump_counter() {
    unsafe {
        // Bug: Mutation through imported name `COUNTER` is missed (exit 0).
        // Control: `counter_mod::COUNTER += 1;` correctly triggers GlobalVariable (exit 2).
        COUNTER += 1;
    }
}

// Case 2: static mut in root, imported into submodule via `use super::...`
pub static mut TOTAL: usize = 0;

mod worker_mod {
    use super::TOTAL;

    pub fn bump_total() {
        unsafe {
            // Bug: Mutation through `use super::TOTAL` is missed (exit 0).
            // Control: `super::TOTAL += 1;` correctly triggers GlobalVariable (exit 2).
            TOTAL += 1;
        }
    }
}
