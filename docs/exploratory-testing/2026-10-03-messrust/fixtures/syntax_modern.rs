pub struct Point {
    pub r#type: i32,
    pub x: i32,
}

pub fn test_modern(p: Point, opt: Option<i32>, items: &[i32]) {
    // let-else
    let Some(val) = opt else {
        return;
    };
    println!("{val}");

    // slice pattern
    if let [first, .., last] = items {
        println!("{first}");
        let _ = last;
    }

    // format captures
    let r#match = 100;
    println!("{match}");

    // struct field access
    println!("{}", p.r#type);
}
