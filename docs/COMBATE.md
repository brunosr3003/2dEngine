# Combate: o desenho do playtest

As regras atuais de skills estão em [SKILLS.md](SKILLS.md). Ver
[PERSONAGEM.md](PERSONAGEM.md) para o rig e a animação.

## A arma é um CONJUNTO, não uma peça

A arma é o conjunto inteiro, e a secundária vem amarrada a ela. A katana é
segurada com as duas mãos, na guarda e nos golpes:

| arma (principal) | secundária | leitura |
|---|---|---|
| **espada e escudo** | manto do guerreiro | linha de frente |
| **katana** | bainha | corte rápido, saque |
| **duas pistolas** | coldre | à distância, pirata |
| **anel mágico** | manto do mago | cura e magia |

Quatro no playtest; outras entram depois.

**Isso resolve sozinho um risco que o offhand livre criava.** Com o offhand
solto, se o escudo fosse a única secundária com número, todo mundo carregaria
escudo e o slot viraria escolha falsa. Amarrando a secundária à arma, a escolha
sai do slot e vai pro conjunto — que é onde ela tem consequência de verdade.

O **anel mágico** no lugar da varinha é decisão de tema: num mundo de
navegação e ilhas, quem cura não anda com um graveto na mão.

## Armadura tem PESO, e o peso é a escolha

Três pesos, e cada um é uma troca declarada:

| peso | dá | cobra |
|---|---|---|
| **leve** | bônus de dano | pouca resistência |
| **média** | equilíbrio | equilíbrio |
| **pesada** | resistência | menos dano |

A armadura deixa de ser "número maior é melhor" e passa a ser uma posição no
eixo dano↔resistência. Combinada com a arma, é ela que faz duas pessoas com a
mesma espada jogarem diferente.

A secundária **é item de verdade**: tem tier e atributo próprios, e melhora
separada da principal. Não é enfeite da arma.

## Acessórios

Quatro, iguais pra todo mundo: **brinco, amuleto, bracelete, cinto**. Não há
variação por classe — o que muda entre dois jogadores é o tier e o que o item
rolou, não a lista de slots.

## Skills

**Três por conjunto de arma, todas ATIVAS.** Doze no playtest.

Desbloqueio automático pelo nível do personagem: **1, 5 e 10**, na ordem de
cada arma. Não há compra com pontos, ranks ou passivas. Cada skill tem gesto
próprio, custo de mana e recarga. Atalhos **1, 2 e 3** ou clique na barra.

## Proficiência

Uma proficiência por conjunto de arma — quatro. O desbloqueio das skills usa
o nível do personagem, independente da proficiência.

## O que ainda falta decidir

1. ~~O peso da armadura é por PEÇA ou por conjunto?~~ **Decidido: por
   conjunto**, como regra de jogo. O VISUAL não sai do item: a aparência é
   skin comprada (ver `docs/PERSONAGEM.md`), e o peso aparece como ícone na
   placa de nome.
2. **Os números.** Quanto a armadura leve dá de dano e tira de resistência, o
   que cada skill custa e cobra de espera. Isso é balanceamento e não trava a
   estrutura — mas trava o playtest.

## Balanceamento

Queixa do playtest: o personagem de corpo a corpo morreu nos mobs iniciais,
enquanto o de distância "não passava nem aperto".

**Como se mede.** `crates/server/src/balanceamento.rs` simula, a 30 Hz e de
forma determinística, um jogador em AUTO COMBATE (e skills em AUTO, pela regra
do cliente) limpando uma zona da própria faixa: 18 vagas no raio de 45,
respawn de 20 s, sorteio de bicho por nível do jogo. Stats, cadência,
mitigação e tabela de mobs saem das MESMAS funções do servidor
(`effective_stats`, `cooldown_do_ataque`, `dano_mitigado`,
`KINDS_INICIAIS`, `kind_para_nivel_em`). Simplifica: chão plano, corpos podem
se sobrepor, tiro de mob sempre acerta, área de skill = mobs a até `raio` do
alvo.

```sh
cargo test -p server --bin server balanceamento -- --nocapture
```

### Antes (golpe básico, sem skills, sem poção)

| Nível | Conjunto | Dano por mob | HP mínimo (10 mobs) |
|---|---|---:|---:|
| 1 | Espada e escudo / Katana | 3,0 / 3,0 | 75% / 70% |
| 1 | Pistolas / Anel | **0,0 / 0,0** | 100% / 100% |
| 5 | Espada e escudo / Katana | 3,0 / 4,0 | 75% / 60% |
| 5 | Pistolas / Anel | **0,0 / 0,0** | 100% / 100% |
| 10 | Espada e escudo / Katana | 6,7 / 7,2 | 45% / 28% |
| 10 | Pistolas / Anel | **0,0 / 0,0** | 100% / 100% |

**Causa, em números:**

1. **O regen de HP não curava.** `(hp + 0,5 × dt) as i32` a 30 Hz soma 0,017
   por tick e trunca pra zero, parado ou não. Quem apanha só desce de HP luta
   após luta, e o melee morria por atrito. Quem atira não apanhava, então nunca
   percebia.
2. **Tudo morria em dois golpes.** Lobo com 50 de HP contra 32–39 de dano, e
   golpe corpo a corpo a cada 0,25 s.
3. **O tiro era intocável.** O mob só reagia a quem entrasse no alcance de
   visão (lobo 9, urso 8). O AUTO de distância para a 5,4 do alvo e mata antes
   de o bicho chegar. Cada tiro ainda congelava o bicho 0,25 s de stagger.

### Metas (asserts em `metas_de_balanceamento`)

Níveis 1, 5 e 10, AUTO, sem poção:

- Todo conjunto limpa 10 mobs vivo.
- HP mínimo ≥ 25% com espada e escudo e ≥ 15% com os outros.
- Quem atira apanha: pelo menos 3 de dano por mob.
- Dano por mob do corpo a corpo no máximo 1,5× o da distância (meta ~1,4×).
- Tempo de luta por abate de cada conjunto a ±20% da média do nível.

### O que mudou

| Onde | Antes | Agora |
|---|---|---|
| Regen de HP | truncado (0/s) | resto acumulado (`regen_de_hp`) |
| Mob golpeado | só reage se vir | provocado 6 s, e a matilha num raio de 16 também |
| Bicho de mordida provocado | velocidade normal | investe a 2,5× |
| Stagger no mob | todo golpe | só golpe corpo a corpo |
| Golpe corpo a corpo | 0,25 s | 0,40 s |
| Tiro das pistolas | 0,55 s, DEX/2 no dano | 0,65 s, DEX/4 |
| Anel (cadência de dano) | 0,32 s | 0,45 s |
| Espada e escudo | +20 de vida | +60 de vida e escudo absorve 40% |
| Katana | — | 12% do golpe volta como vida |
| HP dos mobs comuns | lobo 50, urso 120, pistoleiro 35, tigre 40, mago 45, owlbear 200, arqueiro 45 | 120, 280, 85, 95, 105, 460, 105 |

As constantes estão no topo de `world.rs` (`PROVOCACAO_S`,
`MATILHA_RAIO_UN`, `CARGA_PROVOCADO_MULT`, `REDUCAO_DO_ESCUDO`,
`ROUBO_DE_VIDA_KATANA`, `CADENCIA_DAS_PISTOLAS_S`, `CADENCIA_DO_ANEL_S`). O
HP dos mobs vai ao banco pela migração `balanceamento_hp_mobs_v1`, que só
altera linha com o HP antigo do seed.

### Depois (golpe básico e skills em AUTO, sem poção)

| Nível | Conjunto | s/abate | Dano por mob | HP mínimo |
|---|---|---:|---:|---:|
| 1 | Espada e escudo | 4,05 | 6,0 | 89% |
| 1 | Katana | 3,92 | 10,0 | 90% |
| 1 | Pistolas | 3,28 | 4,0 | 82% |
| 1 | Anel | 3,72 | 10,0 | 81% |
| 5 | Espada e escudo | 4,79 | 17,6 | 46% |
| 5 | Katana | 4,35 | 18,6 | 43% |
| 5 | Pistolas | 4,03 | 8,6 | 59% |
| 5 | Anel | 4,69 | 16,2 | 69% |
| 10 | Espada e escudo | 4,88 | 18,6 | 39% |
| 10 | Katana | 3,94 | 22,9 | 28% |
| 10 | Pistolas | 3,73 | 13,9 | 17% |
| 10 | Anel | 4,25 | 20,6 | 72% |

Razão de dano corpo a corpo × distância: 1,14 (nível 1), 1,46 (nível 5) e
1,20 (nível 10).

**Ainda frágil:** as pistolas no nível 10 terminam com 17% (sem cura própria),
e a razão do nível 5 está no limite do assert. O próximo ajuste natural é uma
skill de fuga ou cura pras pistolas, não mais dano.
