#!/usr/bin/env sh
set -eu

export USER=$(whoami)
curl -L https://nixos.org/nix/install | sh /dev/stdin --no-daemon

cat > $NIXOS_CONFIG <<EOF
{ config, pkgs, ... }:

{
  system.stateVersion = "23.05";

  hardware.opengl.enable = true;
  time.timeZone = "Europe/Oslo";
  i18n.defaultLocale = "nb_NO.UTF-8";

  boot.loader.grub.enable = false;
  fileSystems."/".device = "/dev/null";
}
EOF

source $HOME/.nix-profile/etc/profile.d/nix.sh
# nix-channel --add https://nixos.org/channels/nixos-unstable nixos
# nix-channel --remove nixpkgs

nix-channel --add https://github.com/nix-community/home-manager/archive/master.tar.gz home-manager
nix-channel --update

mkdir -p /nix/var/nix/profiles/per-user/root
ln -s $HOME/.nix-defexpr/channels /nix/var/nix/profiles/per-user/root/channels

nix-build '<nixpkgs/nixos>' -A system -o "${NIXBOX_ROOT}"
nix-shell '<home-manager>' -A install

mkdir -p "${NIXBOX_BINDIR}"
cp "${NIXBOX_EXECUTABLE}" "${NIXBOX_BINDIR}/nixbox"
