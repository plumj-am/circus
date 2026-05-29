-- Track per-build `meta` fields produced by nix-eval-jobs (which mirrors
-- nixpkgs `meta = { description, license, maintainers, homepage, ... }`).
-- The channel tarball generator emits these into `default.nix`'s
-- mkFakeDerivation `meta` attrset so consumers (`nix-env -qa --description`,
-- `nix search`, `nix-env -qa --json`) see the same metadata they would on a
-- real channel.
--
-- All columns are nullable: jobs without a `meta` attrset, or evaluators
-- running an older nix-eval-jobs that does not surface them, leave them
-- unset and the channel generator omits the `meta` block for those builds.
ALTER TABLE builds
ADD COLUMN meta_description TEXT,
ADD COLUMN meta_license TEXT,
ADD COLUMN meta_homepage TEXT,
ADD COLUMN meta_maintainers TEXT;
