# Combate: o desenho do playtest

As regras atuais de skills estão em [SKILLS.md](SKILLS.md). Ver
[PERSONAGEM.md](PERSONAGEM.md) para o rig e a animação.

## A arma é um CONJUNTO, não uma peça

A arma é o conjunto inteiro, e a secundária vem amarrada a ela. A katana é
segurada com as duas mãos, na guarda e nos golpes:

| arma (principal) | secundária | leitura |
|---|---|---|
| **espada e escudo** | manto do guerreiro | linha de frente |
| **katana** | bainha | opens and closes: draw and execute |
| **duas pistolas** | coldre | à distância, pirata |
| **anel mágico** | manto do mago | cura e magia |

Quatro no playtest; outras entram depois.

**Isso resolve sozinho um risco que o offhand livre criava.** Com o offhand
solto, se o escudo fosse a única secundária com número, todo mundo carregaria
escudo e o slot viraria escolha falsa. Amarrando a secundária à arma, a escolha
sai do slot e vai pro conjunto — que é onde ela tem consequência de verdade.

O **anel mágico** no lugar da varinha é decisão de tema: num mundo de
navegação e ilhas, quem cura não anda com um graveto na mão.

## The katana OPENS and CLOSES (the draw)

The katana's passive lifesteal is **gone**. It was 100% of the class's defence,
which is how it managed to be the fastest **and** the safest at once: it
cleared the Ilha Mágica horde at level 60 on 86% health — above the pistol
(78%) and the ring (66%) — while still killing faster than both. Measured with
the lifesteal at zero, the same horde left the katana at **18%**. It was not a
bonus on top of the class's defence; it *was* the defence.

Against a BOSS it already made no difference at all: 4% or 0% give exactly the
same line when dodging. What holds the katana up against a boss is VIT regen,
which every set gets.

In its place the katana gets what no other set has — the ends of the fight:

| | what it is | how much |
|---|---|---|
| **Opener** | first strike on someone who has not seen the player | `KATANA_OPENER_MULT` 2.5× |
| **Execute** | strike on a target below 30% health | `KATANA_EXECUTE_MULT` 1.6× |
| **Thirst** | lifesteal window opened by **Dança** | `KATANA_THIRST_LIFESTEAL` 25% for `KATANA_THIRST_S` 3 s |

And it pays for that in the middle: allocated STRENGTH **no longer pays
twice**. It already gives +1 attack per point through `STAT_POINT_BONUS`, and
the extra half the katana added was a second payment for the same point — 62%
more attack than the sword on the SAME 0.40 s swing. At level 60 attack drops
from 359 to 326, putting sustained damage below the pistol's (381), which is
the set that should hold the highest single-target damage.

Sustain is now **chosen**: Dança costs 18 mana and a 15 s cooldown, and the
window is short. Outside it the katana does not heal itself.

## Armadura tem PESO, e o peso é a escolha

Três pesos, e cada um é uma troca declarada:

| peso | dá | cobra |
|---|---|---|
| **leve** | bônus de dano | pouca resistência |
| **média** | **FOR que cresce com o nível**, equilíbrio | equilíbrio |
| **pesada** | resistência | menos dano |

**A média empresta FOR** (`for_da_armadura`), somada como ponto alocado em
`effective_stats`. Sem isso ela era a pior das três em toda conta: menos defesa
que a pesada e sem o dano da leve — "o meio" não é uma escolha se não dá nada
de próprio. O peso dela continua o do meio: a FOR não a transformou numa leve.

O valor **cresce com o nível**: 1 ponto, mais 1 a cada 10 níveis
(`FOR_DA_ARMADURA_MEDIA`, `FOR_DA_MEDIA_A_CADA`). Fixo em 5 a simulação de
chefe reprovou na hora — o Lobo Alfa (nível 8) caía em 55 s contra a meta de
60, e o jogador **parado** bebendo poção vencia com 13% de vida. Cinco pontos
são ruído no nível 60 e são a luta inteira no nível 8.

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

Cada golpe concede 1 XP de proficiência e cada abate, mais 10. Até o nível 50,
o custo do próximo nível continua sendo `50 × nível`. Depois disso, cresce
12% por nível: o nível 70 exige cerca de 24 mil XP só no próximo degrau, o
100 cerca de 723 mil, e o 115 cerca de 4 milhões. O limite técnico é 200.
Ao morrer, perde 10% do custo do nível atual em cada arma já treinada; isso
pode reduzir o nível. A ficha mostra nível e progresso de cada arma.

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

Níveis 1, 5 e 10, AUTO, **com poção** (no nível 1 só valem a primeira, a
segunda e a última — lá a matilha curta do início é de propósito, ver "Início
do jogo"):

- Todo conjunto limpa 10 mobs vivo.
- HP mínimo ≥ 25% com espada e escudo e ≥ 15% com os outros.
- Quem atira apanha: pelo menos 3 de dano por mob.
- Dano por mob do corpo a corpo no máximo 1,5× o da distância (meta ~1,4×).
- Tempo de luta por abate dentro da `TOLERANCIA_DE_RITMO` (±25%) da média do
  nível.

As duas últimas mudaram junto com o dano de skill; o porquê está em "Dano de
skill", abaixo.

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
| Katana | 12% do golpe volta como vida | opener 2.5× and execute 1.6×; the lifesteal became Dança's window |
| HP dos mobs comuns | lobo 50, urso 120, pistoleiro 35, tigre 40, mago 45, owlbear 200, arqueiro 45 | 120, 280, 85, 95, 105, 460, 105 |

As constantes estão no topo de `world.rs` (`PROVOCACAO_S`,
`MATILHA_RAIO_UN`, `CARGA_PROVOCADO_MULT`, `REDUCAO_DO_ESCUDO`,
`ABERTURA_DA_KATANA`, `EXECUCAO_DA_KATANA`, `ROUBO_DA_SEDE`,
`CADENCIA_DAS_PISTOLAS_S`, `CADENCIA_DO_ANEL_S`). O
HP dos mobs vai ao banco pela migração `balanceamento_hp_mobs_v1`, que só
altera linha com o HP antigo do seed.

### Depois (golpe básico e skills em AUTO, com poção)

| Nível | Conjunto | s/abate | Dano por mob | HP mínimo |
|---|---|---:|---:|---:|
| 1 | Espada e escudo | 3,75 | 4,0 | 96% |
| 1 | Katana | 3,75 | 6,5 | 93% |
| 1 | Pistolas | 2,84 | 0,0 | 100% |
| 1 | Anel | 2,98 | 0,0 | 100% |
| 5 | Espada e escudo | 4,45 | 11,2 | 79% |
| 5 | Katana | 4,62 | 18,6 | 51% |
| 5 | Pistolas | 4,11 | 13,4 | 59% |
| 5 | Anel | 4,32 | 12,6 | 78% |
| 10 | Espada e escudo | 4,56 | 14,5 | 55% |
| 10 | Katana | 3,86 | 23,8 | 44% |
| 10 | Pistolas | 3,15 | 15,0 | 24% |
| 10 | Anel | 4,47 | 24,2 | 69% |

Razão de dano corpo a corpo × distância: 1,15 (nível 5) e 0,98 (nível 10).

### Dano de skill: em cima do ataque, não número fixo

Queixa do playtest: "a skill do pistoleiro é pior que o ataque básico". Era
verdade, e mensurável. O dano de skill era o `dano` do catálogo, cru, sem
escalar com atributo, arma ou nível — e conjurar DESLIGA o golpe básico
(`casting_until`, em `world/habilidades.rs`). No nível 80 o Tiro Certeiro
entregava 28 onde o básico entregaria ~400 na mesma janela: apertar a skill
era perder dano.

`Skill::dano_efetivo(atk, cd)` calcula o dano a partir do que a conjuração
custa:

```
janela    = impacto_em() + RECUPERACAO_S
deslocado = janela × atk / cadência_do_básico    (o básico que a skill desliga)
dano      = deslocado × clamp(ganho × dano/30, PISO, TETO_DO_GANHO)
```

O `dano` do catálogo virou PESO RELATIVO entre as doze. Ganho de 1,60 para
alvo único e 1,30 para área, com **piso** de 1,60 e 1,35 e teto de 1,80 —
contra UM alvo, toda skill rende pelo menos 35% a mais que o básico que
desliga. E a skill pode dar crítico, com a mesma chance e o mesmo
multiplicador do básico.

**Por que mudou (19/09/2026).** Até então a área rendia 0,47× a 0,82× por
alvo e o alvo único batia num teto de 1,05×: contra um bicho ou um chefe,
apertar quase qualquer skill era perder dano — o jogador percebeu o "tempinho
de carregamento" deixando a skill pior que o básico. A regra só fechava
contando três alvos, e o básico corpo a corpo também acerta todo mundo no
cone. O freio da duração das lutas de chefe passou para onde ele deve estar:
a vida do chefe subiu 12% (docs/BOSSES.md) e a katana, que conjura sem parar,
ganhou esperas maiores (Saque 10 s, Dança 15 s, Vento Cortante 20 s). O piso
de área de 1,35 não é arbitrário: com 1,20–1,25 a skill deixava o bicho quase
morto, a luta se arrastava e a espada (nv5) e a katana (nv10) morriam no
simulador de farm; de 1,35 pra cima as quatro terminam vivas.

Um coeficiente fixo não resolveria: a cadência do básico
melhora com o nível, então a dívida cresce junto, e por isso a cadência entra
na conta. Três invariantes em `shared/src/skills.rs` seguram a regra: toda
skill ofensiva rende pelo menos o piso sobre o básico que desliga contra UM
alvo, o dano acompanha o ataque
de quem conjura, e skill sem dano continua sem dano.

**Defeito do simulador achado no caminho.** Ele travava o básico só por
`impacto_em()`, enquanto o servidor trava por `impacto_em() + RECUPERACAO_S`:
devolvia 0,36 s de ataque grátis por conjuração e superestimava toda skill.
Com a janela certa e medindo o código ANTIGO, de dano fixo, o pistoleiro
**morre no nível 10 mesmo com poção infinita** (HP 0%, não limpa os 10). Com a
fórmula nova ele termina com 24% e limpa. Ou seja: o que faltava ao pistoleiro
era dano de skill que escalasse — não uma skill de fuga, como se supôs antes.

**Duas metas recalibradas**, pelo mesmo motivo: foram calibradas contra o
simulador que dava ataque grátis, e quebravam sozinhas no código antigo assim
que ele passou a cobrar certo. A de zona passou a rodar com poção, como as de
chefe já faziam; e a tolerância de ritmo foi de ±20% para ±25%, porque a
pistola limpa 22% mais rápido que a média e paga com o menor HP dos quatro
(24% contra 69% do anel) — e nenhum valor de ganho de área fecha essa
diferença, já que os dois extremos ficam presos no teto.

## Atributos dos mobs e equipamento

**Desde 27/09/2026 isto é a ESCADA — docs/ESCADA.md.** O golpe é
`max(ataque − defesa, ataque × 0,10)` nos dois sentidos, sem olhar nível; o
que se espera de ataque, defesa e vida em cada nível são três retas
(`shared::escada`), e mob, item, refino, chefe e poder recomendado derivam
delas. O que segue é o registro do que valia antes.

Não havia multiplicador oculto por diferença de nível. O nível da faixa
aumentava os atributos reais de cada mob: após o nível 12, a cada nível, +4%
da vida base, +3,5% do ataque base e +0,5 de defesa. A defesa do jogador
valia 1,5% por ponto até 75% — e é isso que quebrou: 50 de defesa cortavam
75% de qualquer golpe, do lobo do 1 ao Colosso do 60, e um F2P a +0 já os
tinha.

O servidor envia, junto à identificação de cada inimigo, vida, ataque e
defesa reais e três referências de equipamento: ataque, defesa e poder
recomendados. O Menu → Mobs lista a versão mais forte de cada espécie das
ilhas visitadas, os chefes e os inimigos vistos de perto, e compara as
referências com os atributos atuais do personagem. São orientações, não
travas de entrada nem modificadores secretos de dano. Na escada a
referência é `escada::linha(nível)` — o mesmo número da ficha.

## Início do jogo

Queixa (jogando a produção no iPhone): "morrer numa quest nível 1 é meio
dureza".

**Como se mede.** `balanceamento::jornada` joga o começo como ele é jogado:
personagem recém-criado (só a arma do conjunto, zero ponto distribuído,
nenhuma skill em AUTO — o padrão de quem nunca mexeu), fazendo em ordem 700,
701, 501, 702 (5 lobos), 502 (6 lobos), 703, 503, a trava de nível 3 da 504,
504 (3 ursos), 704 (nível 5), 705–707 e 708 (4 ursos). Contagens, alvos, XP e
poções saem das `QuestDef` de verdade. Cada caçada vai pra zona que a auto
missão escolhe (`quests::zona_do_bicho`) sobre as zonas REAIS do Bosque
(`world::zonas_comuns_da_ilha`, a mesma conta do `povoar_ilha`), chegando pela
borda vinda da cidade. A vida passa de um passo pro outro (ida e volta da
cidade regeneram andando), a XP sobe com abate e recompensa, e a poção é a da
barra padrão (AUTO abaixo de 60%) com o que as missões deram. Simplifica: a
503 vira 10 abates, 705–707 só dão XP, a Poção de Experiência fica na bolsa.
Roda em ~0,4 s.

```sh
cargo test -p server --bin server metas_do_inicio -- --nocapture
```

### Antes

| Conjunto | Mortes até a 708 (sem poção / com) | 702: HP mínimo | 702: bichos em cima |
|---|---:|---:|---:|
| Espada e escudo | 11 / 10 | 17% | 6 |
| Katana | 7 / 7 | 47% | 6 |
| Pistolas | 34 / 32 | morre 2× | 5 |
| Anel | 32 / 30 | morre 2× | 5 |

**Causa, em números:**

1. **Não existe zona de nível 1 perto da cidade.** Mob só nasce a mais de 68
   da cidade, e o nível sobe com a distância: a zona mais perto do Bosque é
   **2–4**. Nos níveis 3 e 4 metade dos bichos é urso (280 de vida, 18 de
   dano, defesa 8), contra um personagem de 100 de vida, sem ponto e com regen
   de 0,5/s.
2. **Um golpe acordava a zona.** Vagas a 7 de distância, matilha provocada num
   raio de 16 e visão de 9: bater num lobo trazia 5–6 bichos, investindo a
   2,5×. Quem atira morria primeiro (50–67 de dano por abate).
3. **O AUTO emenda um bicho no outro.** Sem descanso, 0,5/s de regen não
   devolvia o que a luta tirava, e a subida pro nível 5 (15 000 de XP, ~20 min)
   terminava em morte por atrito.

### Metas (asserts em `metas_do_inicio`)

Os quatro conjuntos, sem poção e com as poções das missões:

- Ninguém cai até o nível 5 e os ursos da 708.
- HP mínimo de cada caçada ≥ 35% sem poção e ≥ 50% com.
- Nas duas primeiras caçadas (702 e 502), no máximo 2 bichos em cima.
- Caçada de missão em até 10 min; o nível 5 em até 3 h.

E as metas antigas continuam: níveis 5 e 10 inteiras, nível 1 vivo, HP mínimo
e ritmo ±20%; chefes sem mudança.

### O que mudou

Tudo em `world::CURVA_DO_INICIO`, pelo **nível do mob** (a faixa da zona onde
ele nasceu, `EnemyTag::nivel_da_faixa`). Nível 5 em diante, e mob fora de zona
de faixa (chefes, eventos), com os números cheios.

| Nível do mob | 1 | 2 | 3 | 4 | 5+ |
|---|---:|---:|---:|---:|---:|
| Raio da matilha | 6 | 6 | 6 | 8 | 16 |
| Carga do provocado | 2,0× | 2,0× | 2,0× | 2,2× | 2,5× |
| Visão (fração da tabela) | 70% | 70% | 70% | 80% | 100% |
| Vida e dano (fração da tabela) | 60% | 65% | 70% | 80% | 100% |

**Fôlego de iniciante:** até o nível 5 do personagem, 3 s sem levar dano, 2%
da vida máxima volta por segundo (além do regen normal).

Nada muda no banco: `enemy_kinds` fica como está (a fração é aplicada no
spawn), e as missões mantêm as contagens. Com fôlego de 3%/s ou a curva cobrindo o
nível 5, o nível 5 do simulador antigo passava do limite de 1,5× corpo a
corpo × distância (o Anel cura menos e mata mais rápido); 2%/s até o nível 5 e
curva até o 4 passam as duas.

### Depois

| Conjunto | Mortes (sem poção / com) | 702: HP mínimo / bichos | Pior caçada (HP mínimo, sem poção) |
|---|---:|---:|---|
| Espada e escudo | 0 / 0 | 89% / 2 | nível 5: 47% |
| Katana | 0 / 0 | 86% / 2 | nível 5: 52% |
| Pistolas | 0 / 0 | 86% / 1 | nível 5: 69% |
| Anel | 0 / 0 | 86% / 1 | nível 5: 37% |

Cada caçada de missão leva 70–130 s; o nível 5 sai em 19–21 min. Metas antigas
no nível 5: dano por mob e s/abate iguais, razão 1,46×; HP mínimo subiu
(espada 47→71%, katana 43→47%, pistolas 59→73%, anel 69→73%) pelo fôlego.
Nível 10 idêntico.

## INT no ataque básico mágico (26/09/2026)

Com o Anel Mágico equipado, cada ponto **alocado** em INT agora acrescenta
mais 1 ao dano bruto do ataque básico, além do +1 que já entra no atributo
geral de ataque. Dez pontos de INT dão +20 ao básico em relação ao anel sem
esses pontos. As skills continuam usando apenas o ataque geral; armas físicas
não recebem esse bônus. O cálculo é feito no servidor antes do crítico e da
mitigação e usa `shared::basic_attack_damage`, também empregado nos
simuladores de combate.

## População inicial — 26/09/2026

Na ilha inicial, áreas comuns cuja faixa começa até o nível 10 têm teto de
30 mobs (antes 18), mantendo 8 unidades entre slots e os filtros de terreno
acessível, cidade, porto e cabanas. Fortes mantêm a população anterior.
A reposição nas faixas iniciais passa de 20 para 12 segundos. O limite real
depende dos pontos válidos de cada área. Validação: população do Bosque,
proteção de postos e simulação `metas_do_inicio`.

## Início progressivo — 26/09/2026

A curva introdutória dura até o mob de nível 10, alcançando os valores
completos no 11 (antes já no 5). Dano nos níveis 1..10: 40%, 45%, 50%,
55%, 60%, 68%, 75%, 82%, 90%, 95%. Vida: 60% a 100%. Visão, raio de
matilha e velocidade de investida também sobem gradualmente. O fôlego
fora de dano começa em 3% de HP/s após 3 segundos, perde força do nível
6 ao 10 e termina no 11. Mantidos os multiplicadores de nível 20+.

## Modo Pacífico, Hostil e pontos de PK

No HUD, logo abaixo do poder e acima das missões, o botão ao lado dos buffs
alterna Pacífico / Hostil com um toque e mostra os PK points. Pacífico bloqueia ataques a jogadores, inclusive contra outra
facção e na Ilha Mágica. Hostil permite atacar jogadores mesmo que o alvo seja
Pacífico. O nome de quem está Hostil fica roxo para todos os observadores.

Zonas seguras bloqueiam PvP para ambos os lados, inclusive personagens com
PK points. Fora delas, a morte de um jogador sem PK points dá +1 ponto ao
atacante. O ponto é cobrado uma única vez ao zerar o HP, na mesma transição
que aplica as penalidades de morte; acertar golpes não dá pontos. Matar quem
já tem PK points não dá pontos. Nos três graus da Ilha Mágica, mortes não dão
pontos. Modo e pontos persistem ao sair e trocar de zona; não há redução
automática de pontos nesta versão. Protocolo 147.
