# Publishing Tempest

The user asked that game updates be built and published to the site as part of
the delivery, without asking for permission to publish again.
Update Windows, Android, Linux, Mac and iOS/TestFlight through the existing
scripts; record any blocked platform without announcing that it was published.

Before packaging, update `docs/PATCHNOTES.txt`, bump `BUILD` in
`crates/client/src/atualizacao.rs` and run `scripts/check-release-notes.py --seal`.

Patch notes shown in game (`docs/PATCHNOTES.txt`, the "What's New in Tempest"
panel) are ALWAYS in English, never Portuguese, and stay player-facing and
generic: one entry per update day, short bullets such as new region, new boss,
new dungeon, new system, balance changes, bug fixes. No internal detail (shaders,
JNI, protocol numbers, file names). The detailed technical history goes in
`docs/CHANGELOG.md` (Portuguese, for the team).

Publish the packages before the `downloads/releases.json` manifest. Each platform
entry must record the `build` that is actually available and its `update_url`.
Copy the same manifest to `WEB_STATIC/releases.json` on the production API:
`/api/client-release` is what clients query before logging in.
Desktop opens the site; mobile opens the store/TestFlight. While Android is
distributed as an APK, keep the APK link, without pointing at a store that does
not exist. Check the hashes of the public downloads and keep a copy of the
previous version.
