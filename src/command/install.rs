use crate::config::Config;
use crate::setup::setup;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use super::run;

const INSTALL_SCRIPT: &'static [u8] = include_bytes!("install.sh");

pub fn install(config: &Config) -> ExitCode {
    if config.nix_home.exists() {
        panic!(
            "Nixbox installation exists. Please delete {}",
            config.nix_home.display()
        );
    }

    create_install_dir(&config.nix_home);
    create_install_dir(&config.nixbox_bindir());
    create_install_dir(&config.xdg_data_home());
    create_install_dir(&config.xdg_config_home());
    create_install_dir(&config.xdg_state_home());

    setup(config);

    fs::File::create("/tmp/nixbox-install.sh")
        .unwrap_or_else(|err| panic!("Could not create /tmp/nixbox-install.sh: {}", err))
        .write(&INSTALL_SCRIPT)
        .unwrap_or_else(|err| panic!("Could not write to /tmp/nixbox-install.sh: {}", err));

    let env: Vec<(String, String)> = vec![];
    // run(config, "bash", &["-c", "bash <(curl -L https://nixos.org/nix/install) --no-daemon"], env.into_iter())
    run(config, "bash", &["/tmp/nixbox-install.sh"], env.into_iter())
    // run(config, "bash", &[] as &[&'static str], env.into_iter())
}

fn create_install_dir(nixbox_dir: &Path) {
    fs::create_dir_all(&nixbox_dir)
        .unwrap_or_else(|err| panic!("Could not create '{}': {}", nixbox_dir.display(), err));
}
