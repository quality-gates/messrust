//! Realistic service fixture for exploratory testing.

pub struct Config {
    pub max_retries: usize,
    pub timeout_ms: u64,
}

pub struct Service {
    config: Config,
}

impl Service {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    // Clean control method: should produce no findings under rust ruleset.
    pub fn is_ready(&self) -> bool {
        self.config.max_retries > 0
    }

    // Violates BooleanArgumentFlag (in opinionated/cleancode): boolean parameter
    pub fn process_order(&self, order_id: u64, notify: bool) {
        if notify {
            self.send_notice(order_id);
        }
    }

    // Violates ElseExpression (in opinionated/cleancode): terminal else
    pub fn check_limit(&self, count: usize) -> bool {
        if count < self.config.max_retries {
            true
        } else {
            false
        }
    }

    // Violates StaticAccess (in opinionated/cleancode): calls Helper::log
    pub fn audit(&self) {
        Helper::log("audit event");
    }

    // Violates DevelopmentCodeFragment (in rust/design): println! in method
    pub fn debug_dump(&self) {
        println!("Service timeout: {}", self.config.timeout_ms);
    }

    // Violates ExcessiveParameterList (in rust/codesize): 11 parameters (threshold is 10)
    pub fn configure_pipeline(
        &self,
        p1: u32,
        p2: u32,
        p3: u32,
        p4: u32,
        p5: u32,
        p6: u32,
        p7: u32,
        p8: u32,
        p9: u32,
        p10: u32,
        p11: u32,
    ) -> u32 {
        p1 + p2 + p3 + p4 + p5 + p6 + p7 + p8 + p9 + p10 + p11
    }

    // Violates UnusedLocalVariable (in rust/unusedcode): unused binding
    pub fn compute(&self) -> u32 {
        let unused_result = 42;
        100
    }

    fn send_notice(&self, _order_id: u64) {}
}

pub struct Helper;

impl Helper {
    pub fn log(_msg: &str) {}
}
