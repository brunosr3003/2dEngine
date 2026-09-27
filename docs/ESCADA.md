# A escada: atributos por nível, e o golpe por subtração

Pedido do dono (27/09/2026), depois de medir que ele limpava zonas do 20 ao
45 com 94–98% de vida sem poção:

> "não acho que a melhora esteja em nível do mob — nível não tem que dizer
> nada, e sim os atributos. Acho que o grande defeito está em como a defesa é
> calculada hoje, em % e não def − ataque. Re-pense o melhor plano possível
> para ficar sempre balanceado e escalável conforme o jogo flui: nível 35
> precisar de tantos itens com tanta defesa e ataque, nível 40 tanto a mais, e
> por aí vai."

Código: `shared/src/escada.rs` (a régua e o golpe), `shared/src/items.rs`
(a escala das peças e o refino), `shared/src/bosses.rs` (o chefe),
`shared/src/dungeon.rs` (o poder recomendado), `server/src/economy.rs` (o
perfil das espécies) e `server/src/balanceamento.rs` (o guarda).

## O defeito

A mitigação era uma soma de porcentagens que não olhavam quem batia:

| de onde vinha | quanto |
|---|---|
| defesa | 1,5% por ponto, teto 75% |
| pontos em VIT | +5% a cada 25 |
| pontos em RES | +5% a cada 30 (mais 1 de defesa por ponto) |
| armadura pesada | +10% |
| espada e escudo | +40% |
| teto de tudo | 90% |

Cinquenta de defesa cortavam 75% de um lobo nível 1 e 75% de um Colosso
nível 60. O teto se alcançava com 50 pontos — um F2P sem refino (`vuandim`)
já estava nele com o equipamento a +0 — e daí em diante o ataque do mob subir
não mudava nada. O refino tinha uma parte FIXA por nível do item que dominava
peça de base baixa (cinto de defesa 2 → 18 no +5, armadura 14 → 55), e a
"referência" que dizia o poder recomendado tinha um terço da defesa de
qualquer jogador real. Medido em 27/09 com o simulador e o equipamento real
lido do banco: o personagem do dono tomava 1,5% da vida por mob no nível 30
e 1,4% no 45.

## A regra

```
dano = max(ataque − defesa, ataque × PISO)          PISO = 0,10
```

Nos dois sentidos (mob → jogador, jogador → mob, PvP). Não olha nível de
ninguém: o nível só escolhe em que degrau da escada o bicho nasce. A mesma
defesa rende resultados diferentes conforme quem bate, e é isso que faz o
equipamento **envelhecer**. O piso impede a imunidade — mob fraco sempre
belisca, e vinte deles somam (a horda é o que pesa; ver abaixo).

O que continua em porcentagem é só IDENTIDADE — escudo (0,40), armadura
pesada (0,10) —, aplicada **depois** da subtração e com teto 0,50. Os degraus
por ponto (VIT/25, RES/30) saem: eram o mesmo defeito.

Consequência que vem de graça: golpe grande fura armadura, golpe pequeno não.
O chefe telegráfico volta a doer em quem refinou tudo, sem regra especial.

## A escada

Três retas — ataque, defesa e vida ESPERADOS de um personagem no nível,
equipado com a faixa dele a +0 e pontos como `build_do_nivel` assume (um terço
no principal, dois no VIT). Retas, pra a proporção entre dois níveis nunca
mudar. A base absorve o que todo personagem tem sem nível (os 20 de ataque e
100 de vida do `base_player_stats`) e o **legado** — o bônus fixo por peça de
`constants::item_bonus`, que `effective_stats` ainda soma e que vale o mesmo
no 1 e no 60 (é lido da tabela, não copiado; se sair do jogo, a escada
continua fechando).

```
ataque(n) = 45 + 5n      defesa(n) = 10 + 2n      vida(n) = 200 + 20n
```

| Nível | Ataque | Defesa | Vida | Mob comum (vida / ataque) | Poder |
|---|---:|---:|---:|---:|---:|
| 1 | 50 | 12 | 220 | 128 / 3 | 816 |
| 10 | 95 | 30 | 400 | 242 / 30 | 1590 |
| 20 | 145 | 50 | 600 | 370 / 58 | 2450 |
| 30 | 195 | 70 | 800 | 497 / 81 | 3310 |
| 35 | 220 | 80 | 900 | 561 / 93 | 3740 |
| 40 | 245 | 90 | 1000 | 625 / 105 | 4170 |
| 50 | 295 | 110 | 1200 | 752 / 128 | 5030 |
| 60 | 345 | 130 | 1400 | 880 / 151 | 5890 |

(`cargo test -p shared -- --nocapture escada::testes::a_escada_e_reta_e_cresce`
imprime a tabela.) "Poder" é a conta da ficha (`dungeon::poder_de_stats`)
sobre as três retas: é o **poder de referência** das dungeons e da Ilha
Mágica, e por isso se compara com o do jogador — "2.900 de 3.310 esperados"
diz alguma coisa; "poder 4.300" não dizia.

Tudo o mais deriva daqui, e nada tem número próprio.

### Os itens

O template da peça (no banco, espelho de `items::item_template`) é só a
PROPORÇÃO entre as peças. A escala vem de `items::escala_do_roll`: no nível
de item `i`, na cor natural do nível (`tier_from_ilvl`), o **conjunto de
referência** (katana, bainha, armadura média e os quatro acessórios) a +0 soma
exatamente o que os itens têm que dar a um personagem do nível `i` — a escada
menos o que o personagem traz sem item (base, legado, pontos, proficiência).
Cada atributo tem a sua escala (ataque com destreza e sabedoria, defesa, vida
com mana). Uma cor acima da natural vale a razão das cores (azul no nível de
verde: 1,33×); o tier soma +15% por degrau, como antes.

Era `cor × (1 + 1,5% por nível de item) × tier`, um número só pra tudo: a
peça do 35 tinha 25% a mais que a do 18 enquanto o mob tinha 60% a mais.

A defesa das armaduras estreitou em volta da média (leve 0,7× · média 1× ·
pesada 1,4×; o manto do guerreiro de 5,5 pra 2,5). Com a subtração, a pesada
a 1,85× punha o escudeiro no piso contra todo mob comum e a leve a 0,4×
deixava o atirador tomando o dobro. O banco espelha isto pela migração
`escada_armaduras_v1` (só linha que ainda tem a faixa antiga).

Toda peça que existe se recalcula sozinha ao carregar (`ItemInstance::fixar`,
que já existia pra isso): ninguém precisa de migração de item.

### O refino

Só percentual: **+4% da peça por nível**, e nada de parte fixa. +12 (o teto
da Forja) vale +48%, que é a distância de uns quinze níveis na escada. Cash
compra adiantamento, não imunidade — e o adiantamento envelhece junto com a
peça. Foi +5%, depois +8% mais o fixo (17/09); o fixo é o que saiu.

### Os mobs

`escada::mob(perfil, nível)`. O perfil da espécie sai dos MESMOS números que
sempre estiveram na tabela do banco (`Perfil::relativo_ao_lobo`): vida e
ataque relativos ao lobo (urso 2,3× / 1,8×; owlbear 3,8× / 2,8×), defesa em
fração do ataque esperado (1% por ponto da tabela, até 25%: lobo 0, urso 8%,
rochoso 22%). A tabela deixa de ser valor absoluto e passa a ser proporção.

* **vida** = `GOLPES_POR_MOB` (3) golpes do jogador esperado contra um bicho
  de defesa 1,0, vezes a vida do perfil. Calibrado pra reproduzir o tempo por
  abate do jogo medido de 19/09 (3–4,5 s no nível 10): o lobo em três golpes,
  o owlbear em onze.
* **ataque** = a parte que FURA (a defesa esperada do nível) mais o LÍQUIDO
  vezes a personalidade. O líquido é o que o mob de perfil 1 tira de quem
  está na escada: `1,5 + 0,33n` — o lobo tira 1,5% da vida esperada, como no
  jogo medido. A personalidade de ataque entra ACHATADA (`1 + (p − 1) × 0,6`):
  o owlbear tem 2,8× o dano do lobo na tabela, mas contra quem está na escada
  tira 2,1× o líquido. Sem isso os bichos pesados do nível 10 tiravam 30% a
  mais do que no jogo antigo e a meta do 10 morria. A vida não achata.
* **defesa** = a fração do perfil vezes o ataque esperado.
* **a rampa do início**: nos doze primeiros níveis a parte que fura sobe em
  rampa (`furavel`), porque o recém-criado não tem armadura e a jornada do
  início (`metas_do_inicio`) é medida só com a arma. O jogo antigo tinha o
  mesmo em `CURVA_DO_INICIO`.

Quem está na escada toma exatamente o líquido; uma faixa atrás toma o líquido
mais a defesa que lhe falta; quem refinou cai no piso. É o que
`escada::testes::o_mob_comum_sai_das_metas` e `a_mesma_defesa_envelhece`
cobram.

### O chefe

`bosses::dano` = o que fura mais 2,5 líquidos; `bosses::defesa` = 40% do
ataque esperado (quem está uma faixa atrás bate no piso); `bosses::vida` =
250 golpes do jogador esperado. Sem o teto de antes (20 720): o ataque
esperado é reta, então a luta não encurta com o nível. O telegrafado continua
fração da vida e não passa pela defesa. Cabe no u16 do fio até o 60.

## O que mudou, em número

`balanceamento::metas_da_escada::tabela_da_escada` (`--ignored --nocapture`)
imprime tudo; o simulador chega pela **borda** da zona e liga o AUTO ali
(começar no centro de um forte é nascer com 34 mobs em cima). Três perfis:
**na faixa** (a build do nível, +0), **uma faixa atrás** (os pontos do nível
com o equipamento de dez níveis atrás) e **refinado** (na faixa, tudo +10).

Zona comum (18 mobs em raio 45), sem poção, 20 abates:

| Nível | Conjunto | na faixa | uma faixa atrás | refinado |
|---|---|---|---|---|
| 30 | katana | 1,1 s/abate · 17 por mob · vida mín. 96% | 1,6 s · 35 · 94% | 0,95 s · 8 · 99% |
| 30 | espada e escudo | 3,9 s · 7 · 99% | 4,4 s · 12 · 99% | 2,7 s · 5 · 99% |
| 45 | katana | 1,0 s · 19 · 97% | 1,5 s · 36 · 95% | 0,95 s · 8 · 99% |
| 60 | katana | 1,0 s · 20 · 97% | 1,1 s · 38 · 95% | 0,95 s · 9 · 99% |

A zona é fácil de propósito, no 10 e no 60 — era assim no jogo medido de
19/09 e continua. O que pesa é a **densidade**, e é onde a subtração muda o
jogo, porque a horda SOMA:

| Nível 45, com poção | na faixa | uma faixa atrás | refinado |
|---|---|---|---|
| Forte (34 mobs em raio 24) | só a espada e escudo limpa; katana 9/20, pistola 7/20, anel 4/20 — morrem | espada limpa; os outros morrem | os quatro limpam (katana com 78% de vida) |
| Ilhota mágica (5 hordas de 18, um em cinco FORTE) | espada e anel limpam; katana 9/20, pistola 6/20 — morrem | espada limpa; os outros morrem | os quatro limpam (katana 70%, pistola 74%) |

Lê-se assim: na horda o tanque é quem manda (o escudo por cima da subtração),
o refino compra a horda (é o adiantamento que ele é), e quem está atrás
morre. A ilhota deixou de ter o enfraquecimento de 0,85/0,80 nos mobs
comuns: densidade já é o bônus. O **tamanho da puxada** (a matilha,
`world::MATILHA_RAIO_UN`, raio 16) é o botão que decide o quanto a horda
pesa, e ficou onde estava.

Chefes: as doze lutas continuam entre 60 e 240 s esquivando com poção, parado
não vence, e três conjuntos vencem dois níveis abaixo (`metas_dos_chefes`).

## O guarda

"Sempre balanceado" é um teste, não uma intenção. Em
`server/src/balanceamento.rs`:

* `metas_da_escada::a_referencia_anda_na_escada` — o que `build_do_nivel`
  veste mais pontos e proficiência dá a linha da escada com 15% de folga na
  katana, e os outros conjuntos ficam num corredor em volta (defesa 0,5–2,2×,
  ataque 0,6–1,4×). Senão a escada descreve alguém que não existe.
* `metas_da_escada::metas_da_escada` — a cada cinco níveis do 20 ao 60: na
  faixa limpa a zona com ≥ 35% de vida sem poção; uma faixa atrás limpa com
  poção (o tanque, que trava no mago que recua, só precisa não morrer) e toma
  pelo menos 1,25× o que a faixa certa toma; refinado limpa com ≥ 60% e ainda
  toma um quinto do esperado (o piso). Forte e ilhota: uma faixa atrás não
  limpa (salvo o tanque), a ordem refinado ≥ na faixa ≥ atrás vale com cinco
  abates de folga (a puxada é caótica), e na ilhota alguém na faixa limpa do
  30 em diante (no degrau I, alguém refinado).
* `metas_da_escada::o_equipamento_envelhece_e_o_refino_nao_e_imunidade` — a
  mesma peça toma mais a cada dez níveis de mob, e o +10 nunca chega a zero.
* As metas antigas continuam e passaram a vestir a build do nível
  (`simular` usava o pelado com a arma inicial, que servia enquanto o mob mal
  escalava): `metas_de_balanceamento` (1, 5 e 10), `metas_do_inicio` (a
  jornada com a arma só) e `metas_dos_chefes`. No 10, com a faixa vestida,
  a pistola limpa em 1,3 s e a espada em 4,4 s — a tolerância de ritmo foi de
  ±25% pra ±60%, e o "quem atira apanha" passou a valer sobre a média dos
  dois à distância (a pistola derruba o lobo antes de a mordida sair; o anel
  apanha; na horda os dois apanham).

Mexeu em mob, item, refino ou ponto e a proporção quebrou:
`cargo test -p server --bins` reprova.

## O que ainda não está ligado (o servidor)

Esta doc descreve a branch `escada`. A conta de verdade do jogo mora em
`server/src/world.rs`, que estava sendo editado por outra frente quando isto
foi feito, e ainda faz a mitigação antiga. Pra ligar:

1. `dano_mitigado` → `shared::escada::dano_com_reducao` (o hit em `step`, e
   `world/chefes.rs`/`world/dungeon.rs` que o chamam);
2. `vida_e_dano_do_mob`, `defesa_do_mob` e `CURVA_DO_INICIO` de vida/dano →
   `economy::mob_na_escada(kind, nível)` ao nascer o bicho (`place_enemy_*`,
   o respawn e a dungeon);
3. em `effective_stats`, tirar os degraus de VIT/25 e RES/30 do
   `damage_reduction_pct` (ficam escudo e peso da armadura);
4. tirar o 0,85/0,80 dos mobs comuns da Ilha Mágica
   (`place_enemy_in_zone_with_build`), mantendo o 1,25/1,15 do FORTE;
5. o cliente mostrar "ataque 231 / esperado 195" na ficha
   (`escada::linha`) em vez de só o poder;
6. rodar `cargo test -p server --bins` e deployar.

Até lá o simulador anda na frente do servidor de propósito: é ele que diz se
a escada fecha antes de ela encostar no jogo.
