pub fn pure(a: u32) -> u32 { a }
#[cfg(test)]
mod tests {
    #[test]
    fn prints() { println!("x"); }
}
