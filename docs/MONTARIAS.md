# Montarias

Vêm do **Pergaminho de Invocação: Montaria** da loja de cash (LOJA.md). Só
**mobilidade**: nenhum atributo de combate muda (regra da TP em ECONOMIA.md).

## Invocação e cópias

- O pergaminho custa 500 TP, vai para a bolsa e só sorteia quando o jogador
  toca em **Abrir**: Lobo 55%, Tigre 30%, Urso 15%.
- A abertura mostra o pergaminho, partículas e a montaria 3D sorteada. O prêmio
  já foi decidido pelo servidor; pular ou fechar a animação não muda nada.
- A primeira cópia libera a montaria e sua skin padrão para toda a conta.
  Duplicatas são válidas e ficam contadas em `loja_montarias`, para a futura
  combinação/evolução de montarias.
- A loja mostra a chance de cada montaria e, nas já obtidas, `POSSUÍDA ×N`.

## Regras (servidor, `world/loja_mundo.rs`)

- **Montar**: botão com a ferradura no canto de baixo à esquerda (ao lado da
  bateria do modo economia), ou "Montar" na janela Menu → Personagem →
  Montaria. Nada por tecla.
- Leva **1 s** (`MONTAR_S`) com um anel de progresso no botão.
- Só monta **fora de combate** (3 s sem golpear, conjurar nem apanhar),
  **fora de dungeon**, sem coletar, vivo e sem carregar nada. Na cidade pode.
- Montado anda **+50%** (`VEL_MONTADO`, igual pra toda montaria), sem sprint
  por cima.
- **Desmonta sozinho** ao golpear (inclusive o auto-ataque), conjurar skill,
  apanhar, começar a coletar, cair, entrar em dungeon ou carregar algo. O
  mesmo vale durante a subida: ela é cancelada.
- **Viagem automática** (mapa, "Ir para", auto missão) monta sozinha se a
  conta tiver montaria e o personagem estiver a pé (o cliente pede no máximo
  a cada 6 s). Viagem pelo mapa que chega sem nada automático seguindo
  desmonta. Auto combate não monta: lutar desmonta.
- Posse validada no servidor (`Posses::skin_para_montar`): skin escolhida que
  a conta não tem cai na skin padrão da primeira montaria.

## Rede

- `ent_flags::MONTADO` no estado de cada jogador (por tick).
- `EntityMeta::kind` do jogador = **id da skin** (0 = nenhuma). Quando a skin
  muda, a meta é reenviada a quem já conhecia a entidade (o servidor tira a
  entidade do `last_sent` de todos; o cliente troca só a meta).
- `Preferencias::montaria_skin` guarda a escolha (salva no personagem).
- `PedidoLoja::Montar / Desmontar`, `AvisoLoja::Montando { segundos }`.
- `AvisoLoja::Invocacao` leva o prêmio autoritativo para a animação; o item de
  bolsa é `PERGAMINHO_INVOCA_MONTARIA`.

## Visual (`render3d::desenha_montaria`, `rig::aplica_montado`)

- O bicho em peças que já existia (`bichos/lobo`, `bichos/tigre`,
  `bichos/urso`) na escala de montaria, andando na velocidade do cavaleiro.
- **Skin = tinta**: a cor de cada vértice é puxada para a cor da skin
  (`draw_mesh_mat_tinta`), sem malha nova — leve no iPhone.
- O cavaleiro senta na sela (`Montaria::sela`, `sela_frente`): pernas
  abertas com o joelho dobrado, mãos na frente, arma guardada.

### Falta de arte

- **Sela, rédea e estribo** não existem: o cavaleiro senta direto no lombo.
- Os bichos são os mobs: sem cabeçada/armadura de montaria, sem pose de
  "empinar" ao montar, sem animação de subir.
- Skin só troca cor; acessórios (chifre, crina brilhante, armadura) pedem
  peças novas no `tools/voxrender/bichos.py`.
- A sela foi ajustada por número (`sela`, `escala`), não medida numa captura
  de tela: pode precisar de ajuste fino olhando no jogo.

## Catálogo

Em `shared::loja::MONTARIAS` e `SKINS` (fonte única). Preços em LOJA.md.
