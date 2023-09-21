use crate::{block_on, config::Config, guest, host};
use std::{fs, io::Write, path::Path, process::Command};
use nix::sys::signal::{kill, Signal};
use zbus::Result;

const INSTALL_SCRIPT: &[u8] = r#"
set -eux

export USER=$(whoami)
curl -L https://nixos.org/nix/install | sh /dev/stdin --no-daemon

source $HOME/.nix-profile/etc/profile.d/nix.sh
nix-channel --add https://nixos.org/channels/nixos-unstable nixos
nix-channel --remove nixpkgs
nix-channel --update

export NIX_PATH=$HOME/.nix-defexpr/channels/nixos
export NIXOS_CONFIG=/run/host$NIXOS_CONFIG
export pathToConfig="$(nix-build '<nixpkgs/nixos>' --no-out-link -A system)"
nix-env -p /nix/var/nix/profiles/system --set "$pathToConfig"
"#.as_bytes();

const POSTINSTALL_SCRIPT: &[u8] = r#"
set -eux

/bin/sh
"#.as_bytes();

const NIXOS_CONFIG: &[u8] = r#"
{ config, pkgs, ... }:

{
  system.stateVersion = "23.05";

  hardware.opengl.enable = true;
  time.timeZone = "Europe/Oslo";
  i18n.defaultLocale = "nb_NO.UTF-8";

  boot.loader.grub.enable = false;
  fileSystems."/".device = "/dev/null";
}
"#.as_bytes();

pub fn install() {
    let mut config = Config::from_file_or_default();
    config.use_host_root = true;

    create_install_dir(&config.guest_home);
    create_install_dir(&config.guest_nix);

    let host_pid = host::start_server_fork(&config);

    write_text("/tmp/nixbox-install.sh", INSTALL_SCRIPT);
    write_text("/tmp/nixbox-postinstall.sh", POSTINSTALL_SCRIPT);
    write_text(&config.guest_nixos_config, NIXOS_CONFIG);

    block_on(install_nixos()).unwrap();

    config.use_host_root = false;

    kill(host_pid, Signal::SIGTERM).unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));

    host::start_server_fork(&config);

    block_on(async {
        println!(">> Hello");
        let client = guest::client().await.unwrap();
        println!(">> {:?}", client.run("/bin/sh", &["/tmp/nixbox-postinstall.sh"]).await);
    });
}

async fn install_nixos() -> Result<()> {
    let client = guest::client().await?;
    client.run("bash", &["/tmp/nixbox-install.sh"]).await?;

    Ok(())
}

fn create_install_dir(nixbox_dir: &Path) {
    fs::create_dir_all(nixbox_dir)
        .unwrap_or_else(|err| panic!("Could not create '{}': {}", nixbox_dir.display(), err));
}

fn write_text(path: impl AsRef<Path>, text: &[u8]) {
    let path = path.as_ref();
    fs::File::create(path)
        .unwrap_or_else(|err| panic!("Could not create {}: {}", path.display(), err))
        .write_all(text)
        .unwrap_or_else(|err| panic!("Could not write to {}: {}", path.display(), err));
}
