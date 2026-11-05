# Packaging

Where waimea is distributed and how each channel is kept up to date.

| Channel | Files | Status |
|---------|-------|--------|
| GitHub Releases | `.github/workflows/release.yml` | published on every `v*` tag |
| install script | `install.sh` | reads the latest release |
| `.deb` / `.rpm` | `[package.metadata.deb]`, `[package.metadata.generate-rpm]` in `Cargo.toml` | release assets |
| cargo-binstall | `[package.metadata.binstall]` in `Cargo.toml` | release archives |
| Nix | `flake.nix`, `flake.lock` | `nix run github:JohnGamble24k/Waimea` |
| AUR | `aur/waimea-bin`, `aur/waimea` | needs `AUR_SSH_PRIVATE_KEY` |
| Snap Store | `../snap/snapcraft.yaml` | needs `SNAPCRAFT_STORE_CREDENTIALS` |
| crates.io | `Cargo.toml` | needs `CARGO_REGISTRY_TOKEN` |

## What a release does

Pushing a tag `vX.Y.Z` (it must match `version` in `Cargo.toml`) runs `release.yml`:

1. Builds macOS (arm64, x86_64; signed and notarized, plus `.dmg`), Linux (x86_64, arm64) and Windows (x86_64).
2. Builds `.deb` and `.rpm` for both Linux architectures and installs them in `ubuntu:24.04`, `debian:12` and `fedora:latest`.
3. Packs `waimea-<version>-<target>.tar.gz` / `.zip` and checks them against the cargo-binstall metadata.
4. Writes `SHA256SUMS`, runs `install.sh` against the files on Linux and macOS, and builds and tests the snap.
5. Creates the GitHub release with every file.
6. Publishes to the AUR, the Snap Store and crates.io, each only if its secret exists (otherwise the job logs a notice and does nothing).


`gh workflow run release.yml --ref <branch>` is a dry run: steps 1 to 4 run and every file is kept as a workflow artifact, nothing is published.

Repository secrets are added under GitHub > waimea > Settings > Secrets and variables > Actions > New repository secret, or with `gh secret set NAME -R JohnGamble24k/Waimea`.

## AUR

`aur/waimea-bin` installs the release binary; `aur/waimea` builds the release tag from source. The release workflow sets `pkgver`, refreshes the checksums and `.SRCINFO`, and pushes both with [github-actions-deploy-aur](https://github.com/KSXGitHub/github-actions-deploy-aur).

One-time setup:

1. Create an account at <https://aur.archlinux.org/register>.
2. Make a key just for publishing: `ssh-keygen -t ed25519 -C "waimea AUR" -f ~/.ssh/aur_waimea -N ""`.
3. Paste `~/.ssh/aur_waimea.pub` into "My Account > SSH Public Key" on the AUR.
4. Add the private key as a secret: `gh secret set AUR_SSH_PRIVATE_KEY -R JohnGamble24k/Waimea < ~/.ssh/aur_waimea`.

The first push creates both AUR packages (an AUR package exists as soon as something is pushed to `ssh://aur@aur.archlinux.org/<name>.git`). To publish the current version without waiting for a release, push by hand from an Arch machine or container:

```sh
git clone ssh://aur@aur.archlinux.org/waimea-bin.git && cd waimea-bin
cp ../waimea/packaging/aur/waimea-bin/PKGBUILD .
updpkgsums && makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO && git commit -m "Initial import" && git push
```

## Snap Store

`snap/snapcraft.yaml` builds a strictly confined snap (core24) from source for amd64 and arm64. Audio goes through PulseAudio/PipeWire (`audio-playback`), files through `home` and `removable-media`, MPRIS through the `mpris` slot.

Limitation: a strict snap cannot run programs installed on the host, so yt-dlp (online search and streaming of page URLs) and ffmpeg are not available in the snap. Bundling them would add over 100 MB and an old yt-dlp breaks quickly, so they are left out; the description tells users to pick another channel if they need them. Classic confinement would lift this but needs a manual review by the Snap Store.

One-time setup (on any machine with snapcraft: `sudo snap install snapcraft --classic`):

1. Create an Ubuntu One account at <https://snapcraft.io/account> and accept the developer agreement.
2. `snapcraft login`, then `snapcraft register waimea`.
3. Export credentials for CI: `snapcraft export-login --snaps waimea --channels stable,candidate,beta,edge --acls package_access,package_push,package_update,package_release -` and save the output with `gh secret set SNAPCRAFT_STORE_CREDENTIALS -R JohnGamble24k/Waimea` (paste it when prompted).
4. Optional: request auto-connection of `mpris` with the name `waimea` in the [snapcraft forum](https://forum.snapcraft.io/c/store-requests/19) if the store flags it on the first upload.

Each release then uploads both snaps to the `stable` channel. The credentials expire after a year by default (`--expires` changes that); re-export and update the secret when they do.

## crates.io

The crate ships only `src`, `Cargo.toml`, `Cargo.lock`, `README.md` and `LICENSE` (see `include` in `Cargo.toml`); the Packaging workflow runs `cargo publish --dry-run` on every change.

One-time setup:

1. Log in at <https://crates.io> with GitHub, verify your email under Account Settings.
2. Create an API token at <https://crates.io/settings/tokens> with the scopes `publish-new` and `publish-update` (crate name `waimea`).
3. Either publish the current version by hand (`cargo login`, then `cargo publish` from a clean checkout of the tag), or add the token so releases publish automatically: `gh secret set CARGO_REGISTRY_TOKEN -R JohnGamble24k/Waimea`.

Once the crate is live, `cargo install waimea` and `cargo binstall waimea` work (binstall downloads the release archive).
