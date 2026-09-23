pub struct Counter {
    count: u32,
    step: u32,
}

impl Counter {
    // No self use: expect no finding.
    pub fn new(step: u32) -> Self {
        Self { count: 0, step }
    }

    // Reads self: expect ImplicitInput self.
    pub fn value(&self) -> u32 {
        self.count
    }

    // Changes self: expect ImplicitOutput self (and ImplicitInput for the read of self.step).
    pub fn tick(&mut self) {
        self.count += self.step;
    }

    // Explicit-type receiver: expect the same as tick.
    pub fn reset(self: &mut Self) {
        self.count = 0;
    }

    // Closure captures self: expect ImplicitInput self.
    pub fn scaled(&self, xs: &[u32]) -> Vec<u32> {
        xs.iter().map(|x| x * self.step).collect()
    }

    // Consumes self by value: user said accessing class data is implicit input.
    pub fn into_count(self) -> u32 {
        self.count
    }
}

pub trait Shape {
    fn area(&self) -> u32;
    // Default body in a trait declaration reads self.
    fn double(&self) -> u32 {
        self.area() * 2
    }
}

impl Shape for Counter {
    // Trait impl: self checks skipped by design.
    fn area(&self) -> u32 {
        self.count
    }
}
