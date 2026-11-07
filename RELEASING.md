# Releasing Bible for macOS

The iPhone and iPad app goes through the App Store instead; see
[apps/ios/APP_STORE.md](apps/ios/APP_STORE.md).

`scripts/release-macos.sh` builds everything on a Mac in one command. It writes to `dist/`:

| File | What it is |
|---|---|
| `bible-rs-<version>-universal.dmg` | Disk image: the app, an Applications link and the install background |
| `bible-rs-universal.dmg` | The same disk image under the name the website links to |
| `bible-rs-<version>-universal.zip` | The app alone; installed copies update from it |
| `bible-rs-<version>-SHA256SUMS.txt` | SHA-256 of the three files above |
| `Bible.app` | The app itself (universal, signed, stapled) |

The app runs on macOS 11 or later, on Apple silicon and Intel. The script signs it with the
certificate "Developer ID Application: <your name> (<TEAMID>)" and turns on the hardened runtime.
Without a Developer ID in the keychain, `ALLOW_ADHOC=1` builds an ad-hoc signed app: it runs, but
macOS makes the reader approve it once under Privacy & Security.
The app needs no entitlements; `packaging/macos/entitlements.plist` says why. The repository, the
Cargo package, the command-line binary and the release file names keep the name bible-rs.

## One-time setup

1. **Signing certificate.** `security find-identity -v -p codesigning` must list
   `Developer ID Application: <your name> (<TEAMID>)`. To sign with another identity, set
   `SIGN_IDENTITY="…"`.
2. **Tools.** Xcode or its command line tools, rustup, and [uv](https://docs.astral.sh/uv/) or
   Python 3.10+. The script adds the two Rust targets. It installs dmgbuild, the tool that lays out
   the DMG, into `target/dmgbuild`, pinned by hash in `packaging/macos/dmgbuild-requirements.txt`.
3. **Notarization credentials.** Apple must notarize the app, or other Macs refuse to open it. The
   script uses a notarytool profile named `bible-notary` from the login keychain. Create it once;
   notarytool prompts for the secret and stores it in the keychain, never in the repository.

   With an Apple ID and an app-specific password, which you create at https://account.apple.com
   under Sign-In and Security:

   ```sh
   xcrun notarytool store-credentials bible-notary --apple-id <apple-id-email> --team-id <TEAMID>
   ```

   Or with an App Store Connect API key with the Developer role, made under Users and Access,
   Integrations, Team Keys:

   ```sh
   xcrun notarytool store-credentials bible-notary --key AuthKey_<KEYID>.p8 --key-id <KEYID> --issuer <issuer-id>
   ```

   Without the profile, `scripts/release-macos.sh` stops before building and prints these commands.
4. **Update signing key.** Installed copies update themselves with
   [Sparkle](https://sparkle-project.org) and install only zips signed with the private half of
   the `SUPublicEDKey` in `scripts/bundle-macos.sh`. That key lives in the login keychain under the
   account `bible-rs` (item "Private key for signing Sparkle updates"); the release script reads
   it there and stops if it is missing or different. Keep a copy somewhere safe, and move it to a
   new Mac the same way:

   ```sh
   S="$(scripts/fetch-sparkle.sh)"
   "$S/bin/generate_keys" --account bible-rs -x bible-rs-sparkle.key   # export
   "$S/bin/generate_keys" --account bible-rs -f bible-rs-sparkle.key   # import on another Mac
   ```

   Never generate a new key for this app: copies already installed would refuse updates signed
   with it.

## Making a release

1. Set the new `version` under `[workspace.package]` in `Cargo.toml`, merge it to `main`, and
   check out a clean `main`.
2. Run `scripts/release-macos.sh`. It builds the universal app, checks its architectures, minimum
   macOS version, Info.plist and signature, runs the signed binary, notarizes and staples the app,
   builds and signs the DMG, notarizes and staples it, zips the app, writes the checksums and
   writes and signs the update feed. It ends by asking Gatekeeper about the app and the DMG; both
   must say
   `source=Notarized Developer ID`. Expect about two minutes, plus a few minutes for each of
   Apple's two notarization checks.
3. Tag the commit and push the tag:

   ```sh
   git tag -a v<version> -m "bible-rs <version>"
   git push origin v<version>
   ```

4. Create the release as a draft, check it, then publish it on GitHub:

   ```sh
   gh release create v<version> --draft --verify-tag --generate-notes --title "Bible <version>" \
     dist/bible-rs-<version>-universal.dmg dist/bible-rs-universal.dmg \
     dist/bible-rs-<version>-universal.zip dist/bible-rs-<version>-SHA256SUMS.txt dist/appcast.xml
   ```

5. **Windows** (once installers are signed, WINDOWS.md): attach the installer and its checksum list
   from the release workflow's *windows* job, so installed Windows copies can update themselves:

   ```sh
   gh run download <run id> -n bible-rs-v<version>-windows-x64 -D dist
   gh release upload v<version> dist/bible-rs-<version>-windows-x64-setup.exe \
     dist/bible-rs-<version>-windows-x64-SHA256SUMS.txt
   ```

   The app installs only an installer whose SHA-256 matches GitHub's digest for the file and this
   checksum list (WINDOWS.md, "Updates").

The fork publishes the DMG to `https://github.com/trentster/lamplit/releases/latest/download/`,
and that is the only place a download comes from. **This build has no updater**: the feed, the
appcast and the in-app check are gone, because a Mac copy and an App Store copy of the same app
must not both claim the right to replace themselves. Every release is a fresh download, announced
wherever it is announced.

## Notes

- **Keychain.** API keys stay in the login Keychain. Each item trusts the app's designated
  requirement (bundle ID `au.com.onyxit.lamplit.mac`, team `<TEAMID>`), so every release signed this way
  reads them without a prompt. Changing the bundle ID or the signing team makes every user approve
  access once. A key saved by an ad-hoc signed development build asks for approval once, the first
  time a release build reads it.
- **Ad-hoc builds.** The hardened runtime loads only frameworks signed by the app's own team, so an
  ad-hoc signed build (CI, `--ad-hoc`) can't load Sparkle; its Check for Updates… says in the app
  that this build can't update itself.
- **Rejected notarization.** `scripts/notarize-macos.sh` prints Apple's log, which names each
  file that failed and why.
- **Intel testing.** On an Apple silicon Mac without Rosetta, the script checks the x86_64 slice's
  architecture and minimum macOS version but cannot run it. `softwareupdate --install-rosetta`
  lets it run that slice too.
