# Mac: signing, notarization, and the `xattr`

## The problem

Every file a **browser** downloads gets the extended attribute
`com.apple.quarantine`. A quarantined app that Apple has not notarized will not
open: on macOS Sequoia not even through right-click → Open, only through
**System Settings → Privacy & Security → Open Anyway**. The manual escape is

```
xattr -cr /path/to/Tempest.app
```

which deletes the attribute — and it comes back on the next download. That was
the step the owner had to repeat for every release.

**Ad-hoc signing does not fix it.** `codesign --sign -` (what the build did
until 28/09/2026) only makes the binary *run* on Apple Silicon, which demands a
signature even an anonymous one. It says nothing about who published the app, so
Gatekeeper keeps blocking the download. What clears quarantine out of the way is
the **notarization ticket** stapled into the `.app`.

## The solution: Developer ID + notarization + staple

`scripts/build-mac.sh` does this on its own **if** a `Developer ID Application`
identity exists in the login keychain:

1. `codesign --options runtime --timestamp --sign "Developer ID Application: …"`
   — hardened runtime and a timestamp are both required for notarization.
2. `xcrun notarytool submit … --wait` — uploads the zip and waits for the
   verdict. It uses the same App Store Connect key `build-ios.sh` already uses
   (`ASC_API_KEY_ID`/`ASC_API_ISSUER_ID` plus
   `~/.appstoreconnect/private_keys/AuthKey_<id>.p8`), falling back to
   `APPLE_ID`/`APPLE_APP_PASSWORD`.
3. `xcrun stapler staple Tempest.app` — staples the ticket **into the app**, not
   into the zip. That is why the final zip is built after the staple.
4. `spctl --assess --type execute` — the proof: it is the same answer Gatekeeper
   will give on the machine that downloaded the app.

Without the identity the script falls back to ad-hoc and **warns** on stderr,
because a build without notarization is a build that will ask for `xattr` on the
other end.

## Done (28/09/2026)

Certificate created and installed: `Developer ID Application: Bruno Soares Reis
(294S2R54ZP)`. The first submission took about **1h50** — Apple is slow with a
Developer ID it has never seen; later ones take minutes. Result:

```
spctl --assess --type execute --verbose=2 Tempest.app
  Tempest.app: accepted
  source=Notarized Developer ID

xcrun stapler validate Tempest.app
  The validate action worked!

codesign -dv Tempest.app
  flags=0x10000(runtime)
  Authority=Developer ID Application: Bruno Soares Reis (294S2R54ZP)
  Authority=Developer ID Certification Authority
  Authority=Apple Root CA
```

Two stumbles on the way, both already solved in `build-ios.sh` and not reused
here at first:

- `security import` of a `.p12` with an **empty password** fails with "MAC
  verification failed during PKCS12 import (wrong password?)". The password is
  now random and the file is deleted in the `finally`.
- `codesign` died with **`errSecInternalComponent`**. Two things were missing:
  over SSH the login keychain is locked (`security unlock-keychain`), and the
  `-T` on import only edits the ACL — what authorizes `codesign` to *use* the
  key without a password dialog is `security set-key-partition-list`.

And one macOS trap that is not the project's fault: **TCC** stops `sshd` from
reading `~/Downloads`. You can see the file exists (`test -f` succeeds) and you
cannot open or move it — `openssl` and `mv` both get `Operation not permitted`.
The `.cer` had to be moved out of there from the Mac's own Terminal.

## The certificate (one time only)

The Mac keychain already had `Apple Development` and `Apple Distribution`; both
are App Store/TestFlight certificates. Distributing **outside** the App Store
needs a different type, `Developer ID Application`, which a paid Developer
Program account is entitled to and which has to be created once:

1. `ssh mac 'cd ~/2dEngine-release && python3 scripts/cert-developer-id.py --csr'`
   — generates the private key and the CSR under
   `~/.appstoreconnect/developer-id/`. The key **never leaves the Mac**.
2. On the Mac, at `developer.apple.com/account/resources/certificates/add` →
   **Developer ID Application** → upload `developer-id.csr` → download the
   `.cer`.
3. `python3 scripts/cert-developer-id.py --instalar ~/Downloads/<file>.cer` —
   joins the certificate with the private key into a `.p12`, imports it into the
   login keychain and authorizes `codesign` to use it without prompting.

**The API cannot do it.** Tried on 28/09/2026: `POST /v1/certificates` with
`certificateType: DEVELOPER_ID_APPLICATION` answers
`403 FORBIDDEN_ERROR — This operation can only be performed by the Account
Holder`, and an API key cannot hold that role. Step 2 belongs to a browser
logged in as the account owner; there is no way around it.

Worth knowing beforehand: the limit is **two** active Developer ID certificates
per account, and the private key exists only where the CSR was born — losing it
means creating another. It is worth backing up
`~/.appstoreconnect/developer-id/`.

## If there is ever no certificate

Downloading with `curl` does **not** set quarantine — the browser does. So this
opens on the first try even on an ad-hoc build:

```
curl -L -o ~/Downloads/Tempest.zip https://mmo.brunji.com.br/downloads/MMORPG-Mac.zip
unzip -o ~/Downloads/Tempest.zip -d ~/Applications
open ~/Applications/Tempest.app
```
