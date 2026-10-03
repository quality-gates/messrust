use std::fs::File;
use std::time::Instant;
use std::process::Command;

pub fn test_inputs() {
    let _ = File::open("input.txt");
    let _ = Instant::now();
}

pub fn test_outputs() {
    let _ = File::create("output.txt");
    let _ = Command::new("ls");
}
