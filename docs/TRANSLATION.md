# Translating Tempest to English

The repo is public and English is now the language of the code. This file is
the glossary the translation follows, and the record of how far it has got.
Every rename in the codebase uses the term on the right — if a word is missing
here, add it here first, so the same Portuguese word never becomes two
different English ones in two different files.

## Status

| pass | what | state |
|---|---|---|
| 1 | identifiers: files, modules, functions, fields, variables | in progress — `ladder` done |
| 2 | comments and doc comments | not started |
| 3 | `docs/*.md` | not started |
| 4 | locale inversion (English native, Portuguese as translation) + DB name migration | not started |

Passes 1–3 change no behaviour: no string literal moves, no protocol change.
Pass 4 is the risky one and is planned separately, because item and mob names
come out of Postgres in Portuguese (`idioma/en/dados.rs`, "o nome que vem do
banco") and have to migrate with the code.

## Glossary

### The domain

| pt | en | note |
|---|---|---|
| conjunto | `weapon_set` / `WeaponSet` | the weapon + offhand pair that defines the class |
| escada | `ladder` | the per-level stat ruler (`docs/ESCADA.md`) |
| faixa | `tier` | the level band a character or item sits in |
| nivel | `level` | |
| chefe | `boss` | |
| bicho / mob | `creature` / `mob` | `mob` stays, it is already English in use |
| matilha | `pack` | the group that aggros together |
| horda | `horde` | |
| zona | `zone` | |
| ilha | `island` | |
| sitio | `site` | a flat spot where a mob can spawn |
| vaga | `slot` | |
| jogador | `player` | |
| inimigo | `enemy` | |
| personagem | `character` | |
| sessao | `session` | |
| canal | `channel` | |
| realm | `realm` | already English |

### Combat

| pt | en | note |
|---|---|---|
| golpe | `strike` | one swing; `hit` is the thing that lands |
| ataque | `attack` | |
| dano | `damage` | |
| vida | `health` | `hp` where it is the number |
| defesa | `defense` | |
| armadura | `armor` | |
| peso | `weight` | armor weight class |
| alcance | `range` | |
| raio | `radius` | |
| cadencia | `rate` / `cooldown` | swing interval |
| espera / recarga | `cooldown` | |
| conjuracao | `cast` | |
| impacto | `impact` | |
| habilidade | `skill` | |
| destravada | `unlocked` | |
| pocao | `potion` | |
| abate | `kill` | |
| agressor | `aggressor` | |
| provocado | `provoked` | |
| esquiva | `dodge` | |
| telegrafado | `telegraphed` | |
| abertura | `opener` | katana: first strike on an unaware target |
| execucao | `execute` | katana: strike below the health threshold |
| sede | `thirst` | katana: the lifesteal window Danca opens |
| roubo de vida | `lifesteal` | |
| piso | `floor` | a minimum, as in HP floor |
| teto | `cap` | a maximum |
| folga | `tolerance` | slack allowed by a guard |

### Systems

| pt | en |
|---|---|
| balanceamento | `balance` |
| simular | `simulate` |
| duelar | `duel` |
| meta | `goal` |
| guarda | `guard` (a test that holds an invariant) |
| bolsa | `bag` |
| loja | `shop` |
| mercado | `market` |
| oficina | `workshop` |
| forja | `forge` |
| receita | `recipe` |
| craft | `craft` |
| refino | `refinement` |
| grau | `grade` |
| peca | `piece` |
| montaria | `mount` |
| acessorio | `accessory` |
| guarda-roupa | `wardrobe` |
| aparencia | `appearance` |
| colonia | `colony` |
| historia | `story` |
| missao / quest | `quest` |
| idioma | `language` |
| terreno | `terrain` |
| mapa | `map` |
| atualizacao | `update` |

### Everyday words

| pt | en |
|---|---|
| nome | `name` |
| descricao | `description` |
| centro | `center` |
| alvo | `target` |
| cor | `color` |
| lado | `side` |
| topo | `top` |
| passo | `step` |
| chao | `ground` |
| agora | `now` |
| botao | `button` |
| icone | `icon` |
| estilo | `style` |
| som | `sound` |
| previa | `preview` |
| ficha | `sheet` |
| onde obter | `where_to_get` |
| vivo | `alive` |
| morto | `dead` |
| custo | `cost` |
| total | `total` |

## Rules

1. **`cargo test --no-run`, not `cargo check`.** `check` does not compile
   `#[cfg(test)]` code. Renaming `ladder` left 16 errors that `check` reported
   as zero — the whole balance simulator lives behind `cfg(test)`, and it is
   the thing that would have caught a real behaviour change.
2. **Never rename by variable name.** `m.defesa` looks safe until you notice
   `m` is `ladder::Mob` in one file and the simulator's own `Mob` in another.
   Rename through the type or the module path, and let the compiler find the
   field accesses — it does, reliably.
3. **Compile between batches.** A rename that does not compile is a rename that
   silently changed meaning somewhere else.
4. **Never rename a string literal in passes 1–3.** Literals are game content
   and are keyed by the translation tables; moving them is pass 4.
5. **Never rename a database column or value in passes 1–3.** Persistence reads
   columns by name, and a column that disappears is a kick at login.
6. **Keep `Danca`, `Muralha`, `Saque` and the other skill names as they are**
   until pass 4 decides what the English game calls them — they are content,
   not code.
