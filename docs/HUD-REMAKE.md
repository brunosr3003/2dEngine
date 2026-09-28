# HUD visual refresh (local)

Shared slate surfaces, warm text and muted gold selection accents replace the
older blue glass treatment. Buttons, tabs, cards, item slots, tooltips and
resource bars use the shared style in `hud_estilo.rs`. Action discs and minimap
frames have fewer decorative rings. Item rarity, health, mana and cooldown
colors retain their meaning.

Existing commands, shortcuts, combat rules and server messages are preserved.
Player-panel text is no longer scaled twice. Inventory dimensions fit the
available screen, its tab columns use unscaled counts, and equipment rows no
longer scale their row count. Inventory percentages are independent of UI scale.
Translated button/tab labels are measured against their existing rectangles.

## Offline review

Build with `cargo build -p client`, then run:

```sh
MMO_PREVIA_HUD=1 target/debug/client
```

The desktop debug-only preview writes exploration, combat and inventory PNGs
at 1920×1080, 1280×720 and 960×540 to `/tmp/tempest-hud`.
`MMO_PREVIA_SAIDA` overrides that directory. `MMO_PREVIA_HUD_FUNDO` optionally
loads a local scene screenshot behind the real UI widgets. The preview uses
sample data, connects to no server, and is excluded from mobile/release builds.
It is a visual review harness, not an end-to-end gameplay test.

Crafting can be reviewed with `MMO_PREVIA_OFICINA=1`.
Validation: `cargo test -p client` (499 passed, 1 ignored).

No protocol/build bump, release packaging or production deployment is included.
