pub fn service() {
    let unused_value = 42;
    // messrust-disable-next-line UnusedLocalVariable
    let intentional_unused = 7;
}

#[cfg(test)]
mod tests {
    fn test_example() {
        let test_unused = 1;
    }
}
