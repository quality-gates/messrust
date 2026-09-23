pub fn a() {
    // messrust-disable-next-line ImplicitOutput
    println!("a");
}
// messrust-disable ImplicitOutput
pub fn b() {
    println!("b");
}
// messrust-enable ImplicitOutput
// messrust-disable-next-line ImplicitOutput
pub fn c() {
    println!("c");
}
