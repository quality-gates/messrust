fn main() {
    let code = messrust::run(
        &std::env::args().skip(1).collect::<Vec<_>>(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    );
    std::process::exit(code);
}
