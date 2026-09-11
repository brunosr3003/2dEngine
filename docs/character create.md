# Character creation: what to model, and at what size

> The modeling sheet: measurements, names and file rules. The same content in
> Portuguese is `docs/ARTE_DO_PERSONAGEM.md`; the decisions and the reasoning
> behind them are in `docs/PERSONAGEM.md`.

**Start from the template:** `tools/moldes/corpo_molde.vox`. It is the body
built from boxes, already at the right measurements, with the ten parts
separated and named and each cut in a different color. Open it in MagicaVoxel
and sculpt on top. Names, canvas and joint positions are already correct —
and those are exactly the three things the game cannot guess. It is generated
by `tools/voxrender/molde_corpo.py`; the measurements live there and here.

> **Not yet verified in MagicaVoxel.** The template was checked with the
> project's own `.vox` reader (10 named objects, identical canvases), but it
> has not been opened in MagicaVoxel itself. If it doesn't open, or the
> objects show up without names, the fix goes in the generator — not in your
> work.

## What was decided

| decision | choice |
|---|---|
| proportion | **stylized 1:5** — head ~1/5 of the height, hands and weapons slightly bigger than real |
| limbs | **split** — ten rigid parts (upper arm / forearm, thigh / shin) |
| look | **purchased skins** — armor skin (the whole outfit) and weapon skin (one per weapon set). The equipped item does NOT change the look |
| default look | **the pirate** — whoever bought nothing wears it |
| tier | still readable (proposal): the trim color (slots 241–244) comes from the **equipped item**, not from the skin |
| body | **one body**; customization is the head (face, hair, skin tone) |
| mobs | creatures **and** humanoids |

Why split limbs: one terrain block is 0.5 u — about **12 character voxels,
60% of the leg** — and the character climbs one-block steps all the time. Only
with a knee can the code plant each foot on its own step (two-bone IK, pure
math, no animator). The same knee gives the landing crouch and the bent legs
of a rider; the elbow lets the code aim the pistols at the target from any
direction.

## Rules for every file

1. **1 voxel = 4 cm** (0.04 u). The body is **42 voxels** tall = 1.68 u.
2. **Front = +Y. Up = +Z. Floor at z = 0.**
3. **The character's RIGHT is the higher-X side.** Looking at its face, its
   right hand is on YOUR left — like with a person.
4. **Everything that is WORN uses the same canvas: 32 × 24 × 48** — body,
   armor, face, hair, cloak, holster, scabbard. Every object in the file has
   the same size and sits **in the same World position**. Don't move objects;
   only edit inside them. That is what makes one piece fit another with no
   adjustment.
5. **The object name is a contract** (lists below). An object with an unknown
   name is ignored, and the game reports which one.
6. **Every voxel goes in the object of the part it belongs to.** An arm voxel
   inside the `torso` object will not rotate with the arm.
7. **Reserved palette slots** — use them only for this:

| slots | for | the game |
|---|---|---|
| **241–244** | details that change with the TIER (trims, gems, edges), light to dark | swaps them for gray, green, blue or purple |
| **245–248** | hair, light to dark | recolors hair with no new model |
| **249–252** | skin, light to dark | swaps them for the chosen tone |
| **255** | marker (magenta) | reads its position and **deletes** the voxel |
| everything else | free | left as is |

Whatever color you paint in slots 241–252 is only a placeholder: the game
replaces it.

## The body — `corpo.vox`, ten objects

Coordinates are in voxels: voxel 21 spans from 21.0 to 22.0. The boxes are
the template's; the pivot is the point the part rotates around.

| object | what it is | x | y | z | size | rotates around |
|---|---|---|---|---|---|---|
| `cabeca` | neck + head | 12–19 | 8–15 | 33–41 | 8×8×8 (+ neck 4×4×1) | neck (16, 12, 33) |
| `torso` | chest + hips | 11–20 | 9–14 | 20–32 | 10×6×13 | — (the root) |
| `braco_d` | right upper arm | 21–24 | 10–13 | 25–32 | 4×4×8 | shoulder (23, 12, 31) |
| `antebraco_d` | right forearm + hand | 21–24 | 10–13 | 16–24 | 4×4×9, hand at z 16–19 | elbow (23, 12, 25) |
| `braco_e` | left upper arm | 7–10 | 10–13 | 25–32 | 4×4×8 | shoulder (9, 12, 31) |
| `antebraco_e` | left forearm + hand | 7–10 | 10–13 | 16–24 | 4×4×9 | elbow (9, 12, 25) |
| `coxa_d` | right thigh | 16–19 | 10–13 | 10–19 | 4×4×10 | hip (18, 12, 19) |
| `canela_d` | right shin + foot | 16–19 | 10–15 | 0–9 | 4×4×8, foot 4×6×2 | knee (18, 12, 10) |
| `coxa_e` | left thigh | 12–15 | 10–13 | 10–19 | 4×4×10 | hip (14, 12, 19) |
| `canela_e` | left shin + foot | 12–15 | 10–15 | 0–9 | 4×4×8, foot 4×6×2 | knee (14, 12, 10) |

```
   proportion 1:5                       front view, in voxels
   head       8   ─┐ 1/5                shoulders: 18 wide
   neck       1    │                    (= collision diameter, 0.70 u)
   torso     13    │                    hands: at mid-thigh
   legs      20   ─┘ ~half              feet: 2 voxels ahead of the shin
   total     42 = 1.68 u
```

**Rest pose: arms STRAIGHT down, against the torso.** Not the T-pose of
today's model. The rest pose is the standing pose — with it, the game rotates
nothing while the character stands still.

### What you can and cannot change

- **You can:** round, slim, add volume and clothing detail — up to **2
  voxels** outside each part's box.
- **You cannot:** move the joints, change the total height, or change limb
  lengths. Planting feet on steps and the 74 poses depend on those numbers:
  move a knee up one voxel and the foot starts sinking into the step.

### The joint cap

At the elbow and the knee, the LOWER part rises two voxels, inset by one on
each side, into the upper part (in the template: a 2×2×2 block at z 25–26 in
the forearm and at z 10–11 in the shin). At rest it is hidden inside; when the
joint bends, it is what covers the hole that would open there. **Keep the cap**
while sculpting — it is the only thing in the file that never shows and is
still required.

## The head

| file | object | where | rules |
|---|---|---|---|
| `rosto_01.vox`… | `cabeca` | the same box as the body's head | **replaces** the body's head; eyes on the +Y face; skin in 249–252 |
| `cabelo_01.vox`… | `cabelo` | up to 2 voxels past the head (x 10–21, y 6–17, z 32–43); a ponytail or braid may go down to z 28 at the back. A **hat** may go up to 3 past the head on each side and up to the canvas top (x 9–22, y 5–18, z ≤ 47) | sits **on top of** the head; color in 245–248. A hat's band must be **one voxel wider than the head** all around, so its faces never sit on the head's faces |

Proposed for the playtest: **4 faces, 6 hair styles.**

## Armor skins — `assets/vox/personagem/skins/armadura/<name>.vox`

An armor skin is **the whole outfit**, bought in the store. Equipped armor
(its weight, its tier) does not change the look.

- **The same ten objects, with the same names as the body.** Each object
  **replaces the whole body part** — it is not a shell on top. The hand stays
  in the forearm, in skin (249–252).
- **How to make one:** duplicate `corpo.vox` and sculpt the outfit on top,
  part by part. The joints and caps come along.
- **How much it can grow:** up to +2 voxels past each part's box; shoulder
  pads up to +3.
- **Trims in 241–244.** Their color comes from the tier of the item the
  player has EQUIPPED — the skin decides the shape, the item decides the
  color. One skin serves all four tiers.
- **Head:** a skin may add a `capuz` object (hood — replaces the hair; the
  face shows) or an `elmo` object (helmet — replaces the hair and covers the
  face).
- **The pirate (`corpo.vox`) is the default outfit.** No skin is required to
  play; each new one is store content, made when it will be sold.

## Worn items — same canvas, one object each

| file | object | where it sits | attached to |
|---|---|---|---|
| `manto_guerreiro.vox` | `manto` | behind the torso (y 7–8), from the shoulder line (z 32) to the knee (z 10) | torso; pivots at (16, 8, 32) — it sways on its own |
| `manto_mago.vox` | `manto` | same, down to the ankle (z 2); may have a lowered hood | torso |
| `coldre.vox` | `coldre` | belt at the hips (z 19–23) with one holster on each side, behind the arm (y 8–9); **don't go below z 19**, or the thigh passes through it while walking | torso |
| `bainha.vox` | `bainha` | left hip, angled: mouth forward and up (y 15, z 22), tip back and down (y 4, z 16), outside the thigh (x 8–9) | torso |

## Weapon skins — any canvas, one object

A weapon skin belongs to **one weapon set** — it changes the weapon's look,
never the set: the animations come from the set, so a pistol user always
draws pistols. Each set has a **default skin** (the files below); bought ones
go in `assets/vox/personagem/skins/arma/<set>/<name>.vox`.

**Orientation: the way it sits in the hand with the arm hanging down.** Handle
along Y, blade or barrel pointing **+Y** (forward).

**One magenta voxel (slot 255) at the center of the grip** — where the hand
closes. The game reads that point as the hand attachment and deletes the
voxel.

| file | size (Y × Z × X) | details |
|---|---|---|
| `espada.vox` (sword) | 26 × 7 × 3 — handle 6, guard 1 (7 wide), blade 19 | marker at the middle of the handle |
| `escudo.vox` (shield) | 14 × 14 × 3 | face pointing out (−X, it goes in the left hand); marker on the back strap |
| `katana.vox` | 30 × 3 × 2 — handle 8, blade 22 | marker at the middle of the handle |
| `pistola.vox` (pistol) | 11 × 6 × 2 — vertical grip, barrel to +Y | marker at the middle of the grip; **the same model is used in both hands** |

Weapons are **1.25× real size** on purpose: seen from above on a phone
screen, a real-size weapon becomes a line. Tier trims in 241–244.

**The ring is not modeled** — from 27°–75° above, it is one voxel. The
proposal is a glow or rune over the hand, done in code (still open).

## What NOT to model

- **Accessories** (earring, amulet, bracelet, belt): they don't show on the
  body.
- **Creatures and mounts:** they come from the zone14 stock, already split
  into parts.

## Deliverables, in order

| # | files | unlocks |
|---|---|---|
| 1 | `corpo.vox` — **done: the pirate** | the rig with the real body |
| 2 | `rosto_01`, `cabelo_01` — **`cabelo_01` done: the tricorn**; the pirate's face lives in `corpo.vox` | the head |
| 3 | `espada`, `escudo`, `manto_guerreiro` — default skins of the first set | the first weapon set and the first attack |
| 4 | `katana`, `bainha`, `pistola`, `coldre`, `manto_mago` — default skins | the other three sets |
| 5 | armor skins | store content — made as they are sold, **not required to play** |
| 6 | `rosto_02–04`, `cabelo_02–06` | character creation |

**19 files to play** (the armor skins are store content, not a prerequisite). Save them in `assets/vox/personagem/`.

## The first character: the pirate

`assets/vox/personagem/corpo.vox` + `cabelo_01.vox`, generated by
`tools/voxrender/pirata.py`. The old `pirate_captain.vox` was one piece in a
T-pose, thinner and with a smaller head than this sheet (torso 8×4, legs 3×3,
head 1:6) — copying it voxel by voxel would break the joints. So it was
**rebuilt at these measurements, keeping what makes it the pirate**: the same
palette, white shirt with a grey back, red sash, belt with a gold buckle,
black trousers, brown boots, eye patch, one blue eye, a beard framing the
face, a ponytail with a red ribbon — and the black tricorn with the gold brim
and the pink feather.

Two changes from the original, both for the top-down camera:

- **The tricorn is a triangle seen from above** (point in front, two corners
  behind), with the brim turned up on the three sides and not at the
  corners. A square brim turned up all around read as a picture frame.
- **The hair and beard are a lighter grey** — with the original (60,60,60)
  under a black hat, the head became a dark block with a small window of
  face. It lives in the hair slots, so changing it costs nothing.

## How to check your work

Until the part importer exists (step 1 of `docs/PERSONAGEM.md`), the way to
see a file is the command-line render:

```sh
python3 tools/voxrender/voxrender.py assets/vox/personagem/corpo.vox -o corpo.png -d 4 -s 6
```

Once the importer is in, it checks everything that can be checked on its own
— unknown object name, canvas of a different size, voxel outside its part's
range, missing marker on a weapon — and says which file and which object.

## Still open

1. Weapon **always in hand** or **put away out of combat** (the katana in its
   scabbard is the set's identity)?
2. Does **running** exist? (the protocol has a `SPRINT` bit nothing uses)
3. **Who makes the final art** — you in MagicaVoxel, an artist, another source?
4. Which of the **8 mob types** are creatures and which are humanoids, and in
   which areas.
5. The **ring's visual** — a glow or rune on the hand?
6. How many **head variations**, and **which mounts** come first.
7. Skins bought with what — real money, premium currency, in-game gold? Per
   account or per character?
8. Confirm the proposal: **trim color from the equipped item's tier**, and
   **armor weight as an icon on the nameplate**.
