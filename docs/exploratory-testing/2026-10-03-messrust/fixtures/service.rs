pub struct Service {
    pub a: i32,
    pub b: i32,
    c: i32,
    d: i32,
}

impl Service {
    pub fn do_work(&self, flag: bool) {
        println!("Working: {}", self.a);
        if flag {
            let unused_val = 10;
        } else {
            let other = 20;
        }
    }

    pub fn long_param_list(
        &self,
        p1: i32,
        p2: i32,
        p3: i32,
        p4: i32,
        p5: i32,
        p6: i32,
        p7: i32,
        p8: i32,
        p9: i32,
        p10: i32,
    ) {
        let _ = p1 + p2 + p3 + p4 + p5 + p6 + p7 + p8 + p9 + p10;
    }

    pub fn call_external(&self) {
        Other::make();
    }
}

pub struct Other;
impl Other {
    pub fn make() -> i32 {
        1
    }
}
