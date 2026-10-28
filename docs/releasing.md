# Releasing

For the maintainer: how a version becomes a GitHub release and an AUR package.

## What is built

`.github/workflows/release.yml` runs `packaging/dist.sh` on Ubuntu 22.04:

| File | What |
|---|---|
| `quietlinux_VERSION-1_amd64.deb`, `quietlinux-VERSION-1.x86_64.rpm` | packages ([nfpm](https://nfpm.goreleaser.com), `packaging/nfpm.yaml`) |
| `quietlinux-VERSION-x86_64-linux.tar.gz` | prebuilt programs with `install.sh` (which compiles nothing then) |
| `SHA256SUMS` | checksums of the files above |

Every file carries `THIRD-PARTY-LICENSES.html`, the licenses of the Rust
crates compiled into the programs ([cargo-about](https://github.com/EmbarkStudios/cargo-about),
`packaging/about.toml`). The AUR package is built from source by its users
(`packaging/aur/PKGBUILD`).

## Test run

On GitHub: **Actions › Release › Run workflow**. It builds all files and keeps
them as a workflow artifact for a few days, without a release. Try the
packages before the first real release, e.g. the `.deb` in an Ubuntu VM or
container and the archive on any distribution.

## Release steps

1. Set the version in `Cargo.toml` (`[workspace.package]`), run
   `cargo build` so that `Cargo.lock` follows, and update `packaging/aur/PKGBUILD`
   (`pkgver`, `pkgrel=1`).
2. In `CHANGELOG.md`, replace `unreleased` with today's date
   (`## [0.2.0] - 2026-11-30`) and add the link at the bottom.
3. Commit, push, wait for CI to pass.
4. Tag and push the tag:

   ```sh
   git tag -a v0.2.0 -m "QuietLinux 0.2.0"
   git push origin v0.2.0
   ```

   The workflow checks that the tag matches `Cargo.toml` and that the
   changelog has a dated section, builds everything and creates a **draft**
   release with the changelog section as its notes.
5. On GitHub, open the draft under **Releases**, check the notes and files,
   and publish it.
6. Update the AUR package (below).

## AUR

The AUR package lives in its own Git repository on aur.archlinux.org; this
repository keeps the master copy in `packaging/aur/`.

First time only:

- Create an account on <https://aur.archlinux.org> and add your SSH public key
  to it.
- `git clone ssh://aur@aur.archlinux.org/quietlinux.git aur-quietlinux`
  (an empty repository: the name is reserved on the first push).

For each release, after the tag is on GitHub:

```sh
cd aur-quietlinux
cp ../QuietLinux/packaging/aur/PKGBUILD .
updpkgsums                         # downloads the release's source, fills sha256sums
makepkg -si                        # builds and installs it: test it
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "quietlinux 0.2.0"
git push
```

Then copy the PKGBUILD with the filled-in checksum back to
`packaging/aur/PKGBUILD` and commit it.

## After a release

Start the next section at the top of `CHANGELOG.md`, e.g.
`## [0.2.0] - unreleased`, and note changes there as they are made.
