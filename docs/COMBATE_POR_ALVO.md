# Combate por alvo e o wire binário

Duas mudanças que andam juntas: o jogo virou MMO de target (estilo MIR4) e o
protocolo virou binário com delta. As duas existem pelo mesmo motivo — rodar em
celular com muitos mobs.

## O wire

Era JSON. Com ~40 campos por entidade e o nome de cada campo repetido em toda
entidade, um snapshot custava isto:

```
                 JSON        postcard      + snapshot magro
 30 entidades    0,88 MB/s   0,09 MB/s     0,01 MB/s
100 entidades    2,92 MB/s   0,28 MB/s     0,03 MB/s
300 entidades    8,75 MB/s   0,85 MB/s     0,09 MB/s
1000 entidades         —           —       0,29 MB/s
```

O estado de uma entidade custa **10 bytes** por tick. O meta (tipo, nome,
hp_max) custa 24 bytes e vai **uma vez**, quando ela entra no campo de visao.

Por jogador, a 30 Hz. 100 mobs em JSON davam 3 GB por hora de dado móvel.

Medido por `cargo run --bin wire_size`, que existe pra essa decisão ser tomada
com número.

### O que mudou

- **`postcard`** em `protocol::{encode, decode}`. Não é auto-descritivo: não
  carrega nome de campo, inteiro vai em varint, `Option::None` custa 1 byte.
- **Enums tagueados por fora.** O `#[serde(tag = "type")]` existia pro
  Newtonsoft.Json do cliente Unity. Enum tagueado por dentro escreve o *nome* da
  variante e exige `deserialize_any` na volta — que o postcard nunca vai
  implementar. `Handshake` caiu de 17 pra 8 bytes.
- **Sem `skip_serializing_if` em tipo de wire.** Ele muda a quantidade de campos
  escritos, e sem nome de campo o outro lado não tem como saber que faltou um.
  Removido de `components.rs`, `protocol.rs`, `items.rs`, `skills.rs`.
  `mapfile.rs` manteve, porque serializa pra disco em JSON.
- **Frames binários.** `session.rs` mandava `Message::Text` com
  `from_utf8_lossy` nos bytes. Com JSON passava; com postcard destrói o pacote —
  e o sintoma é o cliente travar no handshake sem erro nenhum.

Os dois lados compilam a MESMA definição, porque cliente e servidor usam o crate
`shared`. Era não ter isso que derrubou a produção com web em 63 e cliente em 66.

### Delta

Cada sessão guarda `last_sent: HashMap<EntityId, EntitySnapshot>`. Vai no wire
só quem **mudou** desde o tick anterior daquela sessão. Mob parado não ocupa
byte.

Duas consequências:

- O que sumiu do AOI entra em `removed`. O `removed` global só cobre entidade
  destruída, não entidade que ficou longe demais.
- O cliente **funde** o snapshot em vez de substituir a lista. Entidade ausente
  do pacote é entidade parada, não entidade que sumiu.

`EntitySnapshot` ganhou `PartialEq` pra isso.

## Combate por alvo

O ataque básico não depende mais de botão nem de mira.

```
ClientMessage::SetTarget { target: Option<EntityId> }
```

O cliente manda **só quando o alvo muda**. O servidor guarda em `Session.target`
e, a cada tick, dispara o ataque sozinho enquanto o alvo estiver vivo e dentro
do alcance da arma — `MELEE_RANGE` (1,8) ou `RANGED_ATTACK_RANGE` (9,0), o mesmo
alcance que o Ranger inimigo já usava.

A direção até o alvo alimenta o cone/projétil que já existia: **o pipeline de
dano não mudou, só mudou quem aponta.** As 56 skills também seguem valendo — a
tabela sempre foi de tab-target (`target_type`, `range_tiles`, `cast_time_s`,
`cooldown_s`).

`target_pos` é montado uma vez por tick, não por jogador: varrer o ECS por
jogador seria O(jogadores × entidades), o oposto do que "aguentar muitos mobs"
pede.

No cliente: clique esquerdo seleciona (por distância em **pixels** entre o
cursor e a entidade projetada na tela), direito ou Esc limpa, anel dourado no
chão marca o alvo. Alvo que sai do mundo deixa de ser alvo.

## Snapshot magro

`EntitySnapshot` tinha **58 campos**. O cliente 3D usa 7. A maioria existia pro
paper doll 2D do Unity (`visual`, `skin_preset`, `attack_anim`, `combo_step`,
`hurt_dir`, `sprite_id`), pro combate de ação (`aim_dir`, `is_crit`,
`poise_active`, `defending`) ou pra amarração dos barcos (20 campos de leme,
vela, âncora, estação).

Virou dois tipos:

- **`EntityMeta`** — tipo, nome, hp_max. Vai uma vez, quando a entidade entra
  no AOI.
- **`EntityState`** — id, posição, velocidade, hp, flags. 13 bytes, `Copy`, sem
  nenhuma `String`.

A posição foi quantizada pra **1/16 de tile** em `i16`: 4 bytes em vez de 8, e
±2048 tiles de alcance. Erro de um décimo de pixel numa vista de cima ninguém
enxerga, e o cliente interpola por cima.

Isso também matou o custo do delta: a linha de base por sessão guardava o struct
de 58 campos com duas `String`; agora guarda 13 bytes copiáveis.

## Cone e projétil, fora

O projétil-entidade existia pro combate de ação, onde a flecha podia errar. Com
alvo, o acerto é decidido por distância no mesmo tick — e some N entidades
ticando a 30 Hz por tiro dado.

O cone sobrou **só pros mobs**, que ainda batem em área. Ataque com `target`
acerta exclusivamente a entidade marcada, por distância.

## Fan-out por jogador — medido

`cargo run --release --bin loadbot -- --n 200 --secs 50` sobe N jogadores
falsos que fazem o caminho inteiro (handshake, login, criação de personagem) e
depois andam em círculo mandando input a 30 Hz — o pior caso pro delta, já que
jogador parado não gera tráfego. Contas: `scripts/loadbot-accounts.sh`.

Mapa com 1000 mobs, tudo numa máquina só:

```
jogadores   CPU (1 núcleo)   KB/s por jogador   estados/snapshot
      0          0,7%              —                  —
     50         11,5%            42,5                131
    200         39,2%            66,4                214
```

200 jogadores + 1000 mobs, zero tick atrasado, zero falha de conexão.
Escala perto de linear: ~0,19% de CPU por jogador.

**O teto é o núcleo.** O world loop é uma task só — a partir de ~400–500
jogadores num processo os ticks começam a atrasar. MIR4 e similares resolvem
isso fatiando o mundo em processos por zona, não otimizando o loop.

**Cuidado ao medir:** `ps -o %cpu` dá a MÉDIA desde o início do processo. Num
servidor que ficou ocioso antes do teste isso esconde a carga inteira — chegou
a reportar 4% onde o custo real era 39%. O número certo sai do delta de
`/proc/PID/stat`.

**O login é o pico.** Durante a entrada dos 200 bots a CPU foi a 113% (várias
threads): é o argon2. Auth em serviço separado é o padrão de mercado, e aqui
faz sentido pelo mesmo motivo.

## Cap de AOI e taxa por distância

Duas regras, medidas com o mesmo teste de 200 jogadores e 1000 mobs:

```
                        antes      depois
KB/s por jogador         66,4       14,7     4,5x menos
estados/snapshot          214       46,4
CPU (200 jogadores)     39,2%      28,8%
dado móvel por hora     238 MB      53 MB
```

**Teto por jogador** (`AOI_MAX_ENTIDADES = 60`). Os candidatos do AOI são
ordenados por distância e o pacote leva os mais próximos. O que sobra de fora é
o que o jogador menos enxerga.

`AOI_HISTERESE = 12` dá folga pra entidade já conhecida sobreviver um pouco
além do teto. Sem isso ela entra e sai a cada passo do jogador, e reenviar o
meta (24 bytes) come o que o teto economizou.

**Taxa por distância.** Até `AOI_PERTO` (10 tiles) atualiza todo tick; até
`AOI_MEIO` (17), a cada 3; além disso, a cada 6. O cliente interpola, então um
mob a 20 tiles andando a 5 Hz continua liso na tela.

O `(tick + id) % periodo` espalha os vencimentos entre os ticks. Sem o `+ id`
todos os distantes venceriam no mesmo tick e o tráfego viraria serrote.

O próprio personagem do jogador é exceção às duas regras: sempre cabe, sempre
todo tick.

## O que ainda não foi feito

Falta, em ordem de impacto:
- **IA escalonada.** Mob decide 30 vezes por segundo. Mercado roda IA a 2–5 Hz
  em baldes distribuídos entre os ticks.
- **Taxa por distância.** Todo mundo no AOI recebe na mesma frequência.
- **`rapier2d`** pra 1000 mobs — este jogo precisa de círculo-contra-grade.
- **Aggro e leash** continuam os da IA antiga.
