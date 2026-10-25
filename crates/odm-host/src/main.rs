use odm_host::register::{register, unregister, RegisterOptions};
use odm_host::{run, HostConfig};
use std::path::PathBuf;

const USAGE: &str = "usage: odm-host --register --app <path-to-odm> | --unregister";

fn options(app_exe: PathBuf) -> std::io::Result<RegisterOptions> {
    Ok(RegisterOptions {
        host_exe: std::env::current_exe()?,
        app_exe,
        chromium_ids: odm_core::ids::chromium_ext_ids()
            .iter()
            .map(|s| s.to_string())
            .collect(),
        #[cfg(windows)]
        hkcu_prefix: String::new(),
        #[cfg(unix)]
        home: odm_core::ids::home_dir()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is not set"))?,
    })
}

fn finish(result: std::io::Result<()>, ok_message: &str) -> ! {
    match result {
        Ok(()) => {
            println!("{ok_message}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--register") {
        let app = args
            .iter()
            .position(|a| a == "--app")
            .and_then(|i| args.get(i + 1));
        let Some(app) = app else {
            eprintln!("{USAGE}");
            std::process::exit(2);
        };
        finish(
            options(PathBuf::from(app)).and_then(|o| register(&o)),
            "Registered ODM for Chrome, Edge, Brave, Vivaldi, Opera, Chromium, Firefox and Floorp.",
        );
    }
    if args.iter().any(|a| a == "--unregister") {
        finish(
            options(PathBuf::new()).and_then(|o| unregister(&o)),
            "Unregistered ODM from Chrome, Edge, Brave, Vivaldi, Opera, Chromium, Firefox and Floorp.",
        );
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
