#!/usr/bin/env sh
set -eu

export USER=$(whoami)
curl -L https://nixos.org/nix/install | sh /dev/stdin --no-daemon

source $HOME/.nix-profile/etc/profile.d/nix.sh
nix-channel --update
nix-env -iA \
  nixpkgs.bashInteractive \
  nixpkgs.coreutils-full \
  nixpkgs.gnutar \
  nixpkgs.gzip \
  nixpkgs.gnugrep \
  nixpkgs.which \
  nixpkgs.curl \
  nixpkgs.less \
  nixpkgs.wget \
  nixpkgs.man \
  nixpkgs.findutils

mkdir "${NIXBOX_BINDIR}"
cp "${NIXBOX_EXECUTABLE}" "${NIXBOX_BINDIR}/"
