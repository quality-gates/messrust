use std::cell::Cell;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

static mut BUFFER: [u8; 4] = [0; 4];
static READY: AtomicBool = AtomicBool::new(false);
thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

// E1 Rust 2024 raw borrow of static mut: expect ImplicitOutput global 'BUFFER'.
pub fn buffer_ptr() -> *mut [u8; 4] {
    &raw mut BUFFER
}

// E2 addr_of_mut!: expect ImplicitOutput global 'BUFFER'.
pub fn buffer_ptr_macro() -> *mut [u8; 4] {
    std::ptr::addr_of_mut!(BUFFER)
}

// E3 Cell thread-local through with(): expect ImplicitOutput global 'DEPTH'.
pub fn enter() {
    DEPTH.with(|d| d.set(d.get() + 1));
}

// E4 tokio path: expect ImplicitInput 'fs::read_to_string'.
pub async fn load(path: &str) -> String {
    tokio::fs::read_to_string(path).await.unwrap_or_default()
}

// E5 stdin with a local &mut: expect ImplicitInput 'io::stdin' only.
pub fn read_line() -> String {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    line
}

// E6 log macro with target: expect ImplicitOutput 'info!' and ImplicitInput global 'READY'.
pub fn report() {
    log::info!(target: "svc", "ready={}", READY.load(Ordering::Relaxed));
}

// E7 nested module function: expect ImplicitOutput 'eprintln!'.
pub mod inner {
    pub fn shout() {
        eprintln!("x");
    }
}

// E8 generic impl: expect name 'Wrapper::set' and ImplicitOutput '&mut' param 'dst'.
pub struct Wrapper<T>(T);
impl<T: Clone> Wrapper<T> {
    pub fn set(&self, dst: &mut T) {
        *dst = self.0.clone();
    }
}

// E9 local file read through a local handle: expect ImplicitInput 'File::open' only.
pub fn slurp(path: &str) -> String {
    let mut s = String::new();
    std::fs::File::open(path).unwrap().read_to_string(&mut s).unwrap();
    s
}

// E10 flush stdout: expect ImplicitOutput 'io::stdout'.
pub fn flush() {
    std::io::stdout().flush().unwrap();
}

// E11 mutable local named like nothing global: expect nothing.
pub fn local_only(n: u32) -> u32 {
    let mut acc = 0;
    for i in 0..n {
        acc += i;
    }
    let r = &mut acc;
    *r += 1;
    acc
}
