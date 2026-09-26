pub struct Flag(bool);

impl Flag {
    pub fn get(&self) -> bool {
        self.0
    }
}
