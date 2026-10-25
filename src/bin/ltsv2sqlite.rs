use std::io;
use std::process::ExitCode;

use rs_ltsv2sqlite::stdin2lines2sqlite2fs_default;

fn io_envkey2val(key: &'static str) -> impl Fn() -> String {
    move || std::env::var(key).unwrap_or_default()
}

fn io_path2sqlite() -> impl Fn() -> String {
    io_envkey2val("ENV_SQLITE_DB_FILE_PATH")
}

fn io_print_changed() -> impl FnMut(usize) -> Result<(), io::Error> {
    |chg: usize| {
        println!("changed: {chg}");
        Ok(())
    }
}

fn io_main() -> impl FnOnce() -> Result<(), io::Error> {
    || {
        let path2sqlite: String = io_path2sqlite()();
        let chg: usize = stdin2lines2sqlite2fs_default(path2sqlite)?;
        io_print_changed()(chg)
    }
}

fn sub() -> Result<(), io::Error> {
    io_main()()
}

fn main() -> ExitCode {
    sub().map(|_| ExitCode::SUCCESS).unwrap_or_else(|e| {
        eprintln!("{e}");
        let path2sqlite: String = io_path2sqlite()();
        eprintln!("ENV_SQLITE_DB_FILE_PATH: {path2sqlite}");
        eprintln!("default timestamp key: 'timestamp'");
        eprintln!("default severity key: 'severity'");
        eprintln!("default message key: 'message'");
        ExitCode::FAILURE
    })
}
