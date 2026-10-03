pub struct SuppressedDemo;

impl SuppressedDemo {
    pub fn run(&self) {
        // messrust-disable-next-line DevelopmentCodeFragment
        println!("This print is suppressed");

        // messrust-disable ElseExpression
        if true {
            let x = 1;
            let _ = x;
        } else {
            let y = 2;
            let _ = y;
        }
        // messrust-enable ElseExpression
    }
}
