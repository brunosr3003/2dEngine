# O MAR ABERTO

Desde 21/09/2026 **trocar de ilha exige navegar**. O teleporte grátis do
Capitão do Porto acabou; a quest que monta a Chalupa é a porta de saída da
primeira ilha, e o porto virou oficina.

O barco anterior tinha sido apagado em 20/09 de propósito: era um veículo
completo do lado do servidor — vela, leme, vento, âncora, canhões com mira,
convés com estações — e **o cliente não tinha uma linha disso**. Pior, não
havia pra onde ir. Este documento é o que veio no lugar.

## A regra que rege tudo

> **Nenhum estado alcançável pode deixar o jogador sem poder navegar.**

Ela é a razão de melhoria e reparo nunca falharem, do piso de graça existir, e
de o baú nunca ser destruído. Com a travessia obrigatória, qualquer buraco
aqui vira conta travada.

## Navegar é andar, só que na água

Não existe entidade barco. A primeira versão tinha uma — casco com vida
própria, leme, acelerador, e o passageiro com a posição derivada dele — e o
dono testou e foi direto ao ponto: *"a navegação tá perdendo o referencial de
frente trás que tinha antes"*. Perdia mesmo: leme e acelerador são um esquema
de controle **novo**, e o jogador já sabia um.

E, decidido que combate no mar é **PvP entre barcos** e não luta de convés, o
barco deixou de precisar ser um lugar onde se anda. Sobrou:

- o passo do jogador ganhou um ramo: na ilha colide pela regra de degrau, no
  mar colide com **terra** em vez de água (`terreno::mover_casco`);
- a velocidade é a do casco;
- o cliente desenha um casco no lugar do boneco.

Direcional, câmera, rota — tudo idêntico a andar em terra.

## A ilha vista do mar é uma silhueta

O terreno do mar **não** é o das ilhas. Foi, e o dono cortou: *"não quero que
as ilhas se materializem inteiras no open world; quero uma mega simplificação
da ilha e um porto mostrando onde entra"*.

Três motivos de uma vez: de longe o navegante só precisa saber que há terra e
por onde se entra; ruído de Perlin em quatro ilhas num celular pra desenhar um
borrão é caro; e o mar não é a ilha — quem quer a ilha, atraca.

Então é **analítico**: um domo por ilha e um cais marcando a entrada. Uma
conta de distância por coluna. Os `Gerador`es ainda nascem uma vez no boot,
mas só pra perguntar **onde fica o porto** — o único dado do mundo real que o
mar precisa.

A ilha é parede e o cais é a porta, e isso sai de graça do desenho: o casco só
anda na água. O **ancoradouro** (onde se nasce ao zarpar) é a água logo depois
da ponta — nascer dentro do tabuado deixaria o jogador entalado.

## O que o fio aguenta

- `POS_SCALE` foi de 1/16 pra 1/8 de tile. Apertava em **dois** lugares:
  posição em `i16` cobria ±2047 u (e o Planalto mora em x = −3600), e
  velocidade em `i8` cobria ±7,94 u/s — um casco a 11 u/s já saturava.
- O mar nasce centrado na própria caixa, e há teste que **falha** se ele não
  couber. `quantize` clampa em silêncio: sem o teste, entidades além do teto
  empilhariam na borda sem um erro em log nenhum.
- `flags` virou `u16` pro nono bit (`MARCADO`). A alternativa de graça era
  reaproveitar `MONTADO`, que não vale no mar — custaria zero byte e um bug
  por ano, porque toda leitura de flag passaria a depender de onde o jogador
  está.

## O barco como item

Três cascos (Chalupa, Escuna, Nau) cujos ids guardam a **classe**, não a cor —
a quebra deliberada com pet e montaria. Slot próprio: exatamente um barco
ativo, sem diálogo de "qual barco?" no cais.

`BarcoData` na instância: casco, melhorias, travessias, naufrágios e o baú no
convés viajam com o item. Barco vendido no mercado leva junto tudo o que o
dono investiu.

**O casco está em escala de navio.** A primeira versão deu 400 de vida pra
Chalupa — três vezes um jogador — e a simulação mostrou o que isso significava:
dois peixes afundavam a travessia em treze segundos. São 2.400, e o casco da
faixa aguenta entre 78 e 113 s de mordida: dá pra virar a proa, não dá pra
atravessar por dentro de um cardume.

Três eixos: **casco**, **vela**, **canhão**. Houve um quarto — porão — e ele
saiu antes de nascer. Nada falha, nada decai.

## No mar quem apanha é o casco

Bicho de mar não é encontro de combate: é **perigo**. Ele rói o casco, e a
resposta é desviar ou correr. É isso que dá sentido ao eixo casco e ao reparo —
sem dano no casco os dois seriam número morto.

## O tesouro e a marca

Todo chefe de mundo larga um **Baú do Colosso**, garantido. Ele não cabe em
lugar nenhum a não ser no convés: bolsa recusa, banco recusa, mercado recusa,
correio recusa. São **quatro recusas**, e é isso que faz "quer o tesouro na
outra ilha? navega com ele" ser verdade em vez de uma frase aqui.

Abrir onde caiu paga ×1,0 — quem não quer PvP nenhum leva um prêmio justo de
chefe. Cada travessia multiplica até ×3,4, e cada travessia é uma chance de
perder tudo.

**A marca é derivada**, não guardada: você está marcado enquanto houver baú no
convés. Não há campo pra dessincronizar nem pra esquecer de limpar. O único
pedaço guardado é o rastro de um minuto depois da entrega — sem ele, entregar
um segundo antes do golpe é um drible.

Ela **vence a safe zone**, e por isso a posição do atacante também deixa de
proteger: senão bastava ficar no cais atirando de dentro da bolha.

### As duas mortes

| | barco | baú |
|---|---|---|
| casco a zero | avariado | **boia nos destroços**, de quem chegar |
| morte em PvP | intacto | **vai pro assassino** |

O baú nunca é destruído. Afundar o tesouro faria a estratégia vencedora ser a
**negação** — bastava furar o casco de quem carrega pra ninguém levar nada.
Soma negativa, e um único mau ator tornaria a travessia inútil.

### O corredor da Capitania

Um raio pequeno em volta dos NPCs do cais onde ninguém apanha, marcado ou não.
Vem **antes de tudo**. Sem ele, campar a entrega é a meta inteira e nenhum baú
chega. **A perseguição é o jogo; a porta não é.**

## Karma

Só a **morte** cobra, não o toque: área respinga em quem passa, e punir um
corte perdido seria injusto e ilegível. Só na mesma facção — facção é guerra
consentida. Decai com tempo **online**: deslogar não pode ser a forma barata de
limpar a ficha. Uma morte custa uma hora de jogo.

A punição com mais dentes é também a mais barata: o Criminoso perde o
atendimento de NPC. Sem guarda com IA — ele simplesmente não consegue
**consertar o casco**, e barco que não repara não navega.

E no topo da escala o assassino vira exatamente o que a vítima dele era: PK
aberto. **Um estado, duas portas de entrada** — carregar tesouro, ou matar
inocente.

## Zonas sem lei

Poucas, e com motivo pra parar nelas: os naufrágios de cada rota (onde
carregador e caçador se encontram de qualquer jeito) e a travessia de endgame
inteira. As outras três rotas seguem governadas por karma — elas são
**obrigatórias** pra progredir, e transformar o caminho obrigatório em terra de
ninguém é cobrar imposto por existir.

## O Leviatã

Ele **não** entra em `bosses::CHEFES`, de propósito: aquela lista passa por
`todo_telegrafico_e_esquivavel`, que pergunta se dá pra andar pra fora do golpe.
A pergunta não faz sentido no mar — ninguém anda, quem se move é o casco.

Então ele não tem golpe pra desviar: morde o casco num ritmo fixo, e a luta é
uma corrida entre o dano dele e o seu. Quem decide não é o reflexo — é com que
barco você veio.

## Se o processo do mar cair

O modo de falha novo e severo: navegando, a zona salva é `mar_aberto`, e o
seletor de canal não acha canal servindo ela. A coluna `zona_volta` grava de
onde o jogador zarpou, e o login cai em cascata de volta pra lá.

## Os guardas

- `casco_so_anda_na_agua`, irmão de `nao_anda_sobre_a_agua`: as duas regras
  existem separadas, e um teste ao lado do outro impede alguém de
  "simplificar" as duas numa com um booleano.
- `o_mar_inteiro_cabe_no_orcamento_do_fio`.
- `a_ilha_e_parede_e_o_cais_e_a_porta`.
- `metas_do_mar`: quanto tempo o casco da faixa aguenta sendo mordido. Tem
  **piso e teto** — um guarda que só pega "difícil demais" deixa passar o
  oposto, e foi o que aconteceu na primeira calibragem.
- `a_chalupa_nao_pede_chave_nem_darksteel`: a porta de saída da primeira ilha
  não pode exigir item que só cai de chefe. Este pegou um de verdade — a
  receita pedia Couro, que **é** uma das quatro chaves.
