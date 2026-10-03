use odm_host::{run, HostConfig};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .iter()
        .any(|a| a == "--register" || a == "--unregister")
    {
        eprintln!("not implemented yet");
        std::process::exit(2);
    }
    let cfg = HostConfig::from_env();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let code = match run(&mut stdin.lock(), &mut stdout.lock(), &cfg) {
        Ok(()) => 0,
        Err(_) => 1,
    };
    std::process::exit(code);
}
