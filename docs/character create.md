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
| armor weight | **per set** — three full-body silhouettes (light, medium, heavy) |
| tier | **a color, not a model** — gray, green, blue, purple via palette swap |
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
| `cabelo_01.vox`… | `cabelo` | up to 2 voxels past the head (x 10–21, y 6–17, z 32–43); a ponytail or braid may go down to z 28 at the back | sits **on top of** the head; color in 245–248 |

Proposed for the playtest: **4 faces, 6 hair styles.**

## Armor — `armadura_leve.vox`, `armadura_media.vox`, `armadura_pesada.vox`

- **The same ten objects, with the same names.** Each object **replaces the
  whole body part** — it is not a shell on top. The hand stays in the
  forearm, in skin (249–252).
- **How to make it:** duplicate `corpo.vox` and sculpt the armor on top, part
  by part. The joints and caps come along.
- **How much it can grow:** light is almost the body; medium up to +1 voxel;
  heavy up to +2, shoulder pads up to +3.

| weight | head | torso | arms | legs | silhouette |
|---|---|---|---|---|---|
| **light** | none (hair shows) | tight leather | bracers | trousers | slim |
| **medium** | hood or circlet | doublet | small pauldron | shin guards | medium |
| **heavy** | helmet (hides hair) | plate | wide pauldron | greaves | wide, square |

- **Trims that change with the tier:** 241–244. One armor file serves all
  four tiers.
- **Head:** light has none. Medium has a `capuz` object (hood — replaces the
  hair; the face shows). Heavy has an `elmo` object (helmet — replaces the
  hair and covers the face).

## Worn items — same canvas, one object each

| file | object | where it sits | attached to |
|---|---|---|---|
| `manto_guerreiro.vox` | `manto` | behind the torso (y 7–8), from the shoulder line (z 32) to the knee (z 10) | torso; pivots at (16, 8, 32) — it sways on its own |
| `manto_mago.vox` | `manto` | same, down to the ankle (z 2); may have a lowered hood | torso |
| `coldre.vox` | `coldre` | belt at the hips (z 19–23) with one holster on each side, behind the arm (y 8–9); **don't go below z 19**, or the thigh passes through it while walking | torso |
| `bainha.vox` | `bainha` | left hip, angled: mouth forward and up (y 15, z 22), tip back and down (y 4, z 16), outside the thigh (x 8–9) | torso |

## Hand weapons — any canvas, one object

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
| 1 | `corpo.vox` | the rig with the real body (until then the game uses the template) |
| 2 | `rosto_01`, `cabelo_01` | the head |
| 3 | `espada`, `escudo`, `manto_guerreiro` | the first weapon set and the first attack |
| 4 | `katana`, `bainha`, `pistola`, `coldre`, `manto_mago` | the other three sets |
| 5 | `armadura_leve`, `_media`, `_pesada` | armor weight |
| 6 | `rosto_02–04`, `cabelo_02–06` | character creation |

**22 files in total.** Save them in `assets/vox/personagem/`.

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
