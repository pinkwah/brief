use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::ExitCode;

use crate::init::Service;

pub fn status() -> ExitCode {
    let Some(service) = Service::from_existing() else {
        return not_running();
    };

    let mnt = Path::new("/proc")
        .join(service.pid.to_string())
        .join("ns/mnt");
    let Ok(mntid) = fs::read_link(mnt) else {
        return not_running();
    };

    println!("nixbox running (PID: {})", service.pid);
    println!("\nPID\t\tCOMMAND");
    for entry in fs::read_dir("/proc").expect("Coult not read /proc") {
        let Ok(entry) = entry else { continue };
        let Ok(entry_mntid) = fs::read_link(entry.path().join("ns/mnt")) else {
            continue;
        };
        if entry_mntid == mntid {
            let mut cmdline = String::new();
            File::open(entry.path().join("cmdline"))
                .and_then(|mut file| file.read_to_string(&mut cmdline))
                .expect("/proc/_/cmdline count not be read");

            cmdline = cmdline.replace('\0', " ");

            println!("{}\t\t{}", entry.file_name().to_str().unwrap(), cmdline,);
        }
    }

    ExitCode::SUCCESS
}

fn not_running() -> ExitCode {
    println!("nixbox not running");
    ExitCode::FAILURE
}
