{ pkgs, ... }:

{
  packages = with pkgs; [rustup stdenv.cc];
}
