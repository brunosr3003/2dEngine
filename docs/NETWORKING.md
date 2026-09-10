# Networking

## Modelo: servidor autoritativo, cliente que só suaviza

O servidor é a única fonte da verdade e o cliente **não prediz**. Ele recebe a
posição autoritativa e persegue ela com suavização exponencial; a 30 Hz de
tick e 60+ de desenho, isso basta pra o movimento não ler aos trancos.

Isso é decisão, não falta de tempo. Predição paga latência com uma segunda
simulação no cliente — e uma segunda simulação é uma segunda verdade, que
diverge, que precisa de reconciliação, e que dá ao cliente modificado uma
opinião sobre onde ele está. Sem ela, a única coisa que o cliente pode mentir
é sobre qual botão apertou.

O que o servidor decide e o cliente só desenha: posição, rota, pulo, alvo,
dano, loot, level. O cliente calcula a ALTURA sozinho — mas do mesmo campo de
altura que o servidor usa, então não é opinião, é a mesma conta.

```
Cliente (60+ Hz)                            Servidor (30 Hz)
──────────────                               ───────────────
1. captura input (WASD, clique)
2. envia InputFrame{seq, move_dir,   ────►
   aim, buttons}
                                             3. aplica input, simula,
                                                monta AOI
                                             4. envia só quem MUDOU
5. recebe snapshot           ◄────
6. persegue a posição nova
   (suavização, nunca simulação)
7. desenha
```

## O que trafega

| Mensagem | Quando | Tamanho |
|---|---|---|
| `EntityMeta` | uma vez, ao entrar no AOI | nome, tipo, hp máximo, facção |
| `EntityState` | por tick, só de quem mudou | **13 bytes** |
| `ClientMessage::Input` | ~30 Hz | direção, mira, botões |

`EntityState` é `Copy`: posição em 1/16 de tile (`i16`), velocidade saturada
em `i8`, hp em `u16`, um byte de bandeiras. Antes era um struct de 58 campos
com duas `String` dentro — com 100 jogadores vendo 100 entidades, a diferença
é 10 mil clones com alocação por tick contra 10 mil cópias de bloco.

**Não trafega:** terreno, vegetação, altura. Os dois lados geram a ilha da
mesma semente com o mesmo código (`shared::terreno`). Um mapa de 1 km² são
~16 MB no servidor e zero na rede.

## Interest Management (AOI)

Mandar o mundo inteiro não fecha: 2500 entidades × 13 bytes × 30 Hz × 500
clientes = 487 MB/s. Cada cliente recebe só o que está dentro de
`AOI_RADIUS`, com teto de **60 entidades**.

O teto é o que faz o canal escalar: a banda por jogador passa a ser função da
AOI e não da população. Medido em produção com 1000 jogadores em 11 canais:
**12,8 KB/s por jogador**, com o canal cheio ou vazio.

Snapshot é DELTA: o servidor guarda o último `EntityState` enviado por
entidade por sessão e só manda o que mudou. Entidade parada não custa nada.

## Transporte e formato

WebSocket binário sobre TCP, `postcard` (varint, sem nome de campo).

Evolução de schema: subir `PROTOCOL_VERSION` em `shared/constants.rs`. O
servidor recusa cliente de versão diferente no handshake — binário velho
falhando o handshake é ruído de dois minutos; binário velho *quase*
funcionando é bug de meia tarde.

## Movimento por toque no chão

O clique manda `MoverPara{x, z}` — um PONTO, não um caminho. Quem calcula a
rota é o servidor, com A\* numa grade grossa (8 blocos por célula) sobre o
campo de altura, e quem anda é o seguidor de rota do servidor.

O cliente não manda rota porque rota é regra: um cliente modificado que
mandasse a própria rota andaria por cima de paredão. Ver
[MUNDO](MUNDO.md#movimento).
