{
  craneLib,
  commonArgs,
  cargoArtifacts,
}:
craneLib.buildPackage (commonArgs
  // {
    inherit cargoArtifacts;
    pname = "fc-migrate-cli";
    cargoExtraArgs = "--package fc-migrate-cli";
  })
