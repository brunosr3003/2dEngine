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
| 2 | comments and doc comments | in progress — `ladder` done (78 blocks); tool at `tools/translate_comments.py` |
| 3 | `docs/*.md` | not started |
| 4 | locale inversion (English native, Portuguese as translation) + DB name migration | analysed; step 1 done, step 2 blocked |

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
3. **A batch file is single-use.** `apply` rewrites the lines it was built
   from, so the line numbers in that JSON are stale the moment it runs.
   Re-extract before translating the next chunk; never add translations to a
   batch that has already been applied. The tool refuses the second apply
   (it checks each block is still the text it extracted) rather than
   corrupting the file, but the refusal aborts partway through the run.
4. **Compile between batches.** A rename that does not compile is a rename that
   silently changed meaning somewhere else.
5. **Game-content literals stay; developer literals go.** A string the player
   reads is keyed by the translation tables and only moves in pass 4. An
   `assert!` message, a `panic!` or a `tracing` line is read by us, not by a
   player, and is translated with the code around it.
6. **Never rename a database column or value in passes 1–3.** Persistence reads
   columns by name, and a column that disappears is a kick at login.
7. **Keep `Danca`, `Muralha`, `Saque` and the other skill names as they are**
   until pass 4 decides what the English game calls them — they are content,
   not code.

## Pass 4: the locale inversion

Today `Idioma::Pt` is not just the default, it is the **original**: with `Pt`,
`tr()` is the identity and the dictionary is never even built
(`idioma.rs`). Source literals are Portuguese, and `idioma/en/*` maps
PT -> EN in 2,307 pairs. "English native" means turning that around.

### What was measured

**The inversion is 99% mechanical.** Of the 2,307 Portuguese keys, 2,289
appear verbatim as quoted literals in the source, so the dictionary is itself
the map for rewriting them. The 18 that do not are strings assembled at
runtime or coming from the database.

**Replacing a quoted token is safe.** `"Voltar"` includes its quotes, and
`"Voltar agora"` does not contain that token, so there is no substring
hazard between entries.

**The database hazard is narrower than it looks.** `nome_do_item` is
id -> name with no reverse lookup, NPC names are `&'static str` in `vila.rs`
rather than rows, and the only `n.nome == "..."` comparison is inside a test.
What must never be touched is `characters.name`: it is the PRIMARY KEY and
`proficiencies.character_name` is a FOREIGN KEY onto it. The seeded name
columns are `items`, `enemy_kinds` and `vendor_shops`.

### Step 1 — done

Inverting makes values into keys, and 17 English strings had more than one
Portuguese source. Nine were imprecise English and are now fixed (Mount/Ride,
Upgrade/Improve, Complete/Turn in, Receive/Claim, Create/Craft,
Mounted spd.). That is an improvement on its own terms.

### Step 2 — blocked, and it needs a decision first

**Ten collisions cannot be fixed by better English**, because English carries
no gender and no adjective plural:

Option 2 was chosen: **the grade words keep their agreement, the rest
collapse.** `idioma::tr_f` (and `tr_f_em`) ask for the feminine form of a
word English has only one of. The table is consulted only in Portuguese, so
English never sees the request and no marker can leak into it.

After step 2, these call sites must ask for the feminine form, because the
noun beside them is feminine (*chave*):

  * `client/src/dungeon_ui.rs` — "Chave de craft {cor}"
  * `client/src/dungeon_recompensas.rs` — the same colour, in the rewards list

and these must NOT, because theirs is masculine (*item*): `forja::Grau`,
`oficina_ui::nome_da_cor`, `pets`, `skills`.

The other seven collapses stay collapsed — they are labels where one form
reads fine:

| English | Portuguese forms | kept on inversion |
|---|---|---|
| Purple | Roxa / Roxo | both, via `tr_f` |
| Epic | Épica / Épico | both, via `tr_f` |
| Legendary | Lendária / Lendário | both, via `tr_f` |
| All | Tudo / Todas / Todos | Todas |
| Completed | Concluída / Concluídas | Concluída |
| Available | Disponível / Disponíveis | Disponível |
| Banker | Banqueira / Banqueiro | Banqueira |
| Go to | Ir para / Ir até | Ir para |
| `{} · yours {}` | seu / sua | seu |
| `{} Energy` | Energia / de Energia | de Energia |

With English as the source, `tr_pt("Purple")` can only return one of them, so
Portuguese loses agreement in a handful of places ("Chave Roxo" instead of
"Chave Roxa"). Keeping both would need the English side to carry a
disambiguating key that the player never sees — worth doing for the item
colours, which are the visible ones, and probably not worth it for the rest.

### The remaining steps

2. rewrite the 2,289 Portuguese literals in the source to English, using the
   dictionary as the map (**blocked: the sweep touches every crate at once**);
3. flip `idioma/en/*` to `idioma/pt/*` with the pairs swapped;
4. make `Idioma::En` the default and the identity, `Pt` the dictionary path;
5. migrate the seeded names in `items`, `enemy_kinds` and `vendor_shops`, as
   an `economy_migrations` entry that only touches rows still holding the old
   seed value — and never `characters`.
