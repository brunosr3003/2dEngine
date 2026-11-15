# Reset de nível e árvore de caminhos

Desenho fechado com o dono em 14/11/2026. Substitui o desenho de 12/11
(reset com subclasse escolhida de cara e skills 4–6 por arma).

**Estado:** etapa A feita no branch da 199 (15/11/2026): teto 100
(`progressao::NIVEL_MAX`, `somar_xp`), "100+" na tela (`texto_de_nivel` /
`texto_de_faixa`), dungeon e missão pedem no máximo nível 100
(`dungeon::cadeado`, `quests::nivel_pedido`) e passam disso só por poder,
sem trava de nível em craft (`craft::conferir`, `craft_ui::motivo`) nem em
viagem (Geleira), PvP por poder (`pk::pvp_mult_poder`,
`pk::protegido_por_poder`). Os três pontos que ficaram de fora também
(15/11): o poder de referência das dungeons segue o jogador REAL
(`dungeon::fator_real`, 1,0 até o nível 20 subindo a 1,6 no 40), o
simulador de PvP do Panopticon usa o gap por poder (`Regras::por_poder`), e o
poder de cada jogador vai no `EntityMeta.poder` pra o cliente aplicar a mesma
proteção.

**Renascimento, fase 1 — feito (15/11/2026, versão de teste):** regras e
números em `shared::renascimento` (`pode_renascer`, `divisor_de_renascimento`
/ `xp_depois_do_divisor` = 1,5^r em `Session::grant_xp` depois do ritmo e antes
dos bônus, `mult_de_atributos` = 1 + 0,05·r, `essencia_do_renascimento` =
10·n, `bau_do_renascimento`, `e_marco` / `marcos_ate` / `marco_de` /
`proximo_marco`, `tier_da_aura` / `intensidade_da_aura`). Coluna
`characters.renascimentos` (`Session::renascimentos`). Ação
`ClientMessage::Renascer` → `world/renascimento.rs` `handle_renascer`: só no
`NIVEL_MAX`, fora de dungeon/tutorial/instância/Arena e não caído; zera XP,
pontos (a Energia gasta neles volta), SP e as mortes recuperáveis; mantém o
resto; manda `ProgressUpdate`, `StatPointsUpdate`, `StatsUpdate`,
`ServerMessage::Renascimento { festa }` e marca `nivel_mudou`. O +5% entra em
`world::effective_stats_r` (vida, mana, ataque, defesa, destreza e sabedoria
máximas, no fim da conta; porcentagens, regen, stamina e firmeza não), e
`effective_stats` = `effective_stats_r(.., 0)`; sessão, ranking
(`ranking::monta`, `server --poder`) e `pvp_sim::do_personagem` passam o
número. Prêmios: Essência (item 477 `ESSENCIA_RENASCIMENTO`, vinculada) e o
baú (Darksteel = topo do baú do nível 60 +50% por renascimento, 1 chave na
cor — 2 no marco —, 20+5n materiais na cor; cor 2 verde 1–2, 3 azul 3–4, 4
roxo 5–9, 5 lendária 10+ (material para no roxo)); o que não cabe vai pro
Correio (motivo 5). Marcos: r2 +1 expansão da bolsa e r7 +1 do banco (+10
espaços cada, a unidade de `armazem`; se já no máximo, 2× a Essência do
renascimento), aviso global nos marcos (1, 3, 5, 7, 10 e de 5 em 5 a partir
do 15) pela tabela `avisos_globais` (chega em todos os canais). Na tela:
`EntityMeta.renascimentos` ("R3 Nome" na placa), `LinhaRanking.renascimentos`
(ranking de nível ordena por renascimentos e depois nível), aura de corpo
inteiro e festa do renascimento em `client/renascimento_vfx.rs` (escala com
o tier e mais forte nos marcos), botão e confirmação na ficha
(`ficha_ui`). Ficaram pras próximas fases: árvore de caminhos, loja da
Essência, ascensão de equipamento, slots 4–6, títulos, cor de nome e skins
dos marcos (o número de renascimentos basta pra conceder retroativo via
`marcos_ate`). O divisor e o +5% estão sem calibrar.

**Renascimento, fase 2 — árvore de caminhos com passivas — feita (15/11/2026,
versão de teste):** tabela e regras em `shared::arvore` (`NOS` 4×15,
`Caminho`, `Efeito`, `No { efeito, valor, ativa }`, `ATIVAS`,
`pontos_ganhos`, `custo_de_redistribuir`, `Arvore { nos: [u8; 4] }` com
`gastar` / `validada` / `passivas` / `para_json` / `de_json`, `Passivas` com
`somar_economia`, `PedidoArvore`, `EstadoArvore`). **Pontos** derivados,
nunca guardados: `pontos_ganhos(r, nível exibido)` = 0 sem renascer, senão
10·(r−1) + ⌊nível/10⌋ (seis subidas completas enchem as 60 casas). Guardado
só o quanto foi comprado de cada caminho, em `characters.arvore_json`
(`"[a,b,c,d]"`, ALTER no boot, `busca!` próprio no load, UPDATE próprio no
`write_batch`); no login vira `validada(pontos)` (caminho > 15 corta, e o
excesso sai das últimas casas do caminho mais cheio). Caminho linear: a casa
k só depois da k−1 (o pedido manda o índice da próxima casa, então toque
duplo não compra duas). **Redistribuir:** tudo de volta por 1.000 de ouro ×
pontos gastos (`OURO_POR_PONTO_REDISTRIBUIR`); a opção com Essência (fase 3)
entra como variante NOVA no FIM de `PedidoArvore`. **Efeitos:** combate em
`world::effective_stats_rv(.., renascimentos, &arvore)` (o `_r` antigo =
`_rv` com árvore vazia; sessão, `ranking::monta`/`server --poder` e
`pvp_sim::do_personagem` passam a árvore): as % de vida, mana, ataque,
defesa, destreza e sabedoria SOMAM ao multiplicador do renascimento
(`1 + 5%·r + árvore%`, não compõem); crítico, velocidade de ataque,
movimento, esquiva (`dash_cd_mult`), regen, redução de dano e firmeza somam
direto. Economia em `Session::bonus_economia()` = `acessorios::bonus` +
`Passivas::somar_economia` (drop, ouro dos monstros — campo novo
`Bonus::ouro`, multiplica o ouro do abate —, minério, Energia, Darksteel,
saque melhor); todos os lugares que liam `acessorios::bonus(&equipamento)`
pra economia agora leem `bonus_economia()`. **Protocolo:**
`ClientMessage::Arvore { pedido }` e `ServerMessage::Arvore { estado }` no
fim dos enums; o estado vai no login, depois de cada pedido, no
renascimento e quando a subida ganha um ponto (com aviso no chat).
`world/arvore.rs` `handle_arvore`. **Tela:** `client/arvore_ui.rs`, aberta
pelo botão CAMINHOS da ficha (só com renascimento ≥ 1; ponto vermelho nele e
na Ficha/Menu quando há ponto livre); quatro colunas × 15 casas (grade 3×5),
descrição da casa, "Aprender", redistribuir com confirmação, casas ativas
marcadas com losango dourado e "Nó ativo: … — em breve". O Menu › PERSONAGEM
não ganhou entrada própria: o grupo já tem os 5 de `POR_LINHA` e cada entrada
exige arte no atlas; a árvore abre pela Ficha, que está nesse grupo.

Tabela (casas 5, 10 e 15 = ativas da fase 4, que por enquanto dão a passiva):

| Caminho | Casas 1–15 | Total do caminho cheio |
|---|---|---|
| Assassino | crít 1 · vel.atq 2 · DES 4 · crít 1 · **mov 2 (Passo Sombrio)** · vel 2 · crít 1 · DES 4 · esquiva 5 · **crít 1 (Invisibilidade)** · vel 2 · DES 4 · crít 1 · vel 2 · **mov 2 (Marca da Morte)** | crítico +5 pp, vel. de ataque +8%, destreza +12%, movimento +4%, recarga da esquiva −5% |
| Mago | SAB 4 · mana 5 · atq 2 · regen mana 0,5 · **SAB 4 (Nova Arcana)** · mana 5 · atq 2 · regen 0,5 · SAB 4 · **atq 2 (Meteoro)** · mana 5 · regen 0,5 · SAB 4 · mana 5 · **atq 2 (Distorção do Tempo)** | sabedoria +16%, mana +20%, ataque +8%, regen de mana +1,5/s |
| Guardião | def 4 · vida 4 · regen 1 · def 4 · **red 1 (Provocar)** · vida 4 · def 4 · firmeza 25 · vida 4 · **red 1 (Escudo Baluarte)** · def 4 · regen 1 · vida 4 · def 4 · **red 2 (Inquebrável)** | defesa +20%, vida +16%, dano recebido −4%, regen de vida +2/s, firmeza +25 |
| Corsário | drop 4 · ouro 6 · minério 8 · Energia 8 · **drop 4 (Faro de Tesouro)** · Darksteel 8 · ouro 7 · saque melhor 1 · minério 7 · **drop 4 (Pilhagem)** · Energia 7 · Darksteel 7 · ouro 7 · saque melhor 1 · **drop 4 (Frota Dourada)** | drop +16%, ouro +20%, minério/Energia/Darksteel +15%, saque melhor +2% |

(% salvo indicação; regen em pontos/s.) Desvio do esboço: dano crítico, dano
contra chefe/jogador, dano de skill, área, recarga de skill, bloqueio,
espinhos, velocidade de montaria e bônus de baú ficaram de fora porque não
existe hoje o campo no `PlayerStats`/`Bonus` pra eles entrarem sem mexer no
combate; a tabela usa só efeitos que já têm porta (trocar um `Efeito` depois
é mexer só em `NOS` e em `Passivas`). **Ganchos:** fase 3 — variante
`RedistribuirComEssencia` no fim de `PedidoArvore` (o `match` em
`handle_arvore` está marcado); fase 4 — `No::ativa` = índice em `ATIVAS`
(nome por caminho), pronto pra virar skill dos slots 4–6. Ficaram pras
próximas fases: loja da Essência, ascensão, slots 4–6 e as ativas.

**Renascimento, fase 3 — ascensão de equipamento e loja da Essência — feita
(15/11/2026, versão de teste):** regras em `shared::essencia`.
**Ascensão:** campo novo `ItemInstance::estrelas: u8` (0..=`ESTRELAS_MAX` 5,
`#[serde(default)]`, ÚLTIMO campo — o fio postcard muda junto com a 199, e o
JSON antigo do banco lê 0; teste `json_antigo_sem_estrelas_carrega`). Cada
estrela = `BONUS_POR_ESTRELA` 8% dos atributos BASE da peça, SOMANDO com o
refino em `ItemInstance::effective_bonus` (base × (1 + 4%·refino + 8%·estrelas);
5 estrelas = +40%, o mesmo que 10 refinos) — é por ali que entra no
`effective_stats`, no poder da bolsa/Forja e no ranking; crítico da peça
(`crit_da_peca`) não muda. Sempre dá certo. Só equipamento
(`essencia::ascendivel`: tem slot e não é pet nem montaria — bicho tira força
do nível/afinidade dele, não dos atributos da peça). Custo
(`custo_da_estrela`), tirado da BOLSA:

| Estrela | Essência | Darksteel |
|---|---|---|
| 0 → 1 | 10 | 2.000 |
| 1 → 2 | 20 | 4.000 |
| 2 → 3 | 40 | 8.000 |
| 3 → 4 | 80 | 16.000 |
| 4 → 5 | 160 | 32.000 |
| total | 310 | 62.000 |

(310 de Essência ≈ a Essência dos 7 primeiros renascimentos; 7 peças cheias
≈ 20 renascimentos: a progressão longa.) As estrelas viajam com a instância
(troca, mercado, banco, correio); o Aprimorar (`craft::aprimorar`) fica com o
MAIOR número das duas peças; peça destruída no refino leva as estrelas.
Protocolo: `ClientMessage::Ascender { alvo: AlvoDaForja }` →
`world/essencia.rs` `handle_ascender` (confere peça ascendível, estrela < 5,
Essência e Darksteel; consome; recalcula stats e a vida máxima se vestida) →
`ServerMessage::AscensaoResultado { ok, estrelas, item_id, motivo }`. Tela: aba
**Ascend** na Forja (`forja_ui`, abas Refine | Ascend no topo; só peças
ascendíveis; estrelas, custo, poder e atributos antes › depois; sem
confirmação porque não falha). Cartão da bolsa mostra as estrelas desenhadas
(a fonte não tem ★: `bolsa::desenha_estrelas`) e a célula um selo "★n"; o
preço de referência do mercado só compara peças com o mesmo número de
estrelas.
**Loja da Essência** (`essencia::OFERTAS`, ids fixos no fio), aberta pelo
botão ESSENCE da Ficha (ao lado de CAMINHOS, a partir do 1º renascimento),
`client/loja_essencia_ui.rs`, com confirmação:

| id | Oferta | Essência |
|---|---|---|
| 1 | Redistribuir a árvore (sem ouro) | 20 |
| 2–5 | Chave lendária: Escama, Garra, Chifre, Couro (`chave_na_cor(base, 5)`) | 60 cada |
| 6 | 3 Amuletos de Proteção | 15 |
| 7 | Coroa (chapéu) | 40 |
| 8 | Cavaleiro (roupa) | 40 |
| 9 | Anel do Vazio (skin de anel) | 50 |
| 10 | Pelagem Espectral (skin de montaria) | 50 |

Visuais = skins que já existem (pagas na loja de TP), sem arte nova; chegam
como item de bolsa e destravam ao usar, como as da loja de TP. Expansão de
bolsa/banco ficou de fora (já se compra com ouro). `ClientMessage::LojaEssencia
{ oferta }` → `handle_loja_essencia`: confere a Essência, consome e entrega
tudo-ou-nada na bolsa (`dungeon::bolsa_tudo_ou_nada`), senão Correio (motivo 6,
`essencia::MOTIVO_CORREIO_LOJA_ESSENCIA`, "Loja da Essência"); resposta no
chat. A redistribuição é `PedidoArvore::RedistribuirComEssencia` (variante
nova no FIM; a loja chama o mesmo `handle_arvore`). Itens comprados não são
vinculados (item empilhável não tem o campo), como a chave do baú.
**Ganchos pra fase 4:** `essencia::OFERTAS` é append-only (skins exclusivas
dos marcos ou ativas extras entram como oferta nova com `Entrega` nova);
`ESTRELAS_MAX`/`custo_da_estrela` são o único lugar dos números, se o teto
subir; `Entrega` é o ponto pra ofertas que não são item. Os números (preços,
8%/estrela, custos) estão sem calibrar.

## A ideia

O jogo vira um laço curto e animado: subir do 1 ao 100 rápido, resetar,
ganhar poder permanente e subir de novo. A progressão longa vem do
**equipamento e da árvore**, não de um muro de XP. O motivo: muita gente
abandona no começo, e o começo era justamente o trecho mais lento (dados de
produção, 08–14/11: casuais 3–5× mais lentos que o melhor caso no 30–60).

## Regras

### Nível

- **Teto de nível: 100.** Tudo que hoje passa de 100 (Porão 102, Grutas 110 e
  118, a cadeia de Abyssia 100–117) vira a faixa **"100+"**.
- **Na faixa 100+ a métrica é o poder**, não o nível: o conteúdo pede um
  poder (a conta da ficha, `dungeon::poder_de_stats`). Mobs continuam podendo
  ter nível acima de 100 por dentro.
- **Sem trava de nível em equipar** (já não há desde a 198), **em craft**
  (`craft::conferir` ainda checa `nivel_min`) **nem em viagem** (a única é a
  Geleira no 20, `world.rs` `abrir_menu_viagem`). Quem passa a frear é o poder
  e o custo dos materiais.
- **PvP protegido por poder, não por nível** (`pk::pvp_mult_nivel`): um
  resetado de nível baixo com equipamento de 100 não pode caçar iniciante.

### Ritmo da primeira subida

- Alvo: **tryhard (15–16 h/dia) do 1 ao 100 em ~2 semanas.**
- Já é o ritmo atual: `RITMO_MAX` 1,25 (melhor caso 1→100 ≈ 170 h; tryhards
  reais rodam a ~1,3× o melhor caso ≈ 220 h). Não mexer na curva por ora.

### O reset

- Disponível no **nível 100**, num NPC (a definir qual).
- **Zera:** nível, XP, pontos de atributo distribuídos.
- **Mantém:** itens e equipamento vestido, ouro, TP, pets, montarias,
  proficiências, história concluída (as ilhas continuam abertas).
- **XP depois do reset:** continua pelo nível/poder do mob. O resetado caça o
  mais forte que aguenta e sobe muito rápido no começo (o custo de cada nível
  cresce com o quadrado), desacelerando sozinho — é a "sensação de poder" que
  o dono quer. Trocar o XP por **poder do mob** em vez de nível do mob é
  desejável, mas não bloqueia nada.
- **Divisor por reset** em cima do ritmo, pra cada subida pesar mais. Ainda a
  calibrar: `2^resets` dobra rápido demais pra "vários resets" (3ª subida ≈ 8
  semanas pro tryhard); sugestão `1,5^resets` (2, 3, 4,5, 7 semanas).
- **Bônus por reset** (do desenho de 12/11, mantidos): +3% atributos,
  +15% coleta / material / pó / Darksteel, e aura de corpo inteiro estilo DBZ
  (1 verde, 2 azul, 3 roxa, 4 dourada, 5 buraco negro, 6+ só intensidade).
- **Ranking de nível** ordena primeiro por resets, depois por nível.

### Árvore de caminhos (no lugar da subclasse)

- **Pontos:** 1 a cada 10 níveis, **a partir do primeiro reset** (10 pontos
  por subida).
- **Quatro caminhos.** Cada um é uma trilha de passivas que, indo fundo,
  libera **ativas**; a build pode misturar caminhos (passivas variadas) ou ir
  fundo num (chega nas ativas):
  - **Assassino:** crítico, velocidade, perfuração… → ativa de
    **invisibilidade**.
  - **Mago:** dano mágico, mana, área… → ativa de magia em área.
  - **Guardião:** defesa, vida, bloqueio… → ativa de provocar / escudo.
  - **Corsário:** pirataria e economia — saque (drop), coleta, mineração,
    ouro… → ativa ligada a saque/tesouro.
- As ativas da árvore ocupam **novos slots de skill (4, 5, 6)**, além das 3
  da arma.
- Redistribuir os pontos custa ouro.
- Nós, números e ativas exatas: a desenhar com o dono.

### Porões de resetado

- **Cortado.** Com o 100+ por poder e sem trava de nível/viagem, o desafio do
  resetado é o conteúdo 100+ que já existe (e estágios de poder maiores nele).

## Recompensas do renascimento (decidido 15/11/2026)

Substitui os "bônus por reset" de cima (+3% atributos, +15% coleta): com um
renascimento por semana no tryhard, ganho pequeno some. Cada renascimento
precisa de um ganho que se sente na hora, uma meta visível e um motivo pra
seguir depois do 10º. Quatro camadas:

1. **Ganho fixo por renascimento:** +5% em todos os atributos, somando sem
   teto (+50% no 10º) — o 100+ é por poder, então cada renascimento abre uma
   faixa nova dele. Mais os 10 pontos da árvore (1 a cada 10 níveis da nova
   subida) e um **baú do renascimento** na entrada (Darksteel, chave e
   material na cor do renascimento).
2. **Marcos:**

   | Renascimento | Marco |
   |---|---|
   | 1 | Aura verde + abre a árvore + slot de skill 4 |
   | 2 | +5 espaços na bolsa |
   | 3 | Aura azul + slot de skill 5 + título "Renascido" |
   | 5 | Aura roxa + slot de skill 6 + skin de montaria exclusiva |
   | 7 | +5 espaços no banco + cor de nome |
   | 10 | Aura dourada + ativa suprema de cada caminho + skin de arma |
   | 15 | Aura buraco negro + título "Imortal" |
   | 20+ | Aura ganha intensidade a cada 5 + título com o número |

3. **Essência do renascimento:** cada renascimento dá Essência (mais nos
   renascimentos altos). Loja da Essência: redistribuir a árvore, visuais,
   chave lendária e **ascensão de equipamento** (estrelas que sobem os stats
   da peça — o equipamento segue sendo a progressão longa, alimentado pelo
   renascimento).
4. **Status:** ranking por renascimentos, "R7" ao lado do nome, aviso global
   ao passar de um marco.

**Equilíbrio:** a subida fica mais pesada a cada renascimento (divisor
sugerido 1,5^renascimentos), mas o +5% e a árvore matam mais rápido; a
calibrar no teste pra cada subida ficar parecida com a primeira.

## Mundo novo: 1 ao 100 em uma semana (decidido 15/11/2026)

O dono: "colocar realmente a progressão bem rápida, tipo 1 semana no
tryhard pro lvl 100", com o mundo encurtado e mais denso.

- **Faixas por ilha (níveis NOVOS):** Bosque 1–30, Geleira 30–50, Ermo
  50–70 (precisa de muito mais mob), Planalto 70–100. Celeste, Kōgen e
  Abissal: "100+" por poder.
- **Dentro da ilha o nível sobe do porto (norte) pro sul.**
- **A força não muda (opção b):** o 100 novo tem a força do 60 de hoje (fim
  do Planalto). Uma conversão única nível novo → antigo, por faixas
  (1–30 → 1–15, 30–50 → 15–30, 50–70 → 30–42, 70–100 → 42–60), alimenta tudo
  que é força (ladder, mob, item, chave, drop, poder de referência), pra
  balança calibrada ilha por ilha continuar valendo.
- **Ilha Mágica:** continua como está; só a trava dela muda de nível pra
  poder.
- **Curva de XP:** melhor caso 1→100 ≈ 75–80 h (tryhard ≈ 100 h reais ≈ 1
  semana).
- **Missões:** história e missões de ilha vão pros níveis novos; Ermo e
  Planalto ganham missões pros níveis que ganharam; linha de missões do
  reset (até o NPC no 100, e a de volta apresentando a árvore).
- **Personagens existentes:** ficam com o nível/XP que têm, e **todo mundo
  volta pra ilha inicial**; cada um descobre onde aguenta upar.
- **Lançamento:** nada vai pra produção agora. Tudo é planejado e testado na
  versão de teste; o dono decide depois quando lançar.

### Como foi feito (15/11/2026, versão de teste)

- **As faixas novas são as de hoje renumeradas**, então o jogo continua com
  os níveis INTERNOS (mob, chefe, dungeon, missão, item, chave, curva de XP)
  e só a TELA converte: `progressao::ANCORAS_DA_ESCALA` (1→1, 15→30, 30→50,
  42→70, 60→100), `nivel_exibido(xp)`, `xp_do_nivel_exibido(n)` (barra de
  XP), `texto_de_nivel` / `texto_de_faixa` ("100+" acima do 60 interno).
  Personagens existentes ficam coerentes sozinhos.
- **Teto:** `NIVEL_MAX` = 60 interno (aparece 100); `NIVEL_MAX_EXIBIDO` = 100.
- **Ritmo:** o 1,25 atual já dá ~57 h no melhor caso até o 60 interno (~5
  dias de tryhard a 15 h/dia): a curva não precisou mudar.
- **Ilha Mágica por poder:** `NivelMagico::poder_exigido` (poder de
  referência do nível que abria o andar: I ≈ 2 mil, VI ≈ 11,8 mil);
  `maior_liberado(poder)`.
- **Ermo mais denso:** `world::densidade_da_ilha` — hordas a 70 u (eram 90)
  e 26 mobs por horda (eram 18): ~2,4× mais mob.
- **Degradê dentro da ilha:** já existe — o nível das zonas sobe com a
  distância da cidade (a mais fraca perto do porto/cidade).
- **Missões novas:** Ermo 1100–1109 (8 em cadeia + 2 contratos), Planalto
  1110–1129 (14 em cadeia + 3 contratos), no Mestre de Missões, com
  tradução.
- **Voltar todo mundo pra ilha inicial:** `scripts/voltar-ilha-inicial.sh`
  (com o jogo PARADO; `--sim` pra ver antes). Não roda na produção até o
  dono decidir lançar.

## Etapas

1. **Release A — base (ajuda já quem está abandonando):** teto 100, "100+" por
   poder, sem trava de nível em craft/viagem, PvP por poder.
2. **Release B — o reset:** coluna `resets` no personagem, NPC, divisor por
   reset, bônus de atributo e coleta, aura, ranking, e a árvore com as
   passivas.
3. **Release C — as ativas:** slots 4–6 e as ativas de cada caminho.

## Armadilhas mapeadas no código (14/11)

- **Skills em array fixo:** `ProgressoDeSkills { tiers: [u8; 12], … }` é JSON
  com tamanho fixo; crescer o array sem migração faz o `unwrap_or_default` do
  load **zerar as skills de todo mundo**.
- **Slots de skill fixos em 3:** `skills::DESTRAVA_EM`, `destravada()`,
  `client/habilidades.rs` (`0..3`), `desktop.rs` (teclas 1–3), e
  `world.rs` (`.min(2)` no gesto).
- **Admin `SetLevel`** (molde do reset) não manda `ProgressUpdate` nem marca
  `nivel_mudou`: copiar como está deixa o cliente com o nível velho.
- **Coisas derivadas do nível que mudam quando ele cai:** firmeza no 60,
  bônus da pistola (`nivel/3`), `for_da_armadura`, acesso à Ilha Mágica,
  `min_level` de missão, preço do perdão de PK, perda de XP na morte.
- **Aura e ranking por reset precisam de campo novo** no protocolo (o `u64`
  de auras do `EntityMeta` está cheio) → sobe `PROTOCOL_VERSION`.
- **Cópias da conta de stats** em `pvp_sim.rs`, `balanceamento.rs` e
  `world/ranking.rs` chamam `effective_stats` / `destravada` direto: todas
  precisam do número de resets.
- **Poder de referência das dungeons usa a ladder**, que subestima o jogador
  real (~1,6×, medido com `server --poder`): calibrar antes de travar o 100+
  por poder.
