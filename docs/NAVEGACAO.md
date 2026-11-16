# NAVEGAÇÃO — o mar como sistema (plano, 15/11/2026)

> Status: **PLANO, nada implementado.** O dono escolheu a "opção C": navio
> próprio, mar navegável entre as ilhas, upgrades, carga e PvP do Corsário.
> Este documento é para ele aprovar fase por fase antes de qualquer código.

## A lição de setembro

O jogo já teve um mar aberto completo (commits `1b5ea33`…`3cd9ff4`, removido
em `95c5d3a`, 21/09/2026; o desenho antigo está em
`git show 95c5d3a^:docs/MAR_ABERTO.md`). Ele caiu por **um** motivo: trocar
de ilha **exigia** navegar. Vários minutos de pedágio numa coisa que se faz
toda hora — *"essa mecânica tá furada"*.

O que funcionou e volta:

- **Navegar é andar, só que na água.** Mesmo direcional, mesma câmera, mesma
  rota no mapa. O barco com leme e acelerador (20/09) perdia "o referencial de
  frente e trás" e foi cortado.
- **A ilha vista do mar é uma silhueta.** Um domo por ilha e um cais
  marcando a entrada, sem gerar o relevo inteiro, o que fica barato no
  celular.
- **No mar quem apanha é o casco.** Bicho do mar rói o casco: a resposta é
  desviar, correr ou atirar, e é isso que dá sentido ao reparo e ao upgrade.
- **O tesouro nunca é destruído.** Afundar quem carrega faz o baú boiar;
  matar em PvP o entrega ao vencedor. Negar o prêmio nunca pode ser a jogada
  vencedora.
- **Corredor da Capitania:** perto do cais ninguém apanha. A perseguição é o
  jogo; a porta não é.

## A regra nova que rege tudo

> **O mar é opcional. Nada de progressão obrigatória passa por ele.**

- O **Capitão do Porto continua levando de graça** de uma ilha pra outra,
  como hoje.
- O mar existe porque **rende**: recursos, baús, carga, chefes e PvP que só
  existem lá.
- Material exclusivo do mar serve só pra coisa **do mar** (navio, cosméticos)
  ou é atalho, nunca a única fonte de algo que a progressão de terra pede.

## Fases

### Fase 1 — O mar e o primeiro navio (fundação)

- **Zona nova `mar_aberto`**, um processo, centrada na própria caixa. O fio
  aguenta ±4096 u de posição e ~15,9 u/s de velocidade; há teste que falha se
  o mar não couber. As 4 ilhas com porto (Bosque, Geleira, Ermo, Planalto)
  aparecem como silhuetas com cais.
- **Zarpar e atracar:** no cais, o Capitão ganha "Zarpar com o meu navio"
  além da viagem grátis. No mar, chegar a um cais oferece "Atracar" →
  reconexão na zona da ilha (o mesmo `TrocarZona` de hoje). Se o processo do
  mar cair, o login volta pra ilha de onde zarpou (`zona_volta`).
- **O navio é um item** num slot próprio (um navio ativo, como a montaria). O
  primeiro, a **Chalupa**, sai de uma missão secundária do Bosque, nunca da
  história principal.
- **O que fazer no mar:**
  - **destroços boiando** (coleta: madeira, aço, darksteel, energia, na
    faixa de nível do trecho);
  - **cardumes** (pesca simples: AUTO COLETA no barco);
  - **baús à deriva** que nascem de tempos em tempos.
- **Perigo:** bichos do mar que roem o casco (tubarão, enguia) e **recifes**
  em volta das silhuetas. Casco a zero → o navio fica "avariado", você
  reaparece no último cais e conserta lá (em ouro).
- **Reaproveitado:** `agua.rs` (oceano 3D com ondas e anel do horizonte, já
  no jogo), `terreno::mover_casco`/`Ilha::mover_no_mar` (o casco só anda na
  água), `rumo_do_porto`.
- **Tamanho:** grande. É a fase que cria zona, processo, unidade systemd,
  rota nginx `/z/mar_aberto` e o modo de render do mar.

### Fase 2 — O navio como progressão

- **Cascos por classe:** Chalupa → Escuna → Galeão → Nau, feitos no Estaleiro
  (craft com madeira T1–T4, aço e um material exclusivo do mar, a **Prancha
  de Naufrágio**).
- **Upgrades em eixos:** Casco (vida), Vela (velocidade), Canhão (dano) e
  Porão (espaço de carga). Usar o mesmo padrão de **estrelas** da ascensão
  (`shared::essencia`) para não inventar uma terceira progressão.
- **Skills do navio:** canhonaço (frente), bordada (lados), reparo de
  emergência e fumaça (fuga). Usam a barra de skills enquanto se navega.
- **Cosméticos:** velas, figura de proa e rastro, à venda na loja de TP e na
  de Essência. Sem arte nova pesada: cor da vela e um punhado de
  figuras-de-proa em voxel.

### Fase 3 — Carga e comércio entre ilhas (CORTADA pelo dono, 15/11/2026: "não precisa ter a parte do comércio, faz só a navegação e PvP")

- Cada porto compra e vende **mercadorias** (madeira de lei, peles, gelo,
  especiarias…) com preço próprio. Comprar barato numa ilha e vender caro em
  outra dá lucro.
- A carga vai no **Porão** do navio (bolsa, banco e correio recusam): é o que
  faz "leve até a outra ilha" ser verdade.
- **Economia:** o lucro tem que ser medido como fonte de ouro. A proposta é
  que o NPC compre com um teto diário por personagem e que parte do valor
  seja taxa do porto (sumidouro). Há teste de "ouro por hora no comércio ≤ X
  do farm de terra".

### Fase 4 — Corsário e PvP naval

- **Quem carrega, arrisca:** navio com carga ou tesouro fica **marcado**
  (atacável por qualquer um, mesmo PK Pacífico). Sem carga, o mar segue os
  modos de PK de hoje (`shared::pk`).
- **Afundar quem carrega:** a carga boia nos destroços (de quem chegar
  primeiro). Vencer em PvP direto entrega a carga ao vencedor. O navio nunca
  é perdido, só avariado.
- **Baú do Colosso:** chefes do mundo deixam um tesouro que só cabe no
  convés. Abrir onde caiu paga ×1,0; levar até outra ilha paga mais (até
  ×3), e a travessia é a chance de perder.
- **Zonas sem lei:** poucas e com motivo (naufrágios no meio das rotas).
- **Sinergia com o caminho Corsário** (`shared::arvore`): passivas do mar
  (+dano de canhão, +carga, +saque de destroço, abordagem mais rápida).
  Hoje o Corsário é só loot e coleta; o mar dá o tema que faltava.
- **Karma** pra quem mata inocente sem carga: reaproveitar os pontos de PK
  atuais em vez de criar um sistema novo.

### Fase 5 — Conteúdo do mar

- **Mapas do tesouro** (drop raro) que apontam ilhotas secretas só
  alcançáveis por navio.
- **Evento do Navio Fantasma** (horário, como o fim de semana).
- **Chefe do mar, o Leviatã:** morde o casco num ritmo fixo; a luta é dano
  contra resistência do navio, não desvio de golpe. Fica fora de
  `bosses::CHEFES` de propósito, porque aquela lista exige golpe esquivável.

## Restrições técnicas

- **Cada ilha é uma zona e um processo.** Trocar de zona é reconectar
  (`mandar_para_zona` → `ServerMessage::TrocarZona`). O mar é mais uma zona,
  não um mundo contínuo com as ilhas.
- **Fio:** posição `i16` com `POS_SCALE` 8 (±4096 u) e velocidade `i8`
  (±15,9 u/s). O casco mais rápido fica abaixo disso, com teste.
- **Celular:** silhuetas analíticas, nada de relevo Perlin das ilhas no mar.
- **Skyreach, Kōgen-tō e Abyssia não têm porto** (`tem_porto`): ficam fora do
  mar, como hoje.
- **Dados:** o navio vive na instância do item (como pet e montaria),
  então trocar e vender no mercado leva o que foi investido.

## Decisões do dono (15/11/2026)

1. **Capitão:** a viagem rápida entre ilhas **passa a custar cobre**. Ela
   continua existindo, mas o mar ganha motivo.
2. **PvP no mar:** o **mar inteiro segue os modos de PK** de hoje
   (`shared::pk`: Pacífico, Grupo, Clã, Livre). A "marca" de quem carrega
   carga ou tesouro da fase 4 é por cima disso.
3. **Primeiro navio:** a **missão da história** que tira o jogador da
   primeira ilha entrega a Chalupa. **Jogadores antigos** (que já passaram
   desse passo) recebem um barco de graça por migração. O **Estaleiro** faz
   o craft e os upgrades dos cascos seguintes.
4. **Ordem das fases:** **1 → 2 → 4**. A fase 3 (comércio) foi cortada; a 5 (conteúdo do mar) fica para depois. Sem comércio, a "carga" da fase 4 é só o tesouro (Baú do Colosso e baús do mar).
5. Material exclusivo do mar: por padrão, só pra navio e cosmético (não foi
   perguntado; vale a regra "o mar é opcional").

Com a Chalupa vindo da história, a regra "o mar é opcional" continua de pé
porque o Capitão segue levando (agora por cobre): ninguém é obrigado a
navegar pra avançar.
