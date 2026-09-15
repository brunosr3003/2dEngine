# Chefes de campo

Um chefe e' um mob preset no modelo de corpo inteiro, maior, com nome proprio e
um kit de golpes TELEGRAFADOS: ele para, a forma do golpe aparece no chao e
cresce ate' o impacto; quem ainda estiver dentro no instante do impacto toma.
O servidor decide com a posicao dele — o desenho do cliente e' so' aviso.

Catalogo e contas puras: `crates/shared/src/bosses.rs`. Lugar, respawn,
golpes e loot no servidor: `crates/server/src/world/chefes.rs`. Desenho:
`crates/client/src/telegrafico.rs`.

## Lista

| Kind | Chefe | Corpo | Escala | Ilha | Nivel | Golpes (fase 2 = abaixo de 50%) |
|---:|---|---|---:|---|---:|---|
| 10 | Lobo Alfa da Clareira | lobo | 2,3 | Bosque | 8 | Mordida Dilacerante (cone), Investida (linha), *Uivo da Matilha (anel)* |
| 11 | Capitao Barba-Tormenta | pirata (corpo + chapeu) | 1,8 | Bosque | 11 | Corte de Sabre (cone), Tiro de Canhao (circulo no alvo), *Barragem (circulo grande no alvo)* |
| 12 | Urso Anciao | urso | 2,0 | Bosque | 14 | Patada Larga (cone), Pisao Sismico (circulo em si), *Rugido Esmagador (anel)* |
| 13 | Tigre das Neves | tigre | 2,2 | Geleira | 24 | Garras em Leque (cone), Salto Predador (circulo no alvo), *Rodopio (circulo em si)* |
| 14 | Lobo da Tempestade | lobo grande (corpo inteiro) | 1,0 | Geleira | 30 | Investida Trovejante (linha), Uivo da Tempestade (anel), *Relampago Caido (circulo no alvo)* |
| 15 | Saqueador das Areias | pistoleiro | 1,7 | Ermo | 36 | Linha de Tiro (linha), Rajada em Leque (cone), *Barril Explosivo (circulo no alvo)* |
| 16 | Arqueira do Ermo | arqueiro | 1,7 | Ermo | 41 | Flecha Perfurante (linha), Chuva de Flechas (circulo no alvo), *Armadilha Espinhosa (anel)* |
| 17 | Owlbear Primevo | owlbear | 2,0 | Planalto | 52 | Patada Dupla (cone), Giro Selvagem (anel), *Queda Estrondosa (circulo no alvo)* |
| 18 | Arquimago da Tormenta | mago | 1,7 | Planalto | 60 | Raio Arcano (linha), Meteoro (circulo no alvo), Nova de Gelo (circulo em si), *Anel de Chamas (anel)* |

Os tempos exatos (carga 0,8–2,0 s, recarga, alcance, multiplicador, empurrao)
estao no catalogo.

## Numeros

| O que | Regra |
|---|---|
| Vida | 8.000 + 240 × nivel, teto 18.500 (o dano do jogador cresce com o equipamento da faixa) |
| Golpe comum | 12 + 2 × nivel, mitigado normalmente; cadencia minima 2,4 s (`CADENCIA_COMUM_S`) |
| Golpe telegrafado | tira `dano_mult` × 12% da VIDA MAXIMA de quem ficou dentro (1,8 → 22%, 3,4 → 41%); a resistencia NAO vale contra ele (`RESISTENCIA_NO_TELEGRAFICO = 0`); fase 2 ×1,15 |
| Roubo de vida | o chefe cura 25% de todo dano que causa (regra antiga do servidor, mantida) |
| Defesa | nivel ÷ 2 |
| XP | 40 × nivel (o servidor ainda da' 5× por ser chefe) |
| Respawn | 600 + 10 × nivel s (Bosque ~11–12 min, Planalto ~20 min) |
| Fase 2 | vida ≤ 50%: golpe de fase 2 liberado, carga ×0,85, recarga ×0,75 |
| Pausa | 2,2 s depois de cada impacto antes do proximo telegrafado |
| Loot extra | cobre 200+25×nv, darksteel 20+4×nv, aço na cor da faixa, 60% platina, 25% po cintilante, 2 pocoes de vida maiores, **chave de craft** na cor da faixa do chefe, com a chance de chefe do mundo (5% cinza ate' nv 19, 3% verde 20–39, 2% azul 40–59, 1% epica 60–79, 0,3% lendaria 80+; `shared::chaves`) — sem equipamento |

Cor da faixa: ate' nv 14 cinza, 15–29 verde, 30+ azul (roxo so' por sintese).

## Onde nascem

Um sitio plano numa grade grossa da ilha, a 130+ de qualquer zona segura
(cidade, porto) e 150+ de outro chefe. Os chefes da ilha, do mais fraco pro
mais forte, caem espalhados do perto pro longe da cidade. Deterministico: a
mesma ilha poe os chefes nos mesmos lugares. Aparecem no mapa grande e no
minimapa com uma coroa, nome e nivel ("renascendo" enquanto mortos).

## Golpe telegrafado

1. Com um alvo no alcance e um golpe pronto (e passada a pausa), o chefe
   escolhe o golpe (fase alta primeiro, depois o mais forte), fixa centro e
   direcao (no chao do alvo, em si, ou a frente) e para.
2. `ServerMessage::Telegrafico { id, chefe, forma, centro, dir, carga_s }` vai
   pra quem esta' perto (AOI × 1,5 + tamanho da forma). O chefe fica virado
   pro golpe (rumo no fio) e nao da' golpe comum.
3. No impacto o servidor testa `Forma::contem` com a posicao ATUAL de cada
   jogador vivo e aplica dano/empurrao pelo caminho normal de hit de skill.
   `TelegraficoFim { id, impacto }` apaga o aviso (cancelado se o chefe
   morrer).

Formas: circulo, cone (meio-angulo), linha (sai do centro pra `dir`) e anel
(seguro colado e longe). Mede o centro do corpo.

## Cliente

- Aviso no chao rente ao relevo: fundo vermelho fraco com a forma inteira,
  contorno vermelho, e o preenchimento que cresce do centro (linha: da origem;
  anel: de dentro pra fora) ate' o impacto, esquentando no fim; clarao de
  0,3 s no impacto.
- Chefe desenhado no corpo preset na escala do catalogo (bicho, gente ou o
  corpo do personagem com chapeu pro pirata).
- Painel de alvo com "CHEFE" e a fase; sem o chefe selecionado, uma barra de
  chefe no mesmo lugar quando um chefe em luta esta' a 30 de distancia.
- Protocolo 92.

## Visual e animacao

Regra de design: o auto combate NUNCA desvia de telegrafico — sair da forma
e' a graca da luta, na mao.

**Golpe em tres tempos** (`crates/client/src/chefe_anim.rs`), disparado pelo
`Telegrafico` (o `chefe` que vem nele) e acertado no `TelegraficoFim`:

| Forma (mira) | Gesto | Prepara (carga inteira) | Golpe (0,30 s) | Recupera (0,75 s) |
|---|---|---|---|---|
| Linha | Investida | agacha, recua o tronco, cabeca baixa | dispara pra frente ate' 55% da linha (max 7 u) | volta pro lugar |
| Circulo em si | Pisao | empina alto | desce e esmaga o chao (achata) | assenta |
| Circulo no alvo | Salto | agacha fundo | salta em arco e cai no circulo | volta |
| Cone | Varrida | torce pro lado; a pata arma | varre de um lado ao outro (pata do bicho / braco da gente) | desfaz a torcao |
| Anel | Giro | arma o giro | da' a volta inteira | assenta |

- Tremor crescente nos ultimos 45% da carga (o "vai sair agora").
- Bicho: o ajuste entra no tronco (`bicho::Corpo`) e no corpo inteiro; a
  varrida usa o relogio da patada. Gente (saqueador, arqueira, arquimago) e o
  pirata: o corpo inteiro inclina/gira/avanca e o braco arma ate' quase o
  impacto e so' completa no golpe.
- Morte de chefe: tombo com o dobro do tempo e nuvem de poeira quando bate no
  chao.

**Telegrafico** (`telegrafico.rs`): na carga, fundo fraco + preenchimento com
frente clara correndo na ponta + borda dupla (traco escuro por baixo pra
contraste em neve/areia, traco vivo e brilho de dentro) + setas andando na
linha e no cone; a borda pisca nos ultimos 0,3 s. No impacto: clarao (0,3 s),
onda de choque saindo da borda (0,45 s) e marca de poeira com rachaduras que
some em 1,4 s. Malhas com no maximo 3.000 vertices.

**Efeitos e presenca**: rajadas no impacto em pontos DENTRO da forma, na cor
do elemento do chefe (fogo: pirata e saqueador; gelo: tigre; raio: lobo da
tempestade; arcano: arquimago; espinho: arqueira; terra: urso e owlbear),
pelo pool fixo de `lascas.rs`; tremor de camera curto (0,35 s, teto 0,22 u) se
o jogador esta' a ate' 8 u da forma. Chefe vivo tem sombra larga e aura de
faiscas lentas na cor do elemento; placa sobre a cabeca maior, com moldura
dourada e coroa; barra de chefe com moldura, coroa, marca da fase 2 e a faixa
amarela de vida perdida descendo devagar.

**Modelo**: o Lobo Alfa usa o lobo de corpo inteiro (o detalhado) em vez do
lobo pequeno escalado (a escala e' corrigida no cliente).

## Balanceamento

Regra de design: o auto combate NUNCA desvia de golpe telegrafado — desviar na
mao e' a graca. Quem esquiva vence; quem fica parado dentro do aviso perde.

Simulacao: `crates/server/src/balanceamento.rs` (`duelar`), mesmas funcoes do
servidor (stats, mitigacao, cadencias, skills em AUTO, pocoes com recarga por
grupo, regen) e do catalogo (escolha de golpe, carga, fase 2, recargas, pausa,
formas), com o roubo de vida do chefe.

```sh
cargo test -p server --bin server balanceamento -- --nocapture
```

**Jogador simulado (build do nivel):** 3 pontos por nivel, um terco no
atributo da arma (FOR corpo a corpo, DES pistolas, INT anel) e o resto em VIT;
proficiencia da arma no nivel do personagem; equipamento completo da faixa pelo
nivel (cinza < 15, verde < 30, azul < 60, roxo 60+) com rolagem media: arma,
secundaria do conjunto, armadura (pesada com espada e escudo, media com katana,
leve com pistolas e anel) e os quatro acessorios.

**Perfis:**
- ESQUIVA: reage 0,35 s depois do aviso e sai pela saida mais curta da forma
  (16 rumos) na velocidade normal, sem correr, com 0,3 de folga alem da borda;
  durante a carga nao se aproxima. Se nao da' tempo, toma.
- PARADO: ignora o aviso (e' o auto combate).

**Metas (asserts):**
1. ESQUIVA + pocao no nivel do chefe: os 4 conjuntos vencem, HP minimo ≥ 15%,
   luta de 60 a 240 s.
2. PARADO + pocao: perde, ou termina com HP minimo ≤ 10%.
3. ESQUIVA + pocao dois niveis abaixo: pelo menos 3 dos 4 conjuntos vencem.
4. ESQUIVA sem pocao: so' impresso (e' pra ser apertado).
5. Todo golpe e' esquivavel de qualquer ponto de dentro, na fase dele e na
   fase 2: carga ≥ 0,35 s + pior saida ÷ velocidade (`todo_telegrafico_e_esquivavel`).
6. Golpe comum nunca derruba um jogador cheio do nivel de uma vez.

**O que mudou:**

| Onde | Antes | Agora | Por que |
|---|---|---|---|
| Telegrafado | `dano base × mult`, mitigado | `mult × 12%` da vida maxima, sem resistencia | a espada e escudo batia no teto de 90% de mitigacao e vencia parada todos os chefes |
| Golpe comum | 18 + 3×nv na cadencia do bicho (tigre 1,0 s) | 12 + 2×nv, cadencia minima 2,4 s | o golpe que nao se esquiva matava Pistolas e Anel em ~12 s esquivando tudo |
| Vida | 900 + 420×nv | 8.000 + 240×nv, teto 18.500 | luta de 30 s no Lobo Alfa e 357 s pro Anel no Arquimago |
| Anel do Uivo da Matilha / Rugido Esmagador / Uivo da Tempestade | miolo seguro 2,5 / 4,0 / 3,0 | 0,8 / 1,0 / 1,0 | quem batia colado ficava sempre no miolo e a katana vencia parada |
| Patada Larga / Investida Trovejante / Uivo da Tempestade | mult 2,5 / 2,6 / 2,2 | 3,0 / 3,0 / 2,6 | katana parada ainda vencia Urso e Lobo da Tempestade |
| Carga (fase 0) | Pisao 1,8 · Garras 0,8 · Salto 1,1 · Rodopio 1,3 · Rajada 1,2 · Patada Dupla 0,9 | 2,0 · 0,95 · 1,35 · 1,7 · 1,25 · 1,0 | inesquivaveis na fase 2 (a carga encurta 15%) |
| Queda Estrondosa / Nova de Gelo | raio 7,0, carga 2,0 / 1,5 | raio 6,2, carga 2,0 / 2,0 | inesquivaveis; carga fica dentro de 0,8–2,0 s |

**Antes × depois** (no nivel do chefe; tempo e HP minimo de ESQUIVA + pocao
entre os 4 conjuntos; quem venceu PARADO + pocao com o HP minimo):

| Chefe | Nv | Antes: luta | Antes: HP min | Antes: quem perde esquivando | Antes: vence parado | Depois: luta | Depois: HP min | Depois: vence parado |
|---|---:|---|---:|---|---|---|---:|---|
| Lobo Alfa da Clareira | 8 | 30–49 s | 19% | — | Espada (97%), Katana (50%) | 65–110 s | 38% | ninguem |
| Capitao Barba-Tormenta | 11 | 43–72 s | 30% | — | Espada (95%), Katana (10%) | 81–134 s | 48% | ninguem |
| Urso Anciao | 14 | 45–84 s | 36% | — | Espada (96%), Katana (47%) | 71–141 s | 50% | ninguem |
| Tigre das Neves | 24 | 12–99 s | 0% | Katana, Pistolas, Anel | Espada (86%) | 74–175 s | 50% | ninguem |
| Lobo da Tempestade | 30 | 57–143 s | 53% | — | Espada (97%), Katana (73%) | 61–148 s | 83% | ninguem |
| Saqueador das Areias | 36 | 35–120 s | 0% | Pistolas, Anel | Espada (96%), Katana (15%) | 69–178 s | 63% | ninguem |
| Arqueira do Ermo | 41 | 88–254 s | 18% | — | Espada (97%), Katana (67%) | 85–217 s | 84% | Katana (6%) |
| Owlbear Primevo | 52 | 93–314 s | 50% | — | Espada (95%), Katana (55%) | 77–232 s | 92% | ninguem |
| Arquimago da Tormenta | 60 | 126–357 s | 50% | — | Espada (96%), Katana (89%) | 92–230 s | 94% | Katana (7%) |

Antes, 10 golpes eram inesquivaveis na fase 2 (ou ja' na fase 0); depois,
nenhum. Esquivando sem pocao perde so' Pistolas no Lobo Alfa e no Capitao.
"Vence parado" com HP minimo ≤ 10% passa na meta: sobreviveu por pouco.

## Pendencias

- A simulacao nao faz o strafe de espera do chefe nem o empurrao do golpe, e o
  tiro do chefe de gente sempre acerta.
- Arte de corpo inteiro so' existe pro lobo (Lobo Alfa e Lobo da Tempestade
  usam o mesmo modelo); urso, tigre, owlbear e os de gente usam o preset do mob
  comum escalado. Sem brilho nos olhos.
