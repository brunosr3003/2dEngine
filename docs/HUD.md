# HUD e Menu: igual ao MIR4, adaptado ao PC

> Pedido do usuário: **"tudo tem que ser via menu, tem que ver como funciona a HUD
> do MIR4 e faz igual"**.
>
> Este doc tem três partes: (1) como o MIR4 organiza a tela, pesquisado;
> (2) o HUD do Tempest nesse molde, com posição, tamanho e tecla de cada coisa;
> (3) o Menu Principal, o padrão de painel e a ordem de implementação.
> ⚠️ marca o que a pesquisa não confirmou.

---

## 1. O HUD do MIR4 (pesquisado)

O MIR4 de PC (Steam/launcher) usa **a mesma interface do celular**. Na Steam
a reclamação mais comum é justamente que "parece jogo de celular em emulador";
um jogador responde "é um emulador mesmo, aperte Alt e você vê todos os
atalhos". Então descrever o celular é descrever o PC. A única coisa a mais no
PC é a camada de teclado: **segurar Alt mostra a tecla de cada ícone**.

### 1.1 As zonas da tela

O guia oficial ("Default Screen") numera os elementos em três imagens: faixa de
cima, lado direito e parte de baixo. Juntando essas imagens com os guias de
terceiros:

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ①LV/PODER ②modo PK         ④✉ ⑤evento ⑥vigor ⑦clã ⑧debuff ⑨grupo    │ ①Clã ②Missão ③Equip ④Aviso ⑤MENU │
│ ③HP▬▬▬▬▬▬ MP▬▬▬            ⑩ver grupo ⑪deck ⑫chat ⑭emote ⑭câmera ⑮montaria│ ⑥propr. da área ⑦NOME DA ÁREA ⑧canal│
│   vigor (botão)                                                          │ [minimapa ⚠️]                     │
│ ┌ lista de missões ⚠️┐                                                    │                                   │
│ │ principal  ▸ auto  │                   (mundo)                          │                                   │
│ │ pedidos    ▸ auto  │                                                    │                                   │
│ └────────────────────┘                                                    │               ⑩pular  ⑨esquiva    │
│ chat (2–3 linhas)                                                         │        ⑤skill ⑤skill  ⑧golpe letal│
│  ╭──╮                        ①auto coleta ②AUTO  ③poção ③poção ④⚙       │   ⑤skill     ⑥alvo  ⑦ATAQUE      │
│  │◎ │ joystick (celular)                                                 │                     (botão grande)│
│  ╰──╯                                                                     │                                   │
│▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬ EXP ▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬│
└──────────────────────────────────────────────────────────────────────────────┘
```

| Zona | O que tem (numeração do guia oficial) |
|---|---|
| **Canto superior esquerdo** | ① nível e **Poder** · ② modo de alinhamento (Pacífico/Legal/Hostil/Carnal) e "atacar só usuários" · ③ HP/MP · botão de Vigor logo abaixo da vida |
| **Faixa de cima, ícones pequenos** | ④ correio · ⑤ eventos (login, especiais, conquistas, pergaminhos de missão) · ⑥ vigor · ⑦ pedido de ajuda do clã · ⑧ remover debuff/penalidade de morte · ⑨ criar grupo (público/privado/clã) · ⑩ mostrar nome e HP do grupo · ⑪ trocar deck (espírito/pedra mágica) · ⑫ chat (Todos/Clã/Grupo/Sussurro) · emotes · câmera (perto/longe/manual) · ⑮ montaria |
| **Canto superior direito** | ① Clã · ② Missão · ③ Equipamento (bolsa) · ④ Notificações · ⑤ **MENU** · ⑥ propriedades da área (PvP etc.) · ⑦ **nome da área (tocar abre o mapa)** · ⑧ canal · ⑨ lista de missões com auto-play |
| **Baixo, centro/esquerda do cluster** | ① **auto coleta** · ② **auto combate** (alvo mais perto dentro do alcance das Configurações de Combate) · ③ poções rápidas · ④ engrenagem (uso automático de poção por % de HP e onde fica o vendedor de poção) |
| **Canto inferior direito (arco)** | ⑤ skills equipadas · ⑥ trocar alvo · ⑦ **ataque básico (o botão grande)** · ⑧ golpe letal (enche com ataques e skills) · ⑨ esquiva (10 s) · ⑩ passo aéreo/pulo duplo |
| **Canto inferior esquerdo** | joystick virtual (celular); no PC é WASD |
| **Rodapé** | barra de EXP na largura toda |

⚠️ **Posição do minimapa e da lista de missões.** Um guia de terceiros diz
"o minimapa fica à esquerda da tela principal", e outro, que a lista de missões
fica "do lado esquerdo". O guia oficial, porém, põe a lista de missões (⑨) e o
nome da área (⑦) na imagem do lado direito, e o LevelWinner diz "o objetivo da
missão principal fica à direita". Pela lembrança do jogo e pelo pedido do
usuário, a lista de missões fica à **esquerda**, abaixo do retrato. As fontes
não fecham isso. **Para o Tempest a posição foi decidida (seção 2) e não
depende disso.**

### 1.2 O Menu do MIR4

- **Botão de Menu no canto superior direito** (≡, que alguns guias chamam de
  "+"). A tela do menu mostra as moedas do jogador à esquerda (Cobre, Darksteel,
  Energia, Dragon Steel…). **As moedas não ficam no HUD do mundo, ficam no
  menu.**
- Os ícones são agrupados, e "os ícones de cima são os de fortalecer o
  personagem". Alguns grupos abrem sub-itens: *Personagem* leva a equipamento,
  pedra mágica, visual e montaria.
- Caminhos citados literalmente pelos guias (cada um é um grupo e uma aba):
  - **Train → Constitution / Inner Force.** Dentro da janela, as abas ficam
    **no alto à esquerda**: "clicando na aba Constitution no alto à esquerda
    da janela de Inner Force".
  - **Craft → Crafting / Combine / Unseal / Exchange.** "O Crafting Menu é uma
    interface para craftar sem a ajuda de um mercador." **O craft é remoto, pelo
    menu.**
  - **Forge → Enhance** (e Enchant). Também se chega pelo botão *Enhance* dentro
    do craft de equipamento. **Refinar também é remoto.**
  - **Portal** leva à Magic Square / Secret Peak (conteúdo de andares, com
    ticket diário).
  - **System → Game Settings (Convenience: Save Power, tamanho da UI, minimapa
    liga/desliga) / Environment (gráficos, som) / PC System (atalhos) / Account.**
- Outros itens do menu confirmados em guias: Skill, Spirit, Mystique, Codex,
  Achievements, Clan, Friends, Party, Mail, Shop, Market, Ranking. ⚠️ Não achei
  a grade exata nem a ordem dos ícones.
- A **loja de NPC não é remota**. O botão de poção mostra **onde fica o
  vendedor**, e o jogo leva o jogador até lá pelo auto-path.

### 1.3 Painéis

- Os sistemas abrem em **tela cheia**. O HUD some e o mundo fica escuro atrás.
  As **abas ficam no alto à esquerda** e o **X/voltar no alto à direita**.
  ⚠️ Com abas *laterais* (verticais) só lembro de alguns sistemas; o que os
  guias citam é "aba no alto à esquerda".
- Os painéis vizinhos se ligam por aba: da janela de Inner Force se vai para a
  de Constitution, da avaliação de opções vai-se para Enhance.

### 1.4 Automação, economia de energia e teclado

- **Auto-play de missão:** tocar na missão da lista liga o auto-path e a
  execução. Na janela de missões existe "Auto-Play", que encadeia até 10.
- **Save Power** fica em System → Game Settings → Convenience. ⚠️ Os guias só
  confirmam que a opção existe. De lembrança: a tela escurece e mostra um
  resumo (tempo, EXP e itens obtidos) enquanto o auto roda, e para sair é
  preciso arrastar/tocar.
- **Tamanho da UI:** Settings → Convenience (reduzir).
- **Teclado no PC** (padrão; guias oficiais "Hotkeys (PC)" e "Default
  Attack/Skill"):
  - WASD/setas movem. **Qualquer movimento manual (tecla ou direcional na tela)
    desliga as atividades automáticas.**
  - **F** ataque básico (o botão grande).
  - **1–6** skills. A numeração começa pela direita: da esquerda para a direita
    ficam 6, 5, 4, 3, 2, 1, ou seja, o 1 é o mais perto do ataque.
  - **R** golpe letal.
  - **Tab** alvo: troca entre inimigos perto (ou abre a lista de alvos,
    conforme o "Target Method" nas configurações).
  - **C** poção; **8, 9, 0** slots rápidos.
  - **Shift** esquiva; **Espaço** passo aéreo (de novo: pulo duplo); **G** Air
    Stride.
  - **Alt mostra a tecla de cada ícone**; **Alt+Enter** alterna tela cheia.
  - Tudo é reconfigurável em Menu → System → PC System → Hotkey (movimento,
    combate, slots rápidos, "menu padrão" e "menu de ação").
  - Auto skill por skill: **arrastar para cima ou para baixo** sobre o slot
    (igual ao Tempest).
  - ⚠️ A tecla padrão dos botões AUTO (combate e coleta) não aparece em nenhum
    guia. Alt mostra tecla "para cada ícone", mas não achei qual é.
  - ⚠️ Câmera, montaria e interação com NPC: não achei as teclas padrão.

### 1.5 Princípios que tiramos daí

1. **Fica sempre visível só o que o loop de jogo consome a cada segundo:**
   vida/mana, alvo, skills, botões de AUTO, onde estou (área, canal, minimapa)
   e o próximo objetivo (missões). EXP no rodapé.
2. **Todo o resto mora no Menu.** Moedas, craft, forja, conquistas, social e
   configurações. O HUD tem **um** botão de Menu, mais 3 ou 4 atalhos de uso
   diário ao lado dele.
3. **Craft e refino funcionam de qualquer lugar pelo menu.** A loja de NPC não:
   o menu mostra onde fica e leva até lá.
4. **Tocar leva lá.** Missão, nome da área e vendedor de poção são todos
   clicáveis e ligam o auto-path.
5. **Aviso é um ponto vermelho no ícone**, que sobe até o botão de Menu. Não
   se usa janela pulando na cara.
6. **Um painel por vez, em tela cheia, com abas e X.** Voltar é Esc ou X.
7. **O cluster de combate fica no canto inferior direito, em arco.** O botão
   grande fica no canto, as skills em volta e os AUTO à esquerda dele.
8. **Regra do Tempest (decisão do usuário): nada abre por tecla.** No MIR4 a
   interface é de toque e o teclado do PC só imita os toques. O usuário quer
   **só a HUD**: todo painel abre por ícone do HUD ou pelo Menu Principal. Esc
   só **fecha**. Ver seção 2.5.

---

## 2. O HUD do Tempest

### 2.1 Escala e âncoras

Os números abaixo valem para uma tela **1920×1080**. O código atual usa px
fixos que foram pensados para ~1280 (os painéis da direita têm 194 px).

```
escala = clamp(min(sw/1920, sh/1080), 0.70, 1.30) × escala_ui   // escala_ui: 80–160%, Menu → Sistema → Interface
```

**Área segura (iPhone).** `nativo::area_segura()` lê o `safeAreaInsets` da
janela (notch/Dynamic Island, cantos arredondados, barra do home), soma 4 pt de
respiro com mínimo de 16 pt por lado e converte para px. `hud_layout::acompanhar()`
relê a cada 30 quadros (girar o aparelho troca o lado do notch). O HUD inteiro é
montado **dentro** dessa área (`zonas_com(sw, sh, margens, escala_ui)`): `sw`,
`sh` acima são os da área segura. Painéis ancorados em canto (Menu, janela de
missões, loja do NPC, Interface) usam `hud_layout::tela_segura()`. Fora do iOS
as margens são zero.

**Escala da interface.** `escala_ui` vai de 80% a 160% em passos de 10%.
Padrão: **130% no celular**, 100% no PC. Vale para o HUD (`s` acima) e para
**todo texto** (`hud_estilo::fator_texto`, aplicado em `desenha_texto` e nas
`medir*` — os chamadores continuam passando o tamanho "de 100%"). Se a escala
pedida não couber, `zonas_com` desce de 2 em 2% até caber (nada sobreposto,
tudo na área segura, joystick com espaço para o polegar), sem descer abaixo da
escala da tela × min(escala_ui, 1). O número de missões no rastreador é
decidido pela altura "de 1080" (`sh / escala`). Fica salvo em
`Preferencias.escala_ui` (servidor recorta para 0,8–1,6). Testes: todas as telas
de PC e 5 aparelhos (iPhone 15 Pro/Pro Max, SE, 11, iPad Air) a 80/100/130/160%.
Limite conhecido: painéis grandes de tamanho fixo (Bolsa, Craft, Forja) não
crescem com a escala, só o texto dentro deles.

Cada elemento tem uma **âncora** (canto ou borda), uma margem de 20 px (×
escala) e um tamanho. **Uma função só** (`hud_layout::zonas(sw, sh, escala)`)
devolve os `Rect` de tudo. Desenho, `pega_mouse` e o teste de sobreposição
chamam essa mesma função. É a regra do README ("uma verdade por pergunta"):
hoje cada módulo calcula o próprio `Rect`, e por isso já existem colisões
(seção 4.1).

### 2.2 Layout 16:9 (1920×1080)

```
 0                                                                                             1920
┌────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ╭───╮ Brunji                   ┌──────── ALVO ────────┐        [🎒][📜][👥][🔔]  [ ≡ MENU ]    │ 20
│ │LV │ HP ▬▬▬▬▬▬▬▬▬ 812/900     │ Lobo Cinzento   Lv 7 │                                         │
│ │ 12│ MP ▬▬▬▬▬▬  140/200       │ ▬▬▬▬▬▬▬▬▬▬▬ 88/120   │        ┌──── Bosque ─────── ⚔ PvE ┐     │
│ ╰───╯ vigor ▬▬▬                └──────────────────────┘        │ SA01 · CH 2 · ● 42 ms    │     │
│ PODER 12.345      [buff][buff][buff]                           ├──────────────────────────┤     │
│ ┌[Missões][Grupo]─────────────┐                                │                          │     │
│ │● Conheça o Alquimista  ▸AUTO│                                │        MINIMAPA          │     │
│ │  Fale com o Alquimista      │                                │        240 × 240         │     │
│ │○ Pedras para a Forja        │                                │                          │     │
│ │  Colete pedra  3/6          │                                │                          │     │
│ │○ ...                        │                                └──────── 412, 388 ────────┘     │
│ │        Todas as missões (L) │                                                                 │
│ └─────────────────────────────┘                                                                 │
│                                                                                                 │
│                                          (mundo)                                                │
│                                                                             [toast: Raro!]      │
│                               ┌ AUTO COMBATE · ATACANDO ┐                        ③              │
│                               └─────────────────────────┘                  ②         ╭─────╮    │
│ ┌ MUNDO │ GRUPO │ SISTEMA ────────────┐                                              │ATACAR│   │
│ │ Lobo cinzento: -24                  │ [COLETA X][COMBATE Z][C 🧪][8][9][0]     ①   │ F ⚔ │    │
│ │ +57 Cobre                           │                                             ╰─────╯    │
│ └─────────────────────────────────────┘                                                         │
│▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬ EXP 42,17% ▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬│ 1080
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 2.3 Elemento por elemento

Tamanhos em px a 1920×1080, **antes** da escala.

| # | Elemento | Âncora / posição | Tamanho | Estados | Tecla |
|---|---|---|---|---|---|
| A | **Ficha** (círculo de nível, nome, HP, MP, vigor) | sup. esq., (20, 20) | 440×130 | HP < 30% pulsa (já existe); morto: "Incapacitado" | — |
| A2 | **Poder** | dentro da ficha, abaixo do círculo | — | sobe/desce com animação de número por 1 s | — |
| A3 | **Buffs/debuffs** | à direita do Poder, dentro da ficha | ícones de 28, até 8 | hover: nome e tempo | — |
| B | **Rastreador** com abas `Missões` / `Grupo` | sup. esq., abaixo da ficha, (20, 164) | 440 × (48 + 56 por missão), até 4 missões (5 em 16:10) | missão em auto: faixa verde à esquerda + "▸AUTO"; pronta: verde "Volte ao Mestre"; clique numa missão: auto missão (já existe); **clique no título "Missões ▸"**: painel Missões (aba Em andamento); **link "Todas as missões"** no rodapé: aba Todas; fila de dungeon aparece como 1ª linha ("PROCURANDO GRUPO · 01:23") | nenhuma (substitui J e L) |
| B2 | **Aba Grupo** (party frames) | no lugar da lista de missões | 1 linha de 56 por membro (nome, papel, HP, MP) | fora de alcance: esmaecido; clique no título: painel Grupo | nenhuma |
| C | **Alvo** | sup. centro, y = 20 | 460×80 | chefe: laranja "CHEFE"; clique no nome centraliza a câmera ⚠️ proposta; **X pequeno limpa o alvo** | **Tab** troca para o próximo inimigo perto (MIR4); Esc limpa |
| D | **Ícones do topo** (Bolsa, Missões, Grupo, Avisos) | sup. dir., à esquerda do Menu | 4 × 48×48, vão de 8 | ponto vermelho quando há novidade; tooltip com o nome | nenhuma (substitui I e J) |
| E | **Botão MENU** ≡ | sup. dir., (1900−64, 20) | 64×48 com o rótulo "MENU" | ponto vermelho se algum item do menu tem aviso | nenhuma: **só clique** |
| F | **Área e canal** | sup. dir., abaixo de D/E, (1900−320, 80) | 320×56 | **clique no nome da zona: painel Mapa**; `realm · CH n` (clique = trocar canal, em breve); ponto de latência (verde/âmbar/vermelho, já existe); selo do tipo de área (PvE/PvP) | nenhuma (substitui M) |
| G | **Minimapa** (disco) | sup. dir., colado embaixo de F | dois tamanhos: 320×320 e, expandido, até 520×520 — o maior é limitado pelo espaço até o arco de skills, para crescer o minimapa **sem** encolher o resto do HUD | **sem painel nenhum**: o disco flutua sobre o mundo, com halo curto e anel dourado. O mapa é recortado por MALHA (leque de triângulos com UV própria, `disco_do_mapa`) — não por máscara pintada nos cantos, que era o que obrigava a existir uma caixa opaca. Bússola **N/L/O fora do anel**; o sul não tem letra porque é ali que ficam as **coordenadas**, centradas e encostadas na base do anel. `MARGEM_DO_DISCO` é a folga entre o disco e o retângulo do layout, e é onde bússola e coordenadas moram — seu valor é medido, não escolhido (a pílula tem 18 px de altura). Roda = zoom; clique = viajar, **só dentro do disco**; **⤢ no alto: painel Mapa**; **botão de tamanho ao lado dele**, com a escolha guardada em `Preferencias::minimapa_expandido` | nenhuma (substitui M) |
| H | **Chat** com abas Mundo / Grupo / Sistema | esq., logo abaixo do rastreador (mudou: o canto de baixo é do joystick) | 440×150 | Enter abre o campo (em breve); passar o mouse destaca | Enter |
| H2 | **Joystick virtual** (só toque) | área de início: esq., do fim do chat até a EXP, até antes da faixa central e do AUTO COLETA (`hud_layout::Zonas::joystick`) | raio 78 px × escala; flutuante (base onde o dedo tocou) | aparece ao encostar, some ao soltar; vidro escuro, anel e manípulo com brilho AUTO | — |
| I | **Faixa de estado** (uma só) | centro, y = 1080−300 | até 520×34 | prioridade: aviso de skill > INDO · destino · m > AUTO MISSÃO > AUTO COMBATE/COLETA. Hoje são 3 faixas em alturas diferentes que se sobrepõem | — |
| J | **Botão ATACAR** (grande) | inf. dir., centro em (1920−110, 1080−120) | círculo de raio 60 | sem alvo: "ALVO" (escolhe o inimigo mais perto e ataca); com alvo: "ATACAR" | **F** (MIR4) |
| K | **Skills 1/2/3** em arco | em volta de J, a 150 px do centro. **O 1 é o mais perto do ATACAR** (MIR4 numera a partir do ataque): 1 a 180°, 2 a 225°, 3 a 270° | círculo de raio 42 | recarga em setor (já existe); AUTO no aro; bloqueada "Lv N"; arrastar ↑AUTO / ↓manual (já existe, igual ao MIR4; a dica vai para o tooltip) | 1 · 2 · 3 |
| K2 | **Slot 4 = PULO** | 315° no mesmo arco | raio 42 | era um disco cinza sem função. O celular não tem tecla de espaço, e este é o lugar que o polegar já procura: virou o botão de PULO, com ícone próprio no atlas | 4 · Espaço |
| L | **AUTO COMBATE** | à esquerda do arco, na linha de baixo, (1920−420, 1080−110) | 88×88 | anel verde girando quando ligado (já existe) | Z |
| M | **AUTO COLETA** | à esquerda de L | 88×88 | idem | X |
| N | **Poção + slots rápidos** | entre L e o arco | poção 64×64 + 3 slots de 56×56 | quantidade; ⚙ abre Configurações → Combate (uso automático por % de HP); sem poção: vermelho e "Ir ao vendedor". O AUTO COMBATE usa poção e slots sozinho (MIR4) | **C** poção · **8 · 9 · 0** slots (MIR4) |
| O | **Ganhos** (+57 Cobre) | no mundo, sobre a cabeça (já existe) | — | continua igual | — |
| O2 | **Toast de loot raro** (peça, grau Raro+, baú) | dir., acima do arco, (1920−380, 1080−420) | 360×56, até 3 empilhados, 4 s | cor do grau | — |
| P | **EXP** | rodapé, largura toda | altura 6 (hoje 4) | 10 marcas (já existe) | — |
| Q | **Diagnóstico** | abaixo do rastreador, só enquanto F3 estiver apertado | 430×55 | já existe | F3 |
| R | **Teclas sobre os botões de ação** | botões J, K, L, M, N e o alvo C | chip de 18 px no canto | **visíveis enquanto Alt estiver apertado** (MIR4). Ícones de painel **não** ganham chip, porque não têm tecla. Os rótulos fixos "Z ligar/parar", "X coleta" e "Bolsa [I]" saem da tela | Alt |

**Somem do HUD:** o botão "Sair" do painel de canal (vai para Menu → Sair), o
botão "Bolsa [I]" do canto inferior direito (vira ícone em D), o texto
"I Inventário" da ficha, a dica "Missões · J · clique: ir" do rastreador, a
dica "L fecha" do menu de missões e a dica fixa "Arraste ↑ AUTO / ↓ manual".
Nenhum texto do HUD ou dos painéis cita tecla de abrir.

### 2.5 Teclado: nada abre por tecla

**Decisão do usuário: "não quero nada abrindo por botão do teclado, somente
pela HUD igual no MIR4".** Todo painel abre por ícone do HUD ou pelo Menu
Principal. O teclado **não abre** nada.

**Teclas a REMOVER** (hoje em `main.rs`, `bolsa.rs`, `menu_missoes.rs`):

| Tecla hoje | Abria | Substituída por (HUD) |
|---|---|---|
| **I** | Bolsa | ícone 🎒 **Bolsa** no topo direito (D) · Menu → Personagem → Bolsa |
| **M** | Mapa grande | **clique no nome da zona** (F) · ícone ⤢ do **minimapa** (G) · Menu → Aventura → Mapa |
| **J** | Diário de missões | **título "Missões ▸" do rastreador** (B) · ícone 📜 **Missões** (D) · Menu → Progresso → Missões |
| **L** | Todas as missões | link **"Todas as missões"** no rodapé do rastreador (B) · aba *Todas* do painel Missões · Menu → Missões |
| (proposta antiga Tab/Esc) | Menu Principal | **só o botão ≡ MENU** (E) |
| (proposta antiga C, K, O, U, P, H) | Ficha, Habilidades, Craft, Forja, Grupo, Aventuras | **só os itens do Menu** (3.3); Grupo também pelo ícone 👥 (D) e pelo título da aba Grupo (B2) |
| **nenhuma nova** | Craft / Forja (outro agente) | **não criar tecla provisória**. Até o `menu.rs` existir, o acesso provisório é um **ícone temporário no HUD** (ao lado dos ícones D), removido quando o Menu chegar |

**Esc só fecha.** Na ordem: fecha a janela leve ou o painel do topo da pilha
(do painel aberto pelo Menu, volta ao Menu; do Menu, volta ao jogo). Se não há
nada aberto, **cancela** o que está em andamento (alvo, AUTO, viagem, ir-para),
como já faz hoje. **Nunca abre nada.** Os painéis também fecham pelo X e voltam
pela ←.

**Teclas de AÇÃO: DECIDIDO pelo usuário, "igual no MIR4"** (o MIR4 de PC,
seção 1.4). O que o MIR4 de PC faz por tecla fica por tecla. O que ele só faz
por botão na tela fica só por botão. Toda ação também tem **botão no HUD**,
porque no MIR4 a tecla é só um atalho do ícone.

| Ação | Tecla hoje | MIR4 PC | **Tempest (decidido)** | Nota |
|---|---|---|---|---|
| Andar | WASD / setas; clique no chão | WASD / setas | **WASD / setas** | O clique no chão/NPC/inimigo é **mouse, não tecla**, e continua: é o equivalente do direcional de toque num jogo de câmera alta com clique. ⚠️ O MIR4 não tem clicar-para-andar |
| Ataque básico | (clique no inimigo) | **F** + botão grande | **F** + botão ATACAR (J) | **Conflito:** F hoje aproxima a câmera, e o zoom vai para a **roda do mouse** |
| Skills | 1 · 2 · 3 | **1–6**, o 1 perto do ataque | **1 · 2 · 3** (4 reservado), o 1 perto do ATACAR | Auto/manual por arrasto ↑/↓ (já é igual) |
| Golpe letal | — | **R** | **R reservado** (não existe no Tempest) | **Conflito:** R hoje afasta a câmera, e o zoom vai para a roda |
| Alvo | clique | **Tab** (próximo perto / lista) + botão ⑥ | **Tab** = próximo inimigo perto; clique continua | A *lista* de alvos, se existir, abre só pelo botão (é painel) |
| Poção | — | **C** + botão | **C** + botão (N) | a fazer |
| Slots rápidos | — | **8 · 9 · 0** + botões | **8 · 9 · 0** + botões (N) | a fazer |
| AUTO COMBATE | Z | botão ② (tem tecla, Alt mostra; ⚠️ padrão não documentado) | **Z** + botão L | Mantém Z, que já existe e não conflita |
| AUTO COLETA | X | botão ① (idem) | **X** + botão M | idem |
| Esquiva | — | **Shift** | reservado para quando existir | **Conflito:** Shift hoje é correr. Fica correr até existir esquiva, e aí Shift vira esquiva e a corrida fica só a automática |
| Pular | Espaço | **Espaço** (passo aéreo) | **Espaço** | igual |
| Câmera girar | Q / E | ⚠️ não documentado | **Q / E** (mantido) | não abre nada |
| Câmera zoom | R / F | ⚠️ | **roda do mouse** | libera R e F para o MIR4 |
| Mostrar teclas | — | **Alt** | **Alt** mostra os chips (R) | igual |
| Tela cheia | — | **Alt+Enter** | **Alt+Enter** | não abre painel |
| Diagnóstico | F3 (segurar) | — | **F3** (mantido) | ferramenta de desenvolvimento, não é painel nem existe no MIR4 |
| Falar com NPC | clique | toque no NPC | **clique** | sem tecla |
| Andar cancela AUTO | não (a área acompanha) | **sim**, qualquer movimento manual desliga | ⚠️ **regra de jogo, não de tecla**: fica como está até o usuário pedir | anotado |

**Conflitos com o MIR4 resolvidos pela regra do usuário:** o MIR4 de PC tem
atalhos de **"menu padrão" e "menu de ação"** (abrir sistemas por tecla). No
Tempest **não existem**: nenhum painel abre por tecla, e isso vale acima do
"igual MIR4". O rebind fica em Menu → Configurações → Atalhos, e só para as
teclas de ação desta tabela.

### 2.4 Em 16:10 (1920×1200, a cadeia do Moonlight) e em janela estreita

| Tela | Comportamento |
|---|---|
| **16:10, 1920×1200** | A escala continua 1,0 (limitada por `sw/1920`). Os 120 px a mais ficam no meio, entre o topo e o cluster de baixo: o rastreador mostra **5 missões** e o chat, 8 linhas. Nada troca de canto. |
| **1600×900 / 1366×768** | Escala 0,83 / 0,71. Mesmo desenho. |
| **Largura < 1280 (janela lado a lado)** | Escala mínima de 0,70. O **alvo desce** para baixo da faixa de cima (o código já faz isso abaixo de 1120). Os atalhos D se recolhem: sobra só **MENU**, e Bolsa/Missões ficam dentro dele. O minimapa cai para 200. O rastreador mostra 2 missões e um "+N". O chat encolhe para 320×110. |
| **Altura < 720** | O rastreador fica recolhido (só o título e a missão em auto) e o chat fica com 3 linhas. |
| **Teste** | `hud_layout::zonas` sem sobreposição em 1920×1080, 1920×1200, 1600×900, 1280×720, 1024×768 e 960×1080 (meia tela). Teste puro, sem janela. |

---

## 3. Menu Principal

### 3.1 Como abre

- **Só pelo botão ≡ MENU** no canto superior direito (E). Nenhuma tecla abre
  o Menu (2.5).
- **Fecha** pelo X, por clicar de novo no ≡ ou pelo **Esc** (que só fecha).
- Menu aberto: o mundo continua rodando (é MMO), fundo escurecido a 60%, e o
  AUTO segue ligado.

### 3.2 A tela do Menu

```
┌─────────────────────────────────────────────────────────────────────────────────────────────┐
│ MENU                                         ● Ouro 12.480   ◆ Darksteel 3.200   ✦ TP 0   [X]│
├──────────────────┬──────────────────────────────────────────────────────────────────────────┤
│  ╭──────────╮    │ PERSONAGEM                                                               │
│  │ retrato  │    │  [Ficha•]  [Bolsa]  [Habilidades•]  [Montaria🔒]                         │
│  │  voxel   │    │ PROGRESSO                                                                │
│  ╰──────────╯    │  [Missões•]  [Conquistas🔒]                                              │
│  Brunji   Lv 12  │ OFICINA                                                                  │
│  PODER 12.345    │  [Craft]  [Forja]  [Encantar🔒]                                          │
│  Katana          │ AVENTURA                                                                 │
│                  │  [Mapa]  [Dungeons e Raids  ]                                            │
│  Cobre    1.240  │ SOCIAL                                                                   │
│  Madeira    380  │  [Grupo]  [Amigos🔒]  [Correio🔒]  [Clã🔒]                               │
│  (materiais)     │ COMÉRCIO                                                                 │
│                  │  [Loja]  [Mercado🔒]                                                     │
│                  │ SISTEMA                                                                  │
│                  │  [Configurações]  [Trocar personagem]  [Sair]                            │
└──────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

- Painel de até 1600×900, centralizado. A **coluna da esquerda** (300) tem o
  retrato (o mesmo da bolsa), o nível, o Poder, o conjunto de arma e os
  **saldos** (é aqui que as moedas aparecem, como no MIR4). À direita fica a
  **grade por grupos**, com ícones de 96×96 e o rótulo embaixo.
- **Selos:** ● vermelho = há algo a fazer; faixa **NOVO** = sistema liberado
  agora, some ao abrir uma vez; 🔒 + tooltip = "Nível N" ou "Em breve"
  (clicável, só mostra o aviso).
- Os ícones são pictogramas vetoriais próprios, no mesmo esquema de
  `hud_estilo::icone`: nenhuma arte de outro jogo.

### 3.3 Itens

A coluna **"Também no HUD"** lista o outro caminho por clique além do Menu.
**Nenhum item tem tecla** (2.5).

| Grupo | Item | Ícone sugerido | Abre | NPC? | Selo ● quando | Também no HUD | Estado |
|---|---|---|---|---|---|---|---|
| Personagem | **Ficha** | busto | painel com XP, poder, status de combate e os seis atributos (`AllocStatPoint`/`ResetStats`) | não | há pontos livres | clique no círculo de nível da ficha (A) | feito |
| Personagem | **Bolsa** | mochila | painel **Personagem**, aba Bolsa (a bolsa atual: equipamento + grade) | não | peça melhor que a vestida entrou; bolsa cheia | ícone 🎒 (D) | existe, migrar |
| Personagem | **Habilidades** | três lâminas | painel **Personagem**, aba Habilidades (as 12 skills, liberação por nível, AUTO/manual por skill, prévia de `previa_skills.rs`) | não | skill liberada no nível | clique direito ou ⓘ numa skill do arco (K) ⚠️ proposta | a fazer (a barra já existe) |
| Personagem | Montaria/Barco | âncora | aba Montaria | — | — | — | em breve |
| Progresso | **Missões** | pergaminho | painel **Missões**: abas *Em andamento* (o diário de hoje) · *Todas* (o menu L de hoje) · *Semana* (primeiras vitórias, DUNGEONS) | não para ver; **aceitar e entregar continuam no NPC** (o "Ir" leva até lá) | missão pronta pra entregar; missão nova disponível | ícone 📜 (D); título e link "Todas" do rastreador (B) | existe, fundir |
| Progresso | Conquistas | troféu | painel Conquistas | — | — | — | em breve |
| Oficina | **Craft** | martelo e bigorna | painel **Oficina**: abas por estação *Forja · Ateliê · Fundição · Carpintaria* (`shared::craft_station`), com receitas, materiais (tenho/preciso), "de onde vem" no hover e botão Craftar (`Craft { recipe_id }`) | **não, de qualquer lugar** | receita nova liberada (**não** "tem material": vira ruído) | só Menu (ícone temporário no HUD até o Menu existir) | **outro agente fazendo agora** |
| Oficina | **Forja** | chama sobre lâmina | painel **Forja**: abas *Refino* (+1…+12, chance, custo, faixa segura até +5; `RefineItem`) · *Combinar* (2 iguais → próximo tier) · *Encantar* 🔒 | **não, de qualquer lugar** | — | só Menu (ícone temporário no HUD até o Menu existir); botão "Refinar" no cartão da peça na Bolsa | **outro agente fazendo agora** |
| Oficina | Encantar | runa | painel Forja, aba Encantar | — | — | — | em breve |
| Aventura | **Mapa** | bússola | painel **Mapa** (o mapa grande de hoje, com filtros e "Ir para") | não | — | nome da zona (F); ⤢ do minimapa (G) | existe, migrar |
| Aventura | Dungeons e Raids | portal | painel **Aventuras** (DUNGEONS §9 "Telas no cliente": abas Porão / Gruta / Caçada / Chefes) | não para entrar na fila — e **não há entrada física no mundo**: a janela é o único caminho (decisão do dono) | entrada nova liberada; primeira vitória da semana disponível | faixa de fila no rastreador (B) | **existe** (F1/F2: Porão e Gruta; Caçada em breve) |
| Social | **Grupo** | três cabeças | painel Grupo: membros, convidar por nome (`PartyInvite` existe), sair | não | convite recebido | ícone 👥 (D); título da aba Grupo (B2) | a fazer (cliente) |
| Social | Amigos · Correio · Clã | aperto de mão · envelope · estandarte | painéis próprios | — | carta nova (Correio: é por onde chega o baú de raid) | — | em breve |
| Comércio | **Loja** | balança | **painel Lojas da ilha**: lista de vendedores (nome, o que vende, distância) com **"Ir"** (auto-path `ir_para::Objetivo::Npc`, e ao chegar abre a loja). Ver 3.4 | **sim** | — | — | a fazer |
| Comércio | Mercado | sacola | leilão entre jogadores (ECONOMIA) | — | — | — | em breve |
| Comércio | Loja TP | gema | loja de cash, **remota** (ECONOMIA §TP) | não | — | — | em breve |
| Sistema | **Interface** (hoje) | engrenagem | tamanho do HUD e dos textos, 80–160% (padrão 130% no celular), muda na hora e salva no personagem — `config_interface.rs` | não | — | — | **existe** |
| Sistema | **Configurações** (futuro) | engrenagem | abas *Jogo* (mostrar minimapa, **Economia de energia**) · *Combate* (raio do AUTO, poção automática por %) · *Gráficos/Som* · *Atalhos* (lista, e no futuro rebind) | não | — | — | a fazer |
| Sistema | Trocar personagem | setas | volta à seleção (a mesma rotina de `sair` sem fechar o jogo) | — | — | — | a fazer |
| Sistema | **Sair** | porta | confirmação → `sair()` (hoje é o botão "Sair" do HUD) | — | — | — | existe, mover |

### 3.4 Loja pelo menu: proposta

**A loja de NPC continua exigindo o NPC**, e o menu é o atalho até ele. Os
motivos:

- O servidor valida proximidade em `handle_shop_buy` (`INTERACT_RADIUS`) e
  cada vendedor tem sua lista (`VendorTag.shop_id`). Loja remota mudaria regra
  de servidor e economia, não só HUD.
- A cidade e o porto (VILA_E_PORTO) só têm movimento se o jogador precisa
  passar por lá. O MIR4 faz o mesmo: o botão de poção **mostra onde fica o
  vendedor** e leva até ele.
- "Tudo via menu" continua valendo: Menu → Loja → Ir, e o personagem vai
  sozinho, correndo, e abre a loja ao chegar. São dois cliques e zero caminhada
  manual.
- **Exceções remotas:** a loja de TP (cash) e, se o usuário quiser, "Repor
  poções" remoto com +20% de preço (ralo de ouro). ⚠️ Decisão do usuário.

### 3.5 Craft e Forja sem NPC: consequência

O usuário pediu craft e forja pelo menu em qualquer lugar, e o MIR4 faz igual
(Craft → Crafting sem mercador; Forge → Enhance). Lendo o servidor
(`handle_craft`, `handle_refine_item`) não achei checagem de distância, mas
existem `GrabStation`/`ReleaseStation` e as estações "da praça"
(`craft_station`). ⚠️ **Conferir** se algum caminho do craft ainda exige estar
na estação. Se as estações físicas continuarem no mundo, viram decoração e
ponto de encontro, sem regra própria. Não pode existir "craft só funciona
perto": o menu seria mentira.

---

## 4. Padrão de painel

### 4.1 Hoje (lido no código)

| Módulo | Onde desenha hoje | Como fecha | Problema |
|---|---|---|---|
| `bolsa.rs` | centro, até 1080×680, escurece 35% | I, Esc, X | já é quase o padrão; o botão "Bolsa [I]" (sw−136, sh−52) **se sobrepõe** ao rótulo "Z ligar/parar" do AUTO COMBATE (y = sh−34) |
| `missoes.rs` diário (J) e janela do Mestre | esquerda, x = 24, y = 16% da altura, 360 de largura | J, Esc, X; a do NPC fecha longe | diário e janela de NPC usam o mesmo painel |
| `menu_missoes.rs` (L) | centro, 540 de largura | L, Esc, X; botão "Todas" abaixo do rastreador | é um segundo painel de missões |
| `mapa.rs` grande (M) | centro, quadrado + lateral | M, Esc, X | sem escurecer |
| `loja.rs` | esquerda, x = 24, 360 de largura | Esc, X, afastar-se | ocupa o mesmo lugar do diário |
| `dialogo.rs` | centro-baixo, 600×176, y = sh−430 | Esc, X | ok, fica como "janela leve" |
| `hud.rs` canal | sup. dir. 194×94, com "Sair" | — | "Sair" no HUD |
| `mapa.rs` mini | sup. dir., y = 118, 194×194 | — | pequeno a 1080p |
| `missoes.rs` rastreador | **sup. dir.**, y = 322 | — | vai para a esquerda |
| `habilidades.rs` barra | 3 círculos em linha, inf. dir., y = sh−142 | — | vira arco |
| `auto_combate.rs` / `auto_coleta.rs` | coluna à direita, y = sh−144 / sh−262 | — | na mesma coluna das skills |
| faixas centrais | AUTO: sh−205 · INDO: sh−240 · aviso de skill: sh−235 | — | **três faixas quase na mesma altura, uma em cima da outra** |
| `main.rs` Esc | if/else de 6 ramos; no fim, a condição `!x.pega_mouse() && …` com 11 termos | — | cada painel novo aumenta os dois |

### 4.2 O padrão

**Painel grande (sistema)**, que é o que o Menu abre:

```
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ mundo escurecido 55% ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
░ ┌───────────────────────────────────────────────────────────────────────────────────────┐ ░
░ │ ⚒ OFICINA                                   ● Ouro 12.480  ◆ Darksteel 3.200   ← [X]  │ ░
░ ├─────────────┬─────────────────────────────────────────────────────────────────────────┤ ░
░ │ ▌Forja    • │                                                                         │ ░
░ │  Ateliê     │                         conteúdo da aba                                 │ ░
░ │  Fundição   │                                                                         │ ░
░ │  Carpintaria│                                                                         │ ░
░ │             │                                                                         │ ░
░ ├─────────────┴─────────────────────────────────────────────────────────────────────────┤ ░
░ │ clique: craftar · Shift+clique: ×5                                          [aviso]   │ ░
░ └───────────────────────────────────────────────────────────────────────────────────────┘ ░
```

- **Tamanho:** `min(sw−80, 1600) × min(sh−80, 900)`, centralizado. A escala
  vale igual.
- **Barra de título (56):** ícone e nome do sistema; à direita os **saldos que
  aquele painel gasta** (Forja: ouro e darksteel; Loja: ouro), **←** (volta ao
  Menu quando veio dele) e **X**.
- **Abas verticais à esquerda (220):** ícone, rótulo e selo ●. A aba ativa tem
  um filete dourado. Um sistema de uma aba só não mostra a coluna.
  (O MIR4 põe as abas no alto à esquerda; como aqui o painel é largo e as
  estações têm nome longo, a coluna vertical ganha. Em janela estreita as abas
  sobem para uma linha horizontal.)
- **Rodapé (36):** teclas e o **aviso do servidor** ("Material insuficiente",
  em vermelho por 3 s; hoje cada painel faz o seu aviso).
- **Entrada:** com painel grande aberto, **todo clique e roda são dele**
  (`pega_mouse = true` na tela toda). Isso substitui a condição de 11 termos
  do `main.rs`. **WASD continua andando** (a bolsa já é assim) e o AUTO continua
  rodando. O HUD por baixo é desenhado **só com EXP e a faixa de estado**. O
  resto some, como no MIR4.
- **Pilha de telas:** `Nenhuma | Menu | Painel(Sistema, aba)`. Abrir pelo Menu
  empilha (← ou Esc volta ao Menu); abrir por ícone do HUD não empilha (X ou
  Esc volta ao jogo). Clicar de novo no mesmo ícone do HUD fecha. **Nenhuma
  tecla abre** (2.5). **Só um painel grande por vez.** Clicar fora não fecha,
  para não fechar sem querer no meio de um refino.
- **Um enum só no cliente** (`telas.rs`):
  `enum Sistema { Personagem, Missoes, Mapa, Oficina, Forja, Aventuras, Grupo, Lojas, Config }`
  e `abrir(Sistema, aba)`. Rastreador, notificações e selos pedem "abra X na
  aba Y" por aqui.

**Janela leve (contextual)**, que o mundo abre e não o Menu:

| Janela | Posição | Fecha |
|---|---|---|
| **Diálogo** (`dialogo.rs`) | centro-baixo, acima do cluster (como hoje) | Esc, X, fim da conversa |
| **Janela de NPC**: loja, oferta do Mestre, cofre | **esquerda, (20, 164)**, 480 × até 70% da altura, **por cima do rastreador** (o rastreador some enquanto ela está aberta) | Esc, X, **afastar-se** (a regra `FECHA_LONGE` que já existe) |
| Pronto-check de dungeon | modal central de 520×260 | aceitar/recusar/tempo |
| Tooltip | segue o mouse, preso à tela (como hoje) | — |

Janela leve **não escurece** o mundo e só pega o mouse dentro dela. Janela leve
e painel grande não convivem: abrir um painel grande fecha a janela leve, menos
o diálogo, que bloqueia abrir painel.

### 4.3 Cada painel existente no padrão

| Hoje | Vira | Abas |
|---|---|---|
| Bolsa (I) | **Personagem** | Bolsa (a tela atual inteira: equipamento à esquerda, grade à direita, cartão) · Atributos · Habilidades · Montaria 🔒 |
| Diário (J) + Todas (L) | **Missões** | Em andamento · Todas · Semana 🔒 |
| Janela do Mestre (oferta/entrega) | **janela de NPC** (leve) | Disponíveis · Em andamento (como hoje) |
| Mapa grande (M) | **Mapa** | Ilha (mapa + lateral de filtros e "Ir para") · Mundo 🔒 |
| Loja (NPC) | **janela de NPC** (leve); pelo Menu, o painel **Lojas da ilha** (lista + Ir) | — |
| Diálogo | igual (leve) | — |
| Craft (novo) | **Oficina** | Forja · Ateliê · Fundição · Carpintaria |
| Forja (novo) | **Forja** | Refino · Combinar · Encantar 🔒 |

---

## 5. Mapeamento atual → novo, por módulo

| Módulo | Muda |
|---|---|
| `hud.rs` `draw_hud` | O painel de canal vira **F (área e canal)** a (1580, 80) × 320×56. Sai o "Sair". O chat vai para 480×170 com abas. `pega_mouse` passa a ler `hud_layout::zonas`. |
| `hud.rs` `draw_ficha` | 440×130. Sai o texto "I Inventário". Entra a linha de buffs. |
| `hud.rs` `draw_alvo` | 460×80. Mesma regra de descer em tela estreita, com o limite vindo do layout. |
| `hud.rs` `draw_exp` | Altura 6. |
| `hud_estilo.rs` | Novos ícones: menu, mochila, pergaminho, bigorna, chama, bússola, portal, grupo, balança, engrenagem, porta, sino. Chip de tecla (Alt). Selo ●/NOVO/🔒. |
| **novo** `hud_layout.rs` | `escala()`, `zonas(sw, sh, escala) -> Zonas` (todos os Rect), teste de não-sobreposição. |
| **novo** `telas.rs` | Pilha de telas, `Sistema`, `abrir/fechar/voltar`, **cascata do Esc**, `pega_mouse` global. |
| **novo** `painel.rs` | Moldura do painel grande: título, saldos, abas verticais, rodapé, aviso, ← e X. |
| **novo** `menu.rs` | Botão ≡, tela do Menu, grade, selos, coluna de retrato e saldos. |
| `bolsa.rs` | `tela()` passa a receber o `Rect` de conteúdo do `painel.rs`. Sai `botao_da_bolsa()`. Vira a aba Bolsa de Personagem. |
| `habilidades.rs` | `retangulo(slot)` → posição no arco em volta de J (e slot 4). A dica fixa sai. O aviso passa para a faixa de estado única. |
| `auto_combate.rs` / `auto_coleta.rs` | `retangulo()` vem do layout (linha de baixo, à esquerda do arco). Os rótulos "Z…"/"X…" viram chip de Alt. As faixas de estado passam para a faixa única. |
| `ir_para.rs`, `auto_missao.rs`, `mapa.rs` `desenha_viagem` | Deixam de desenhar faixa própria e **informam** o estado para a faixa única. |
| `mapa.rs` | `mini_rect` → G (320×320, abaixo de F), com as coordenadas embaixo. `desenha_grande` passa a desenhar dentro do conteúdo do painel Mapa. |
| `missoes.rs` | `rastreador_rect` → **esquerda** (20, 164), com abas Missões/Grupo. `desenha` (diário) vira a aba *Em andamento*. A janela do Mestre vira janela de NPC na esquerda. |
| `menu_missoes.rs` | Vira a aba *Todas*. Sai `botao_rect` (o link "Todas as missões (L)" fica dentro do rastreador). |
| `loja.rs` | Vira janela de NPC (esquerda, 480). Novo painel **Lojas da ilha** lista os vendedores com Ir. |
| `dialogo.rs` | Só a posição vem do layout. |
| `ganhos.rs` | Igual. Novo toast de loot raro à direita. |
| `main.rs` | **Removem-se** `is_key_pressed(KeyCode::I / M / J / L)` (linhas ~379–398) e o "I alterna a bolsa". O Esc passa para a cascata de `telas.rs` (só fecha/cancela). Não se cria tecla nova de abrir. Teclas de ação conforme a tabela da 2.5: **entram** F (atacar), Tab (próximo alvo), C (poção), 8/9/0 (slots), Alt (chips), Alt+Enter; **mudam** R/F de zoom para a roda do mouse (R fica reservado para o golpe letal); **ficam** WASD, 1/2/3, Z, X, Espaço, Shift (correr, até existir esquiva), Q/E, F3. A condição gigante de `atualizar_alvo` vira `!telas.pega_mouse() && !zonas.contem(mouse)`. |
| `bolsa.rs`, `missoes.rs`, `menu_missoes.rs`, `hud.rs` | Saem os textos com tecla: "Bolsa  [I]", "I  Inventário", "Missões · J · clique: ir", "L fecha · …". |
| Craft/Forja (outro agente) | **Sem tecla de abrir.** **Devem desenhar dentro de `painel.rs`** (receber o `Rect` de conteúdo e a aba ativa). Se chegarem antes do `painel.rs`, basta que o desenho aceite um `Rect` de conteúdo em vez de calcular a própria posição, e o acesso provisório é um ícone no HUD: aí a migração é trocar quem passa o `Rect`. |

---

## 5.1 Estado da implementação

**Feito** (cliente, `crates/client/src`):

- `hud_layout.rs`: escala e `zonas()` com todos os retângulos; testes de
  **nenhuma sobreposição** e **tudo dentro da tela** em 1920×1080, 1920×1200,
  1600×900, 1366×768, 1280×720, 1024×768, 960×1080 e 940×980 (a janela padrão).
  O arco das skills ficou 1 à esquerda, 2 na diagonal, 3 acima e o 4 reservado
  à esquerda do 2 (em caixa, não só em círculo). Chip de tecla com Alt, faixa
  de estado única, escurecer.
- **Teclas de abrir removidas:** I, M, J, L. Nenhum painel abre por tecla.
  Esc só fecha (cascata em `Jogo::esc`), e o painel aberto pelo Menu volta ao
  Menu.
- **Topo direito:** ícones Bolsa / Missões (ponto vermelho) / Grupo (em breve)
  / Avisos (em breve) e **≡ MENU** (ponto vermelho); área e canal (clique no
  nome da zona abre o Mapa, "Sair" saiu daqui); minimapa 320 com **⤢** e
  coordenadas.
- **Esquerda:** ficha 440×160 escalada, linha de buffs (Poção de XP) e
  rastreador com abas Missões/Grupo, título "Missões ›" (diário), link "Todas
  as missões", "› AUTO" na missão em auto; some quando a janela do NPC ou a
  loja estão abertas.
- **Alvo** centrado no vão livre do alto, com X que limpa a seleção.
- **Cluster inferior direito:** botão **ATACAR** (F), skills 1/2/3 + slot 4
  reservado, AUTO COMBATE (Z), AUTO COLETA (X), **poção de vida (C)** e slots
  rápidos **8/9/0** (mana, vigor, experiência). Rótulos fixos "Z ligar/parar",
  "X coleta", "Bolsa [I]", "I Inventário", "Arraste ↑ AUTO" saíram.
- **Faixa única:** aviso de skill > INDO > AUTO MISSÃO > Viajando > AUTO
  COLETA > AUTO COMBATE.
- **Menu Principal** (`menu.rs`): tela cheia, coluna com nível, Poder, arma e
  saldos (Ouro, Cobre, Darksteel), grupos Personagem, Progresso, Oficina,
  Aventura, Social, Comércio, Sistema; cadeado + motivo no hover nos "em
  breve"; ponto vermelho em Missões. Abrem: Bolsa, Missões, Todas, Craft,
  Forja, Mapa, Lojas, Sair.
- **Craft e Forja** pelo Menu (Oficina), de qualquer lugar; botões provisórios
  do HUD removidos.
- **Vendedores** (`lojas.rs`, Menu → Comércio): vendedores NPC da vila com distância e só "Ir" (nunca compra de longe, sem taxa). A **Loja** do Menu é a de cash (TP): cadeado "Em breve"
  (anda até o NPC e a loja abre ao chegar).
- **Painel grande** (Menu, Bolsa, Mapa, Craft, Forja, Todas, Lojas): o HUD some
  (fica EXP e faixa), o mundo escurece, todo clique e roda são do painel, um
  por vez. O AUTO continua ligado com painel aberto.
- **Teclas de ação:** F atacar (sem alvo, mira o inimigo mais perto), Tab
  próximo inimigo, C poção, 8/9/0 slots rápidos, Alt mostra teclas,
  Alt+Enter tela cheia. Zoom da câmera só na roda (R/F saíram da câmera).

**Pendente:** moldura única `painel.rs` com abas laterais e ← (os painéis
ainda usam a moldura própria), painéis Ficha/Habilidades/Configurações, fusão
Personagem (Bolsa+Atributos+Habilidades) e Missões (diário+todas em abas),
chat com abas e campo de digitação, aba Grupo com party frames, selo NOVO,
toast de loot raro, economia de energia, escala da UI configurável.

## 6. Tarefas de implementação (em ordem)

| # | Tarefa | Pronto quando |
|---|---|---|
| 1 | **`hud_layout.rs`**: escala e `zonas()` com todos os Rect do HUD atual, **sem mudar posição ainda** | teste de não-sobreposição roda e **falha** nos 2 conflitos conhecidos (Bolsa × rótulo Z; faixas centrais) |
| 2 | **`telas.rs`**: pilha, `Sistema`, cascata do Esc (só fecha/cancela), `pega_mouse` global. Bolsa, mapa, missões e loja passam a abrir por aqui. **Remover as teclas I/M/J/L** e, no mesmo passo, pôr os ícones 🎒/📜 e o clique no nome da zona e no título do rastreador (senão o jogador fica sem acesso) | a condição de 11 termos some do `main.rs`; I/M/J/L não fazem nada; tudo abre por clique; Esc fecha |
| 3 | **`painel.rs`**: moldura com abas, saldos, ←, X, rodapé e aviso | a bolsa desenha dentro dela sem regressão (os testes da bolsa passam) |
| 4 | **`menu.rs`**: botão ≡ (só clique), grade com todos os itens da 3.3 (🔒 nos em breve) | todo sistema existente abre pelo Menu; ← e Esc voltam ao Menu |
| 5 | **Craft e Forja no Menu**: ligar os painéis do outro agente como `Sistema::Oficina` e `Sistema::Forja`, sem NPC e **sem tecla**; tirar o ícone temporário do HUD | craftar e refinar do meio do mato, pelo Menu, com o servidor validando |
| 6 | **Topo direito**: ícones D, MENU E, área F, minimapa G 320 com ⤢, coordenadas. "Sair" vai para o Menu | a 1920×1080 nada se sobrepõe; clique no nome da zona abre o Mapa |
| 7 | **Esquerda**: ficha 440, rastreador na esquerda com abas Missões/Grupo, janela de NPC por cima do rastreador | clicar numa missão continua ligando a auto missão |
| 8 | **Cluster inferior direito**: botão ATACAR, skills em arco, AUTO Z/X, 2 poções, EXP 6, faixa de estado única | bots/usuário jogam 10 min sem texto sobreposto; arrastar ↑AUTO continua funcionando |
| 9 | **Painéis por fusão**: Personagem (Bolsa + Atributos + Habilidades), Missões (diário e "todas" como abas), Mapa em painel | o título do rastreador e o link "Todas" abrem o mesmo painel em abas diferentes |
| 10 | **Selos e NOVO**: ponto vermelho por sistema, propagado ao MENU | missão pronta acende Missões e MENU; abrir apaga |
| 11 | Configurações (escala da UI, minimapa, combate, atalhos das teclas de ação); **Alt mostra os chips**; F atacar, Tab próximo alvo, C poção, 8/9/0 slots; zoom da câmera na roda | escala de 80% e 120% passam no teste de layout; R/F não mexem mais na câmera |
| 12 | **Painel Lojas da ilha** (lista de vendedores + Ir) | Menu → Loja → Ir → a loja abre ao chegar |
| 13 | **Economia de energia** (o Save Power do MIR4): tela preta com resumo (tempo, XP/h, itens, HP), sem desenhar 3D, 10 fps. Liga pelo Menu, ou sozinha após N min ocioso com AUTO ligado; qualquer tecla sai | GPU perto de 0% em sessão pelo Moonlight; o AUTO continua |
| 14 | Toast de loot raro, chat com abas, aba Grupo com party frames | — |
| 15 | Aventuras (DUNGEONS F2) entra no Menu no lugar do 🔒 | — |

Os passos 1 a 4 não mudam nada visível para quem joga. O 5 destrava o pedido
imediato (craft e forja pelo menu). A mudança de lugar vem dos passos 6 a 8.
**Todo passo que mexe em posição deve ser testado com o usuário jogando** (e
não reiniciando o cliente dele no meio da partida).

---

## 7. Decisões pendentes (usuário)

**Já decidido:** nenhum painel abre por tecla, e Esc só fecha (2.5). Teclas de
ação "igual no MIR4" (tabela da 2.5), com o botão ATACAR + F.

1. **Loja:** só atalho até o NPC (proposta), ou compra remota? Se remota, só
   "Repor poções" com +20%?
2. **Andar cancela o AUTO?** No MIR4 cancela; no Tempest hoje não (a área do
   AUTO acompanha o personagem). É regra de jogo, então ficou como está.
3. **Estações físicas de craft** da praça: viram só decoração, já que craft e
   forja passam a ser pelo menu em qualquer lugar?

---

## Fontes

- MIR4 Official Community, guia da tela padrão (elementos numerados: topo, lado direito, combate): https://forum.mir4global.com/post/17
- MIR4 Official Community, configurações de jogo (Save Power em Convenience): https://forum.mir4global.com/post/52
- MIR4 Official Community, Enhance (Forge → Enhancement pelo menu): https://forum.mir4global.com/post/34
- MIR4 Official Community, Hotkeys (PC) (Alt mostra; movimento, combate, slots rápidos, menu padrão e menu de ação configuráveis): https://forum.mir4global.com/post/25
- MIR4 Official Community, Default Attack/Skill (F ataque; skills 6…1 da esquerda; R golpe letal; arrastar para auto): https://forum.mir4global.com/post/26
- gameplay.tips, MIR4 Combat Guide (Tab alvo; C poção; 8/9/0 slots rápidos; AUTO usa skills e poções): https://gameplay.tips/guides/mir4-combat-guide.html
- Steam, "Autobattle" (os dois AUTO ficam à esquerda das poções; movimento manual desliga o auto): https://steamcommunity.com/app/1623660/discussions/0/5116650215569517764
- gameplay.tips, MIR4 Interface Guide: https://gameplay.tips/guides/mir4-interface-guide.html
- gameplay.tips, MIR4 Basic Controls (WASD, Shift, Espaço, G, Alt): https://gameplay.tips/guides/mir4-basic-controls.html
- gameplay.tips, MIR4 System FAQ for PC Users (Alt mostra atalhos, Alt+Enter, PC System Hotkey): https://gameplay.tips/guides/mir4-system-faq-for-pc-users.html
- gameplay.tips, MIR4 Game/System Settings: https://gameplay.tips/guides/mir4-game-system-settings.html
- gameplay.tips, MIR4 Definitive Craft System Guide (Craft → Crafting/Combine/Unseal/Exchange; Forge → Enhance): https://gameplay.tips/guides/mir4-definitive-craft-system-guide.html
- MIR4 Wiki, Crafting Menu ("craft sem a ajuda de um mercador"), via resultado de busca (a página deu erro 500): https://www.mir4.wiki/wiki/Crafting_Menu
- MIR4 Wiki, Inner Force (Train → Constitution/Inner Force, abas no alto à esquerda): https://www.mir4.wiki/wiki/Inner_Force
- BlueStacks, MIR4 Beginner's Guide (joystick à esquerda, botões à direita, engrenagem das poções, auto-path pela missão): https://www.bluestacks.com/blog/game-guides/mir-4/m4-beginner-guide-en.html
- LevelWinner, MIR4 Beginner's Guide (ícone de menu no alto à direita, Auto-Play de missões): https://www.levelwinner.com/mir4-beginners-guide-tips-tricks-strategies/
- Steam, "NEW HUD/UI FOR PC PLAYERS" (UI de celular no PC; reduzir o tamanho em Settings → Convenience): https://steamcommunity.com/app/1623660/discussions/0/3032600513512926775/
- Magic Square pelo ícone Portal do menu (via resultado de busca): https://www.touchtapplay.com/mir4-magic-square-guide/

## Estilo visual

A cara da interface mora num lugar só: `crates/client/src/hud_estilo.rs`. Layout e posições continuam em `hud_layout.rs`, e o teste de não sobreposição segue valendo; o estilo só muda como cada retângulo é pintado.

### Tokens

| Token | Uso |
|---|---|
| `FUNDO` / `FUNDO_ALTO` | vidro escuro dos painéis, em gradiente de cima (alto) pra baixo |
| `FUNDO_BAIXO` | áreas rebaixadas: trilho de barra, slot vazio, campo de texto |
| `BORDA` / `BORDA_FORTE` | borda de 1 px quase invisível; a forte no hover e em tooltip |
| `BRILHO` | filete claro no topo (sensação de vidro) |
| `OURO` | raridade, chefe, valores, títulos de janela |
| `ACENTO` | seleção, hover, foco, aba ativa |
| `TEXTO` / `SUAVE` | texto principal e secundário |
| `AUTO`, `VERDE`, `AZUL`, `VERMELHO` | estados |

Raios: `RAIO_PEQUENO` 5, `RAIO` 9, `RAIO_GRANDE` 14. Tipografia (`tam`): título 20, subtítulo 16, corpo 14, legenda 12, mini 11. Contraste medido por teste (WCAG sobre cena de grama escura): texto ≥ 7:1, suave/ouro/acento ≥ 4,5:1.

### Primitivas

A macroquad não tem retângulo arredondado. `ret_gradiente` desenha **uma malha** em leque a partir do centro, com a cor interpolada por altura; `borda_arredondada` é um anel de quads entre dois contornos. Sem sobreposição de peças, cor translúcida não escurece nos cantos, e o batcher junta tudo no mesmo draw call. `sombra` são três camadas crescendo e sumindo. `traco` é linha com ponta redonda (mesma "caneta" em todo pictograma).

### Componentes

`painel`, `painel_destaque` (filete colorido no topo), `cartao`, `botao` (primário/secundário), `botao_redondo` (ATACAR, skills, AUTO, nível), `aba`, `chip_tecla`, `barra` (pílula com trilho, gradiente, brilho e "fantasma" do que acabou de sair), `slot` (moldura e brilho na cor da raridade), `tooltip`, `badge` (ponto vermelho pulsando), `separador`, `scroll`. Estados: `Normal`, `Sobre`, `Pressionado`, `Ativo`, `Desabilitado` (prioridade testada). Hover e pressão animam em ~120 ms por `anima`, guardado por posição do widget, sem mudar assinatura de ninguém.

As telas de fora do mundo (`ui.rs`: login, servidores, personagens) usam os mesmos componentes.

### Fonte

Noto Sans Regular e Bold (Google), embutidas com `include_bytes!` como antes; licença em `assets/fonts/LICENSE-NotoSans.txt`. Números de dano, "+N item" e quantidade no ícone também usam a fonte da UI. Quem alinha texto mede com `medir`, `medir_forte` ou `medir_dim` — medir com a fonte padrão da macroquad e desenhar com a Noto desalinha.

### Custo

Um `painel` antigo eram ~22 triângulos (retângulos e linhas). O novo são ~210 (sombra 3 leques, gradiente 1 leque, borda 1 anel), em malhas pequenas que o batcher agrupa. Com 30 painéis na tela são ~6 mil triângulos por quadro no passe de UI — irrelevante até em GPU de celular, e sem textura nem render-to-texture.
