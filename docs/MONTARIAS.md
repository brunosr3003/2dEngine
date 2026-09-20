# Montarias

A montaria é **item de bolsa**, como o pet (docs/PETS.md): o grau (a cor) mora
no `item_id`, e por isso ela é **negociável no mercado** e **combinável** na
aba Combinar do Craft. Equipada no slot `EquipSlot::Montaria`, é nela que se
monta.

**Não há nível** — de propósito. O pet tem, porque ele trabalha; a montaria só
leva você de um lado pro outro.

## A cor é a variação

O sistema de **skins acabou**. A cor do grau é a aparência: ela tinge o bicho
inteiro, que era exatamente o que a skin fazia. Nove skins viraram cinco cores
por espécie, e a aba Skins da loja saiu.

| grau | velocidade montado | pontos de atributo |
|---|---:|---:|
| Cinza | 150% | 4 |
| Verde | 160% | 5 |
| Azul | 170% | 7 |
| Roxo | 180% | 9 |
| Laranja | 190% | 11 |

O **cinza vale o que a montaria única valia antes** (`VEL_MONTADO` = 1,5×):
ninguém ficou mais lento do que já estava, a cor só sobe daí.

A montaria dá **menos** ponto que o pet no mesmo grau — ela só anda. Os pontos
entram como ponto alocado, pela afinidade da espécie:

| espécie | modelo | afinidade |
|---|---|---|
| Lobo da Clareira | `lobo` | SPD 6 · DES 4 |
| Tigre das Neves | `tigre` | FOR 6 · DES 4 |
| Urso de Carga | `urso` | VIT 6 · RES 4 |

## De onde vem

O **Pergaminho de Invocação: Montaria** (500 TP, aba Materiais) sorteia
espécie e cor: Cinza 55%, Verde 28%, Azul 12%, Roxo 4%, Laranja 1%. O
pergaminho entrega um item na bolsa; nada vira posse de conta.

**Combinar**: 3 do mesmo grau tentam 1 do grau de cima, cobrando cobre.
Aposta, como a chave e como o pet — falhar consome as três. 60/40/25/10% por
degrau.

## A regra que isto quebrou

`ECONOMIA.md` dizia que item de TP não muda atributo de combate, e este
documento dizia que toda montaria corre igual: *"pagar mais caro compra
aparência, não vantagem"*. **As duas coisas deixaram de valer para a
montaria**: a cor dá velocidade e atributo.

É a mesma decisão consciente tomada no pet, e o que segura o equilíbrio é o
mesmo: a montaria é negociável, então quem farma compra a cor que quiser no
mercado por gold, do mesmo jeito que compra TP.

## A migração

As posses antigas foram **apagadas e o TP devolvido** (`loja_posses` de
`montaria:%` e `skin:%`, e `loja_montarias` inteira). A decisão foi começar do
zero no sistema novo, não converter. Roda uma vez, marcada por
`montarias_viraram_item_v1` no livro-caixa: reiniciar o servidor não paga de
novo.

## Regras (servidor, `world/loja_mundo.rs`)

- **Montar**: botão com a ferradura no canto de baixo à esquerda (ao lado da
  bateria do modo economia), ou "Montar" na janela Menu → Personagem →
  Montaria. Nada por tecla.
- Leva **1 s** (`MONTAR_S`) com um anel de progresso no botão.
- Só monta **fora de combate** (3 s sem golpear, conjurar nem apanhar),
  **fora de dungeon**, sem coletar, vivo e sem carregar nada. Na cidade pode.
- Montado anda o que a COR da montaria manda (`montarias::velocidade`), sem
  sprint por cima.
- **Desmonta sozinho** ao golpear (inclusive o auto-ataque), conjurar skill,
  apanhar, começar a coletar, cair, entrar em dungeon ou carregar algo. O
  mesmo vale durante a subida: ela é cancelada.
- **Viagem automática** (mapa, "Ir para", auto missão) monta sozinha se
  houver montaria EQUIPADA e o personagem estiver a pé (o cliente pede no máximo
  a cada 6 s). Viagem pelo mapa que chega sem nada automático seguindo
  desmonta. Auto combate não monta: lutar desmonta.
- Quem decide qual montaria vale é o **slot do equipamento**, conferido no
  servidor: desequipar no meio da montada derruba quem estava montado.

## Rede

- `ent_flags::MONTADO` no estado de cada jogador (por tick).
- `EntityMeta::kind` do jogador = **item_id da montaria** (0 = nenhuma), que
  já diz espécie e cor. Quando a montaria
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
