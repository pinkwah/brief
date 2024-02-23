{ runCommand, system, buildPackages, nix, brief-cli }:

let
  installerClosureInfo = buildPackages.closureInfo {
    rootPaths = [ nix brief-cli ];
  };

  inherit (brief-cli) version;

  env.meta.description = "Brief bootstrap binaries for ${system}";

in runCommand "brief-binary-tarball-${version}" env ''
  cp ${installerClosureInfo}/registration $TMPDIR/reginfo

  cp ${./install.sh} $TMPDIR/install       \
    --subst-var-by brief-cli ${brief-cli}  \
    --subst-var-by nix ${nix}

  chmod +x $TMPDIR/install
  dir=brief-${version}-${system}
  fn=$out/$dir.tar.xz

  tar cvfJ $fn                                     \
    --owner=0 --group=0 --mode=u+rw,uga+r          \
    --mtime='1970-01-01'                           \
    --absolute-names                               \
    --hard-dereference                             \
    --transform "s,$TMPDIR/install,$dir/install,"  \
    --transform "s,$TMPDIR/reginfo,$dir/.reginfo," \
    --transform "s,$NIX_STORE,$dir/store,"         \
    $TMPDIR/install                                \
    $(cat ${installerClosureInfo}/store-paths)
''
