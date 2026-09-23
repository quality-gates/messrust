use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Instant;

static REQUESTS: AtomicU64 = AtomicU64::new(0);
static AUDIT: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static CONFIG: OnceLock<HashMap<String, String>> = OnceLock::new();
static VERSION: &str = "1.0";
const MAX_ITEMS: usize = 10;

thread_local! {
    static SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

pub struct Order {
    pub items: Vec<u32>,
}

// Pure: only parameters, constants, and an immutable static.
pub fn order_total(prices: &[u32], tax: u32) -> u32 {
    let base: u32 = prices.iter().take(MAX_ITEMS).sum();
    base + tax + VERSION.len() as u32
}

// Implicit input: env var (expect ImplicitInput 'env::var').
pub fn api_url() -> String {
    env::var("API_URL").unwrap_or_default()
}

// Implicit input: clock (expect ImplicitInput 'Instant::now').
pub fn elapsed_ms(start: Instant) -> u128 {
    Instant::now().duration_since(start).as_millis()
}

// Implicit input: OnceLock read (expect ImplicitInput global 'CONFIG').
pub fn setting(key: &str) -> Option<String> {
    CONFIG.get().and_then(|c| c.get(key).cloned())
}

// Implicit output: counter (expect ImplicitOutput global 'REQUESTS').
pub fn handle(order: &Order) -> usize {
    REQUESTS.fetch_add(1, Ordering::SeqCst);
    order.items.len()
}

// Implicit output: audit log through a Mutex (expect ImplicitOutput global 'AUDIT').
pub fn audit(message: &str) {
    AUDIT.lock().unwrap().push(message.to_string());
}

// Implicit output: thread-local write, common idiom (expect ImplicitOutput global 'SCRATCH').
pub fn remember(byte: u8) {
    SCRATCH.with(|s| s.borrow_mut().push(byte));
}

// Implicit output: writes to stderr through a macro argument (expect ImplicitOutput 'io::stderr').
pub fn warn_user(message: &str) {
    let _ = writeln!(std::io::stderr(), "warning: {message}");
}

// Implicit output: mutates an argument (expect ImplicitOutput '&mut' parameter 'order').
pub fn add_item(order: &mut Order, item: u32) {
    order.items.push(item);
}

// Implicit output: logging (expect ImplicitOutput 'println!').
pub fn checkout(order: &Order) -> u32 {
    let total = order_total(&order.items, 0);
    println!("total={total}");
    total
}

impl Order {
    // Default mode: no findings for self use.
    pub fn count(&self) -> usize {
        self.items.len()
    }

    // Default mode: no findings for &mut self.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}
