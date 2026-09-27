# Itens: grau, tier, refino e encanto

Mesmo padrão do MIR4, nomes nossos.

## As quatro dimensões

Uma peça é descrita por quatro números independentes:

| dimensão | faixa | como muda | perde ao subir? |
|---|---|---|---|
| **grau** (cor) | Comum · Fino · Raro · Épico · Lendário | dois tier IV do grau abaixo | — |
| **tier** | I · II · III · IV | dois iguais viram o próximo | — |
| **refino** | +0 a +15 | material + moeda, com chance | **sim, zera** |
| **encanto** | atributos nos slots | reagente próprio | não |

O grau é a cor que o jogador vê de longe; o tier é a força dentro dela; o
refino é o esforço; o encanto é a personalização.

## A combinação

```
2 × (grau G, tier I)    →  1 × (grau G, tier II)
2 × (grau G, tier II)   →  1 × (grau G, tier III)
2 × (grau G, tier III)  →  1 × (grau G, tier IV)
2 × (grau G, tier IV)   →  1 × (grau G+1, tier I)
```

Uma regra só, aplicada cinco vezes. E ela tem uma consequência que vale saber
**antes** de assinar embaixo:

```
Comum I → Comum IV          8 peças
Comum IV → Fino I           2 peças
uma cor inteira             ×16
Comum I → Lendário I        65.536 peças
Comum I → Lendário IV      524.288 peças
```

Meio milhão de peças comuns por um lendário IV. É exatamente assim que o MIR4
funciona, e é de onde vem o grind dele. O número não é um acidente do desenho —
é o desenho.

**O refino zera na combinação.** Subir de tier com +12 na peça joga o +12 fora.
Isso é o que impede o jogador de refinar cedo: refino é para a peça em que ele
vai ficar, não para a que vai virar material.

## Refino +1 a +12

### Quanto cada nível dá (decisão de 27/09/2026)

Cada nível soma **+4% dos atributos da peça**, e só isso
(`shared::items::REFINE_BOOST_PER_LEVEL`). +12 vale +48% — a distância de
uns quinze níveis na escada (docs/ESCADA.md): cash compra adiantamento, não
imunidade, e o adiantamento envelhece junto com a peça. Refinar não cria
atributo.

Foi +5% (17/09 de manhã), depois +8% mais uma **parte fixa** por nível do
item (17/09 à tarde), porque numa peça cinza o +1 arredondava pra nada. A
parte fixa é o que saiu: numa peça de base baixa ela dominava — cinto de
defesa 2 virava 18 no +5, armadura de 14 virava 55 — e, com a defesa em
porcentagem fixa de então, deixava o dono imune do 20 ao 45. Hoje a peça já
nasce na escala do nível (`items::escala_do_roll`), então o percentual muda
a peça em toda faixa que importa (teste
`o_refino_e_percentual_e_muda_toda_peca_da_faixa`). Numa cinza do começo o
+1 ainda pode arredondar pra nada: refinar cinza não é o jogo.

**Estes números são os do MIR4.** Vieram da [MIR4 Wiki, revisão 4392, de 5 de
fevereiro de 2022](https://www.mir4.wiki/wiki/Enhancing) — a era do jogo base,
que é exatamente o recorte pedido. A wiki está fora do ar hoje (HTTP 500 na
raiz, *"Cannot access the database"*); isto veio do arquivo da Wayback Machine.

| alvo | chance | falha |
|---|---|---|
| +1 a +3 | 100% | — |
| +4 | 80% | só material |
| +5 | 50% | só material |
| +6 | 30% | **destrói a peça** |
| +7 | 20% | destrói |
| +8 | 15% | destrói |
| +9 | 10% | destrói |
| +10 | 8% ⚠️ | destrói |
| +11 | 6% ⚠️ | destrói |
| +12 | 5% ⚠️ | destrói |

⚠️ = **extrapolado, não é dado.** A wiki tem "?" de +10 a +12; continuei o
achatamento da curva que vinha (80, 50, 30, 20, 15, 10).

**Falhar destrói a peça, não derruba o nível** — e isso independe do grau. É a
diferença entre "tempo" e "aposta": até o +5 refinar é rotina, do +6 em diante
é decisão.

### Custo por tentativa

Uma Pedra de Melhoria do mesmo grau, mais:

| grau | darksteel | cobre |
|---|---|---|
| Comum / Fino | 3.000 | 1.000 |
| Raro | 12.000 | 8.000 |
| Épico | 120.000 | 50.000 |
| Lendário | 1.200.000 ⚠️ | 500.000 ⚠️ |

⚠️ Lendário a wiki nunca preencheu; mantive o salto de ~10× que vinha de Raro
para Épico.

### O custo em PEÇAS

Com destruição, a pergunta deixa de ser "quantas tentativas" e vira **"quantas
peças você queima"** — o refino passa a consumir a economia de *drop*, não só a
de moeda. Conta fechada (`1 / ∏ chance`), não simulação:

```
alvo         peças por uma      tentativas por peça
 +5                      1              6,3
 +6                      3              7,3
 +7                     17              7,6
 +8                    111              7,6
 +9                  1.111              7,6
+10                 13.889              7,6
+11                231.481              7,6
+12              4.629.630              7,6
```

Isso explica por que ninguém tem um +12 no MIR4. E dá o alvo de projeto: o
**+7** é o que um jogador comum alcança, o **+9** é conquista de guilda, e
acima disso é vitrine.

## Encanto

O plano antigo de afixos sorteados foi cancelado. `AffixSlot` continua em
`items.rs` somente para ler bancos e clientes antigos, mas toda peça atual o
mantém vazio e o login remove afixos antigos. Se Encanto for implementado no
futuro, ele também deverá ter resultado explícito e determinístico.

## O que já existe no código

`ItemInstance` tem `rarity` (1-5, que é o **grau**), `tier` (I-IV),
`refinement`, gemas e `sockets`. `affixes` permanece no formato salvo e no
protocolo apenas por compatibilidade; peças atuais sempre o deixam vazio.

Os atributos da peça são **fixos**. `item_id` + cor + tier + nível do item
determinam exatamente vida, mana, ataque, destreza, sabedoria e defesa. O jogo
usa o ponto central da antiga faixa do template, com os multiplicadores de cor,
tier e nível; nenhum RNG ou afixo participa. Assim, duas peças equivalentes têm
sempre os mesmos números e a tela de Craft mostra valores exatos.

No primeiro login com essa regra, `fixar_pecas` normaliza as peças antigas da
bolsa, do banco e as vestidas. Refino, gemas e vínculo são preservados; os
atributos sorteados e afixos antigos são substituídos pelo valor fixo. Peças
antigas que chegam depois pelo mercado ou correio são normalizadas antes de
aparecer na bolsa.

O campo continua se chamando `rarity` no wire e no banco por compatibilidade;
o nome no código é grau.

# Itemização: os slots

O código já tem `EquipSlot` com 11 peças de equipamento. Ferramenta não existe
mais: ver `docs/COLETA.md`.
O MIR4 tem **8** (arma, peito, calça, luvas, botas, colar, bracelete, anel).

**Isso não é detalhe.** O número de slots multiplica a economia inteira: com o
+9 custando 1.111 peças, 8 slots são 8.900 peças por personagem e 11 são
12.200 — o mesmo jogo fica 37% mais longo sem que nenhuma tabela mude. Vale
decidir de propósito, não por herança.

## DECIDIDO: 7 slots (docs/COMBATE.md)

A proposta de 11 slots abaixo ficou pra trás. O jogo tem **arma** (o conjunto
inteiro), **secundária** (amarrada ao conjunto: manto do guerreiro, bainha,
coldre, manto do mago — só entra a do conjunto da arma), **armadura** (uma,
em três pesos) e **quatro acessórios iguais pra todo mundo**: brinco, amuleto,
bracelete, cinto. Um id por peça (400–414); o grau e o refino moram na
instância. Os itens antigos foram migrados ou apagados pela M27
(`server/persistence.rs`).

## (histórico) Proposta: 11 slots, quatro famílias

| família | slots | identidade |
|---|---|---|
| **arma** | Arma | dano — é o slot que decide quanto se bate |
| **arma secundária** | Offhand | escudo, foco ou aljava, conforme a arma |
| **armadura** | Elmo, Peito, Calça, Luvas, Botas | defesa e vida |
| **acessório** | Colar, Anel, Cinto, Capa | atributo e utilidade |

Cada slot precisa de **identidade**, senão a decisão de equipar é só "o número
maior" e as peças viram planilha. A proposta é cada slot ter um stat dominante
e um secundário:

| slot | dominante | secundário |
|---|---|---|
| Arma | dano | conforme a arma (dex, wis) |
| Offhand | defesa **ou** dano | conforme o tipo |
| Elmo | vida | wis |
| Peito | defesa | vida |
| Calça | vida | defesa |
| Luvas | dano | dex |
| Botas | dex | vida |
| Colar | wis / mp | dano |
| Anel | dano | dex |
| Cinto | vida | defesa |
| Capa | defesa | wis |

Assim "trocar as botas" tem um efeito que o jogador sente e nomeia — e não é o
mesmo efeito de trocar o cinto.

## Como o grau e o tier escalam o stat

> 27/09/2026: a escala de NÍVEL DE ITEM deixou de ser `1 + 1,5% por nível`
> e passou a sair da escada (`items::escala_do_roll`, docs/ESCADA.md); a
> razão entre cores e o +15% por tier continuam os daqui.

Vinte degraus de Comum I a Lendário IV. A proposta é **+15% por degrau**:

```
Comum I      1,0×
Fino I       1,75×
Raro I       3,1×
Épico I      5,4×
Lendário I   9,4×
Lendário IV 14,2×
```

Catorze vezes o stat inicial no topo, contra 524.288 peças de custo. A relação
entre esses dois números **é** a curva do jogo: se o topo der 14× e custar meio
milhão, quase ninguém vai; se der 100×, quem não for fica sem jogo.

O refino soma por cima: **+4% por nível**, então +12 ≈ 1,6×. Um Épico I +9 bate
um Lendário I +0 — que é o que faz refinar valer a pena em vez de todo mundo
só empilhar cor.

⚠️ Esses dois números (15% por degrau, 4% por refino) **são nossos**, não do
MIR4, e são os primeiros a afinar quando houver com que medir.

## De onde cada peça vem

Nada disso é lista escrita à mão. A distribuição de mob já decide **nível** por
distância do desembarque; o grau da peça sai do nível do mob que morreu, com
peso — mob comum quase sempre larga Comum, chefe larga Raro pra cima. Assim:

- ilha e distância decidem o nível
- nível decide o grau provável
- grau e tier decidem o stat
- refino decide o resto

Nenhuma tabela por ilha, nenhuma peça nomeada à mão. **Item novo é dado**, do
mesmo jeito que mob novo é dado.

## O que falta decidir

- **8 ou 11 slots** (o parágrafo do topo tem o custo dos dois).
- **Chance de promoção**: no MIR4 a promoção é por sorte e a taxa não é
  publicada. Nossa está determinística — dois iguais sempre viram o próximo.
  Determinística é mais legível; por sorte é mais MIR4 e precisa de um sistema
  de pena, como o *Mystic Incense Burner* deles (25 pontos = garantido).
- **Arma secundária por tipo de arma**: escudo com espada, aljava com arco,
  foco com cajado — ou secundária livre?

# De onde vem o equipamento (o fluxo real do MIR4)

> Referência do MIR4 abaixo. No Tempest, a regra atual é **zero equipamento
> de mobs ou chefes**; eles dão cobre, materiais e poucas poções.
> Veja [a tabela do jogo](LOOT_DOS_MOBS.md).

Pesquisado, não lembrado. **Matar mob não dá peça** — ou quase nunca.

| fonte | o que dá |
|---|---|
| **Mob comum** | cobre (muito comum), poção (comum), peça verde (incomum), Épico/Lendário (**muito raro**) |
| **Quest de campo / pedido** | peça verde **garantida** — é assim que se veste no começo |
| **Chefe / raid** | baús (chefe 15, semi-chefe 5); chance de peça. Raid abre no nível 30, até 15 jogadores |
| **Mineração** | **darksteel** — o recurso que TUDO consome: refino, encanto e craft |
| **Craft** | é daqui que sai Raro, Épico e Lendário. Não de drop |

*"Only craft rare, epic, and legendary weapons"* — o jogo espera que você
**fabrique** o que é bom, e o mob só alimenta a fabricação.

## Rendimento de mineração (medido, wiki rev. 3883)

Sítio de mineração (Vale Bicheon / Vale da Cova das Cobras):

| nó | DS/golpe | HP do nó | respawn | rendimento/hora |
|---|---|---|---|---|
| Branco | 30 | 80 | 8-12 min | 6.750 – 8.300 |
| Verde | 40 | 130 | 9-14 min | 10.170 – 11.700 |
| Azul | 60 | 230 | 16-22 min | 14.200 – 16.130 |
| Vermelho | 120 | 400 | — | — |
| Dourado | 140 | 800 | — | — |

Área especial (Praça Mágica / Pico Secreto), rendimento por nó:

| nó | DS/golpe | HP | por nó |
|---|---|---|---|
| Verde | 60 | 50 | 4.500 |
| Azul | 100 | 75 | 7.500 |
| Vermelho | 150 | 100 | 15.000 |
| Dourado | 300 | 125 | 37.500 |

## O número que decide se copiamos os valores

Juntando as três tabelas reais — chance de refino, custo em darksteel por
tentativa, rendimento de mineração por hora:

```
grau         alvo    peças  tent/peça      darksteel   horas minerando
Comum/Fino     +5        1        6,2         18.750               1h
Comum/Fino     +7       17        7,5        377.500              27h
Raro           +5        1        6,2         75.000               5h
Raro           +7       17        7,5      1.510.000             106h
Raro           +9     1111        7,6    101.586.667           7.154h
Épico          +7       17        7,5     15.100.000           1.063h
```

**Um slot Raro +7 custa 106 horas de mineração. Oito slots: 851 horas.**

Isso não é exagero de cálculo — é o desenho. O MIR4 é um jogo de deixar
rodando: tem auto-hunt, auto-mineração e horizonte de anos. Os números dele
pressupõem que ninguém está *jogando*, está *esperando*.

**A estrutura vale copiar; os valores não.** Estrutura é o que faz o sistema
ter forma: faixa segura até +5, destruição acima, refino perdido na promoção,
craft como fonte do que é bom, mineração como base de tudo. Os valores são
sintonizados para retenção de anos com o jogo minerando sozinho — num play
test, ninguém chega a ver um Raro +7, e aí não se aprende nada sobre o topo.

A proposta é manter cada regra e escalar as três constantes que ligam esforço a
resultado — **darksteel por tentativa**, **rendimento de mineração** e **peças
por craft** — até um Raro +7 caber em algumas horas em vez de 106. Continua
sendo o mesmo jogo; só cabe numa sessão.

# Estado real no código (forja e poção de XP)

- **Refino ligado:** o `RefineItem` antigo (só ouro, até +15) saiu. A Forja usa
  `shared::forja`: +1..+12, chance da tabela acima, até +5 falhar só gasta o
  material, **do +6 em diante falhar destrói a peça**. Vale pra peça da bolsa
  e pra peça vestida (`ClientMessage::Refinar { alvo }` → `RefinoResultado`;
  vestida destruída sai do slot e os stats são recalculados).
- **Custo por tentativa no jogo:** o de `custo_de_refino` dividido por 10
  (`forja::DIVISOR_DO_CUSTO_EM_JOGO`) — Comum/Fino 300 darksteel + 100 cobre,
  Raro 1.200 + 800, Épico 12.000 + 5.000. A Pedra de Melhoria ainda não existe
  como item e não é cobrada.
- **Onde se refina:** pelo botão Forja do HUD, de qualquer lugar, ou clicando no
  Ferreiro da vila. Nenhuma tecla abre.
- **Aprimorar:** aba **Aprimorar** do Craft (protocolo 107). A instância
  guarda a **cor** em `rarity` (1 cinza … 5 lendário — é o que pinta a borda e
  decide o custo do refino) e o **tier** em `tier` (I…IV; peça salva antes do
  campo lê como I).
  - duas peças com o mesmo `item_id`, cor e tier → uma do tier seguinte;
  - duas **Tier IV +8** → uma da **cor de cima, Tier I** (`REFINO_PARA_COR`),
    pedindo o nível da cor (verde 20, azul 40, épico 60, lendário 80) e subindo
    o nível do item pro da cor;
  - a nova recebe os valores fixos do novo degrau (`ItemInstance::roll_em`):
    cada tier soma +15% (`bonus_do_tier`, IV = 1,52×); o refino se perde; peça
    vinculada contamina; peça com gema é recusada;
  - cobre por degrau de origem: 500 / 2.000 / 8.000 (tiers) e 16.000 (cor),
    ×4 a cada cor.
- **Encanto:** não existe; o plano antigo de atributos aleatórios foi cancelado.
- **Poção de Experiência** (`item_id::XP_POTION` = 350, stack 20, sem compra,
  venda 1): usar dá **+30% de XP de personagem por 1 hora**; beber outra com o
  bônus ativo renova a hora cheia (não acumula %). O fim do bônus é um instante
  absoluto salvo em `characters.xp_bonus_ate`, então sobrevive a relog,
  reinício e troca de ilha. Aplica em todo XP que passa por `grant_xp`
  (kill, missão); não mexe em XP de proficiência. Só sai de recompensa de
  missão de área (ver MISSOES.md).

## Poções de recurso (cura ao longo do tempo)

No molde das Large/Great HP Potion da MIR4: uma parte **na hora** e o resto em
**ticks de 1 s**, sempre em **fração do máximo** (acompanha o nível). Tabela em
`shared::pocoes::CURAS`.

| poção | id | na hora | por tick | duração | total | recarga do grupo |
|---|---|---|---|---|---|---|
| Vida | 2 | 4% HP | 2% | 5 s | 14% | 8 s (grupo Vida) |
| Vida+ | 9 | 6% HP | 3% | 5 s | 21% | 8 s (grupo Vida) |
| Mana | 8 | 5% MP | 3% | 5 s | 20% | 8 s (grupo Mana) |
| Mana+ | 10 | 8% MP | 4,5% | 5 s | 30,5% | 8 s (grupo Mana) |
| Vigor | 11 | — | 5% | 6 s | 30% | 15 s (grupo Vigor) |

- **Recarga por grupo:** Vida bloqueia Vida+. A recarga é **sempre ≥ a
  duração** da cura — garantido em tempo de compilação — então **não existe
  tomar outra no meio** nem substituição. O servidor recusa o `UseItem` do grupo
  em recarga **sem gastar** a poção e avisa ("[Poção] Vida em recarga: 5,2 s").
- Recurso cheio: recusa sem gastar. Dano não interrompe. Caído/morto: a cura
  acaba (a recarga continua).
- `PocaoGrupo { grupo, recarga_s, cura_s }` vai ao beber e ao recusar; o HUD
  mostra a sombra da recarga no botão, borda verde enquanto cura e ícones
  "+V / +M / +E" com os segundos.
- **AUTO da barra:** abaixo do limiar e com o grupo fora da recarga; o tier é o
  **menor cujo total cobre o que falta pro máximo**, senão o maior que houver
  (`pocoes::escolher`). Substitui a regra antiga "Vida+ abaixo de 35%".

## Poções de buff de drop

| Item | id | Efeito | Duração |
|---|---|---|---|
| Poção de Experiência | 350 | +30% de XP de personagem | 1 h |
| Poção de Fortuna | 351 | +30% da quantidade de **ouro e cobre que caem de bicho** (loot de quem matou) | 1 h |
| Poção de Sorte | 352 | chance de cada linha de drop **não garantida** × 1,2 — loot de bicho (de quem matou) e coleta (de quem coleta) | 1 h |

- Beber outra com o buff ativo **renova a hora cheia**; não acumula porcentagem.
- O fim de cada buff é absoluto e salvo no personagem (`xp_bonus_ate`, `fortuna_ate`, `sorte_ate`): vale depois de relog, reinício e troca de ilha.
- Fortuna não vale na coleta (é ouro de bicho). Linha garantida (chance 100%) continua 100% com Sorte.
- Nenhuma das três se compra: saem de recompensa de missão (área → Experiência; diária de criar → Fortuna; diária de refinar → Sorte).
