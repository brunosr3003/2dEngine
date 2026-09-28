# Modo economia de energia

No molde do MIR4: o jogo segue no automático com a tela acesa, mas quase
sem gastar bateria. Código: `crates/client/src/economia.rs`.

## Como liga

- **Bateria** no canto inferior esquerdo da tela, sempre visível (com ou sem
  painel aberto). O joystick termina acima dela.
- **Menu → Sistema → Interface → Ativar agora.**
- **Sozinho**: sem tocar na tela por N minutos (Interface: Nunca, 3, 5 ou
  10 min). Padrão: 5 min no celular, nunca no PC. Fica salvo nas
  preferências do personagem (`Preferencias::economia_auto_min`).

## O que muda

- O mundo 3D, o HUD e os painéis **não são desenhados**. Fundo preto (pixel
  apagado no OLED), só um resumo pequeno e escuro.
- O quadro cai pra **10 fps** (`economia::FPS`); o terreno não gera malha
  nova.
- Rede, auto combate, **skills automáticas**, auto coleta, auto missão e poções
  automáticas seguem rodando nesse ritmo.
  - As skills **não seguiam** até 28/09/2026: `bloqueia_entrada` entra no
    `teclado_bloqueado()`, e `usar_habilidade` saía inteiro nele, levando a
    rotação AUTO junto com o gesto e a tecla. Com a tela preta o personagem não
    lançava nada — nem a skill de cura, que é o que sustenta — e morria em lugar
    onde aguentava jogando. Entrada bloqueada vale pro toque, não pro
    automático: `habilidades::pedido_automatico`.
- **Tela sempre acesa** enquanto joga (no iOS, `idleTimerDisabled`), com ou
  sem o modo: no automático ninguém toca e o iPhone apagaria e pausaria o app.
- O resumo escorrega uns pixels devagar, pra não marcar a tela.

## O resumo

Nome, nível, HP, EXP, o que está fazendo (AUTO COMBATE, AUTO COLETA, AUTO
MISSÃO, AGUARDANDO VOCÊ, MORTO, PARADO, SEM CONEXÃO), tempo no modo e o que
rendeu desde que ligou: XP, ouro, níveis, mortes e cada item que entrou na
bolsa.

## Como sai

Só **deslizando** o trilho de baixo até o fim. Toque no preto não anda, não
mira e não desliga o auto; o dedo que deslizou ainda fica bloqueado por 0,4 s
depois de sair.

Ao destravar abre a janela flutuante **"Enquanto você estava fora"**: tempo
ausente, XP, ouro, níveis, mortes e cada item que entrou na bolsa (ícone,
quantidade e nome, o mais numeroso primeiro). Fica aberta até tocar em **OK**;
enquanto aberta, o toque não chega ao mundo.
