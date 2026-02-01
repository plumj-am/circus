{
  craneLib,
  commonArgs,
  cargoArtifacts,
}:
craneLib.buildPackage (commonArgs
  // {
    inherit cargoArtifacts;
    pname = "fc-evaluator";
    cargoExtraArgs = "--package fc-evaluator";
  })
