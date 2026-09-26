pub trait Worker {
    fn run(&self, payload: String);
}
