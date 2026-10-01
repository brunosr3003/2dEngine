# Bestiary by island

Every island has its own common mobs, and no common mob spawns on two
islands (`economy::testes_do_bestiario::no_mob_spawns_on_two_islands`).

The Morganeers (Gunman, Mage, Archer) follow the story from island to
island, so they appear on each one in that island's own version.

## Rosters

Weakest first. `kind_para_nivel_em` opens the list from the start as the
level rises, so the order matters.

| Island | Common mobs (kind) | Beach |
|---|---|---|
| Bosque | Wolf 0, Bear 1, Gunman 2, Tiger 3, Mage 4, Owlbear 5, Archer 6 | Crab 8, King Crab 9 |
| Glacier | White Tiger 12, **Frostcoat Archer 30**, White Bear 11, **Frostcoat Mage 31**, Walrus 10, **Snow Owlbear 32** | Walrus 10 |
| Waste | **Dune Raider 33**, Scarab 13, **Sand Archer 34**, **Sun Mage 35**, Scarab Queen 14 | Scarab 13 |
| Plateau | **Crag Lynx 36**, **Cliff Archer 37**, **Cave Bear 38**, **Storm Mage 39**, **Storm Owlbear 40**, Rockback 15 | Crag Lynx 36 |

The Bosque roster is unchanged on purpose: `balanceamento::metas_do_inicio`
is calibrated against exactly those seven mobs.

## Variants

A variant (bold above, `shared::bestiary::VARIANTS`) is a new kind with its
own name and model, tied to the **species** it varies on:

- **Numbers.** The variant copies its species' row in
  `economy::KINDS_INICIAIS` (`economy::todos_os_kinds`). It changes the look,
  not the fight, so the ladder and the balance sim measure the same mob.
  Guarded by `variants_fight_like_their_species`.
- **Quests.** A kill quest for a species counts all of its variants
  (`quests::kill_conta`), and the quest map points at any of them. "Archers of
  the blizzard" on the Glacier is the Frostcoat Archer.
- **AI and client.** Anything keyed on the species (the mage's magic bolt, the
  archer's draw pose, attack sounds, map markers) goes through
  `bestiary::species_of`.
- **Loot.** Each variant keeps its species' materials, with copper and potions
  at its island's tier (`loot_mobs::BASE_VARIANTES`, migration
  `variantes_por_ilha_v1`).
- **Art.**
  - Human variants are new outfits on the Morganeer body
    (`tools/voxrender/humanoides.py`): `assets/vox/humanoides/<name>.vox` for
    the rig, plus `assets/vox/<name>.vox` as the whole model.
  - Beast variants are the species' mesh with a palette swapped by index
    (`tools/voxrender/bichos.py`, `PELAGENS`). The swap leaves eyes, claws and
    stripes alone.

Variant kinds start at 30. Kinds 10–19 are shared with the boss catalogue,
and the retired sea mobs used 20–24.

## Adding a variant

1. Add a row to `shared::bestiary::VARIANTS` (new kind, species, name, zone).
2. Put the kind in its island's slot in `economy::kinds_do_bioma`.
3. Add its loot to `loot_mobs::BASE_VARIANTES`. A new kind on a database that
   already ran `variantes_por_ilha_v1` needs a new migration name.
4. Art:
   - human: add an entry to `humanoides.py` `MOBS`, then to
     `render3d::RIGS_DE_GENTE` and `MODELOS_DE_GENTE`;
   - beast: add a `PELAGENS` entry and a `BICHOS` row in `bichos.py`, then to
     `bicho::BICHOS` and `bicho::modelo_de_kind`.
5. Add the Portuguese name to `idioma/pt/dados.rs`.
6. Render `MMO_PREVIA_BESTIARY=1` and look at the PNG.
