{
  craneLib,
  commonArgs,
  cargoArtifacts,
}:
craneLib.buildPackage (commonArgs
  // {
    inherit cargoArtifacts;
    pname = "fc-queue-runner";
    cargoExtraArgs = "--package fc-queue-runner";
  })
