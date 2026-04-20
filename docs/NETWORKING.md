# Networking

## Modelo: servidor autoritativo + predição client-side

O servidor é a única fonte da verdade. Clientes enviam *intents* e recebem
*snapshots*. Para não sentir lag, o cliente **prediz** o resultado dos
seus próprios inputs localmente, e **reconcilia** quando o snapshot do
servidor chega.

### Ciclo de um frame

```
Cliente (30 Hz)                             Servidor (30 Hz)
─────────────                                ───────────────
1. capturar input (WASD, mouse)
2. aplicar input localmente                 ← predição client-side
   (mover jogador já)
3. enviar InputFrame{seq, move, aim}  ────►
                                             4. recebe InputFrame
                                             5. tick: aplica, simula,
                                                atualiza AOI
                                             6. envia WorldSnapshot{
                                                  last_input_seq: N,
                                                  entities: [...]
                                                }
7. recebe Snapshot              ◄────
8. RECONCILIAÇÃO:
   - descarta inputs com seq ≤ N
   - "rebobina" para a pos autoritativa
   - "refaz" os inputs ainda pendentes
9. render
```

## Transport

Hoje: **WebSocket binário** sobre TCP. Razões em
[ARCHITECTURE.md](ARCHITECTURE.md#por-que-websocket-e-não-udp).

Futuro (Fase 5):

- Nativo: **QUIC** via `quinn` (UDP com streams ordenados +
  datagrams não-confiáveis para snapshots).
- Browser: **WebTransport** quando maduro; WebSocket como fallback.

O `shared::protocol` é agnóstico ao transporte — mensagens são
`Serialize/Deserialize` com bincode. Trocar de WS para QUIC é mudar a
camada de I/O no client/server, sem tocar no protocolo.

## Serialização

`bincode` 1.3 com `serde`. Alternativas avaliadas:

| Formato | Tamanho | Velocidade | Evolução |
|---|---|---|---|
| bincode | ★★★★★ | ★★★★★ | frágil sem versionamento manual |
| postcard | ★★★★★ | ★★★★☆ | similar, mais no-std |
| rkyv | ★★★★★ | ★★★★★ | zero-copy, sintaxe extra |
| JSON | ★☆☆☆☆ | ★★☆☆☆ | máxima debugabilidade |

Mantemos **bincode** pela simplicidade. Evolução do schema: bumpar
`PROTOCOL_VERSION` em `shared/constants.rs`; servidor rejeita cliente
com versão diferente no `Handshake`.

## Interest Management (AOI)

Não queremos mandar o mundo inteiro para cada cliente. Um MMO com 500
jogadores e 2000 mobs em 1 mapa → 2500 entidades × 64 bytes × 30 tick/s
× 500 clients = **2.4 GB/s**. Inviável.

**Solução:** cada cliente só recebe entidades dentro de um raio
`AOI_RADIUS` (24 tiles) da sua posição.

### Implementação atual (ingênua)

`GameWorld::send_snapshots` itera **todas** as entidades vs **todas** as
sessões → O(E × S). Suficiente para testar com dezenas de jogadores.

### Fase 2 — Spatial hash grid

```
cell_size = SPATIAL_CELL_SIZE (16 tiles)
grid: HashMap<IVec2, Vec<Entity>>

rebuild_grid() a cada tick:
  for each entity with Position:
    cell = (pos / cell_size) floor
    grid[cell].push(entity)

query(center, radius):
  cells = all cells intersecting circle(center, radius)
  return entities in those cells
```

Isso transforma o snapshot em O(E + ΣAOI) ≈ linear.

### Fase 3 — Delta / eventual

Hoje mandamos snapshot full do AOI a cada tick. A próxima otimização:

- **Delta snapshots:** mandar só o que mudou vs último snapshot ack pelo
  cliente. Requer IDs estáveis e histórico server-side (ring buffer de N
  snapshots).
- **Event-driven:** eventos raros (spawn, death, chat) em canal reliable;
  state contínuo (posição) em canal unreliable com interpolação.
- **Quantização:** positions em fixed-point 16 bits, não float32.
  Reduz payload ~50%.

## Predição client-side (a implementar)

Ainda não implementado. Plano:

1. Cliente mantém buffer `Vec<InputFrame>` com os últimos N inputs.
2. Ao gerar input: aplica localmente na entidade do próprio jogador
   (mover de acordo com `PLAYER_SPEED`).
3. Ao receber `Snapshot`:
   - `last_input_seq = M` — descarta inputs ≤ M do buffer.
   - Pega `entity.pos` autoritativa do snapshot.
   - Re-aplica os inputs restantes (> M) a partir dessa posição.
4. Se a diferença autoritativa vs predita for grande (> threshold) → snap.
   Se pequena → lerp suave em poucos frames (imperceptível).

Isso dá a sensação de input instantâneo mesmo com 100ms+ de ping.

## Anti-cheat (roadmap)

Primeira passada — **servidor nunca confia no cliente**:

- `ClientMessage::Input` só tem *intent* (dir + aim). Movimento real é
  integrado pelo servidor com `PLAYER_SPEED`.
- Dano não vem do cliente. Cliente manda "apertei o botão de ataque"
  (em `buttons`), servidor valida cooldown, alcance, linha de vista.
- Cooldowns e estado de habilidade são server-side.
- Rate-limit de pacotes por sessão (a implementar).

## Sequência de conexão

```
Cliente                              Servidor
───────                              ────────
TCP connect
WebSocket handshake (HTTP upgrade)
─── Handshake{proto, ver} ────────►
                                  ◄─── HandshakeAck{proto, time}
─── Login{username, token} ──────►
                                      auth (stub / DB)
                                  ◄─── LoginOk{pid, eid, spawn}
(loop)
─── Input{seq, ...} ──────────────►  (30x/s)
                                  ◄─── Snapshot{tick, entities, ...} (30x/s)
─── Chat(text) ───────────────────►
                                  ◄─── Chat{from, text}
─── RequestDisconnect ───────────►
                                     despawn + fechar
```

## Métricas a instrumentar (Fase 3+)

- `tick_duration` (histograma) — alerta se > 25ms no 30Hz.
- `snapshot_bytes` por sessão — spotar crescimento anômalo de AOI.
- `messages_dropped` — backpressure do mpsc cheio.
- `ping_rtt` — cliente mede com `server_time_ms`.

Via `tracing` + um exporter (Prometheus/OTLP).
