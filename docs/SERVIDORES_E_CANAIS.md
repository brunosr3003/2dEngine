# Servidores e canais

Dois níveis, e eles **não** se misturam.

## Servidor (realm)

`SA01`, `BRASIL1`. Mundo próprio, **banco próprio**, personagens próprios.
Jogador de um servidor não vê nem encontra jogador de outro. Só se cruzam num
**merge**, que é operação de banco, não de rede — juntar dois realms é mover
linhas, e é decisão de operação, não algo que o jogador faz.

**A exceção é a TP.** A Tempest Points é da CONTA e vale em todos os realms —
então ela não pode morar no banco de nenhum. Exige um cadastro de conta único
e um livro-caixa central, fora dos `DATABASE_URL` dos realms. Ver
`docs/ECONOMIA.md`, seção TP.

**O mercado também é global** (um só pra todos os realms). Ele mora no banco
CENTRAL (`DATABASE_URL_CENTRAL`, junto do livro da TP): o item ou o gold sai do
banco do realm em custódia e volta por cartas idempotentes. Não é consulta
atravessando realms — cada realm só escreve na própria saída e lê as próprias
cartas. Ver `docs/MERCADO.md`.

Tem lotação global: `MMO_REALM_CAPACIDADE`, hoje **1500**. Batido o teto, o
supervisor para de abrir canal e quem chega espera na fila. De onde sai esse
número, ver abaixo.

Um realm = um `DATABASE_URL`. É isso que garante o isolamento: não existe
consulta que atravesse servidores por acidente.

## De onde sai a lotação

Três limites, todos medidos — não estimados.

**Processo.** Um canal aguenta **600 jogadores** a ~100% de um núcleo, ainda
sem tick atrasado, mas sem folga. Para operar, 400.

```
jogadores num processo    CPU (1 núcleo)    RSS
        200                    21%          ~17 MB
        400                    46%             —
        600                    98%          760 MB
```

**Banda.** Medida na própria VPS: **707 Mbit/s de upload** (4 conexões
paralelas) e 628 de download — um link de 1 Gbit/s com o overhead esperado.
Conexão única dá ~200 Mbit/s, mas isso é limite de TCP, não do link.

O custo por jogador muda com a aglomeração: 4,4 KB/s espalhado, **12,2 KB/s
amontoado** (600 numa instância). Planejar pelo pior caso, com metade do link
de margem: 44 MB/s ÷ 12,2 KB/s ≈ **3.600 jogadores**.

**RAM.** 760 MB por processo cheio. Oito processos = 6 GB.

Os três convergem em **3.000 a 3.500 por servidor**.

### Por que 1500 e não 3500

A VPS (16 núcleos, 16 GB) **não é só do jogo**: hospeda ERP, relay, OSRM,
Supabase e cinco Postgres. Load 0,72 e 5,2 GB em uso hoje.

Um servidor a 3.000 jogadores tomaria 6 GB e 8 núcleos — e levaria o resto
junto. 1500 cabe com folga e continua sendo bastante gente pra um MMO
começando.

Se o realm encostar nesse teto, a resposta **não é subir o número**: é subir
uma VPS dedicada, e aí os 3.500 valem.

Com a medição de produção o limite ficou mais claro, e é **RAM de mob**, não
CPU: 1500 jogadores a 100 por canal são 15 canais × ~490 MB ≈ **7,4 GB** — e a
máquina tem 16 GB dividido com o resto. O teto de 1500 continua valendo como
decisão, mas quem chega perto dele primeiro é a memória. Duas saídas, quando
chegar a hora: subir a capacidade do canal (menos canais, mesma gente) ou
apertar o `lazy_spawn` pra dormir zona mais cedo.

## Quantos cabem num canal

Medido **na VPS de produção**, com 1000 mobs vivos no mapa, bots fazendo o
caminho inteiro (handshake, login, personagem) e andando a 30Hz:

```
jogadores no canal    CPU (1 núcleo)    RSS      KB/s por jogador   estados/snapshot
        0                  5,8%        ~490 MB          —                  —
       60                 30,4%         532 MB        15,0                47,2
       80                 39,0%         508 MB        14,7                47,1
```

Três coisas saem daí, e as três importam mais que o número escolhido.

**O custo por jogador é linear e pequeno:** ~0,49% de um núcleo cada. De 60
para 80 a CPU subiu na mesma proporção que a população.

**A banda por jogador NÃO cresce com a lotação** — 15,0 KB/s com 60, 14,7 com
80. Quem limita não é a quantidade de gente no canal, é o **cap de AOI**: cada
jogador recebe no máximo 60 entidades, e a 47 estados por snapshot esse teto já
está quase cheio. Encher mais o canal não muda o que chega no celular.

**O mob é custo FIXO do canal, não do jogador.** Os ~490 MB e os 5,8% de CPU
com o canal vazio são os 1000 mobs. Isso inverte a intuição: **canal pequeno é
caro**, porque cada canal novo repovoa o mapa inteiro de novo.

```
1500 jogadores no realm, por capacidade de canal:
   60 por canal → 25 canais → 12,3 GB só de mob
  100 por canal → 15 canais →  7,4 GB
  150 por canal → 10 canais →  4,9 GB
```

Em 16 GB dividido com ERP, Supabase e cinco Postgres, 25 canais não cabem.

### Por que 100 no campo e 40 na cidade

**Campo: 100.** 49% de um núcleo, metade de folga, e o dado móvel do jogador
não muda. O mapa tem 180x140 tiles — 60 jogadores ali dão um por 420 tiles,
quase o tamanho de uma tela. Com 60 o mundo fica *vazio*, e ainda paga o dobro
de mob por jogador.

**Cidade: 40, instância única.** Mapa de 60x50 e todo mundo no mesmo lugar; ali
o limite é visual, não técnico. Lotou, entra na fila.

O teto técnico do processo continua sendo ~200 (0,49% × 200 ≈ 100% de um
núcleo), mas 200 seria operar sem folga nenhuma.

`MMO_CANAL_CAPACIDADE` mora na unit de cada zona, **não** no
`/etc/tempest.env`: o systemd deixa o `EnvironmentFile` sobrescrever o
`Environment=` da unit, então um valor global no arquivo vencia o valor por
zona e a cidade subia com a capacidade do campo.

## Orçar muitos mapas e eventos

Medido em produção, um processo por vez, com 1000 bots no realm:

```
processo de mapa PARADO (0 jogadores)     0,4% de núcleo    19 MB
canal com 22 jogadores                     24%             143 MB
canal com 51 jogadores                     33%             151 MB
canal com 81 jogadores                     44%             164 MB
canal com 91 jogadores                     30-37%          182 MB
```

Duas leituras, e as duas mudam como se planeja o mundo.

**Mapa parado é quase de graça.** 0,4% de núcleo e 19 MB. Trinta mapas
residentes cabem em meio núcleo e 600 MB. Ter muitos mapas não é o problema —
ter muitos mapas *cheios ao mesmo tempo* é.

**O mob é custo-base; o jogador é custo-marginal.** Sair de 0 para 22
jogadores custou 24% de um núcleo — isso é o `lazy_spawn` acordando as zonas.
Ir de 22 para 91 (quatro vezes mais gente) custou treze pontos a mais.
Quadruplicar jogadores custou metade do que custou acordar os mobs.

A consequência é contra-intuitiva: o botão de maior efeito **não** é "menos
jogador por canal". É densidade de mob e raio do `lazy_spawn`. Diminuir a
capacidade do canal só multiplica quantas vezes você paga a base.

### A conta

```
núcleos ≈ 0,2 × instâncias_com_gente + 0,002 × jogadores
RAM     ≈ 19 MB × instâncias_paradas + 180 MB × instâncias_com_gente
banda   ≈ 13 KB/s × jogadores          (teto do AOI, não cresce com a lotação)
```

Conferindo contra o que está no ar: 11 × 0,2 + 1000 × 0,002 = **4,2 núcleos**;
medido entre 3,6 e 4,7.

### Evento em mapa único

`MMO_CANAL_UNICO=1` mais fila. O teto ali **não é decisão de design**: é o teto
de um processo, ~200 jogadores, porque o world loop é uma task só. Evento que
precise de mais gente que isso tem que virar instâncias (estilo raid) — um
boss de mundo com 500 pessoas num processo só não fecha o tick de 33ms, e não
existe configuração que resolva isso.

### O que ainda falta pra muitos mapas

Hoje cada zona é uma unit fixa do systemd. Pra vinte mapas de evento o certo é
o supervisor **subir a zona quando ela abre** e derrubar quando esvazia, como
já faz com canal. Falta a outra ponta: portal pra zona fora do ar hoje só
avisa no log — precisaria acordar o processo e segurar o jogador numa tela de
carregamento.

## Canal

Dentro do servidor, uma instância do mapa. Existe pra **distribuir gente e não
lotar região**. Todos os canais de um realm compartilham o mesmo banco, então
trocar de canal é reconectar — não recomeçar.

Um canal é literalmente outro processo do mesmo binário, com `MMO_CANAL` e
`BIND_ADDR` diferentes.

O que força essa divisão: o world loop é uma task só e para num núcleo —
medido em ~400 jogadores nesta máquina, com 1000 mobs no mapa. O canal resolve
isso *dentro* do servidor.

## Canais abrem e fecham sozinhos

`supervisor` cuida de um realm:

```sh
DATABASE_URL=postgres://…/mmo_sa01 \
MAP_FILE=data/maps/game.json \
MMO_SERVER_BIN=./server \
MMO_REALM=SA01 \
MMO_CANAL_CAPACIDADE=60 \
MMO_CANAL_ABRIR_EM=48 \
MMO_CANAIS_MIN=1 MMO_CANAIS_MAX=8 \
MMO_REALM_CAPACIDADE=1000 \
MMO_PORTA_BASE=9000 \
./supervisor
```

| variável | o que faz |
|---|---|
| `MMO_CANAL_CAPACIDADE` | teto de um canal; acima disso entra na fila (campo 100, cidade 40) |
| `MMO_CANAL_ABRIR_EM` | abre o próximo canal antes de encher (default 80% do teto) |
| `MMO_CANAIS_MIN` / `MAX` | quantos canais podem existir |
| `MMO_CANAL_FECHAR_APOS` | ciclos de 5s vazio antes de fechar (default 6 = 30s) |
| `MMO_REALM_CAPACIDADE` | teto do servidor inteiro; batido, para de abrir canal |
| `MMO_CANAL_UNICO=1` | área de instância única — nunca abre um segundo |

Comportamento medido, com teto de 25 e abertura em 20:

```
t= 7s   SA01/1=24
t=14s   SA01/2=0   SA01/1=24     ← abriu sozinho
t=49s   SA01/2=0   SA01/1=12     ← carga saindo
t=56s   SA01/1=0   SA01/2=0
        (30s depois, canal 2 fecha e sobra o mínimo)
```

O supervisor **não fala com os canais**: lê o mesmo heartbeat que o cliente
enxerga (`/api/channels`). Um jeito a menos de as duas visões discordarem. E
como ele é dono dos processos, canal que morre sozinho é reaberto no ciclo
seguinte — ele serve de supervisor de verdade, não só de balanceador.

## Fila por carga, não só por cabeça

Contar jogador é um proxy — e um proxy ruim quando a carga não é uniforme. 60
pessoas num boss com 300 mobs acordados não é a mesma coisa que 60 espalhadas,
e o teto de população não sabe distinguir. O que quebra de verdade é o tick não
fechar em 33ms: aí todo mundo lagga junto.

Então o servidor mede o **p99 do tempo de trabalho por tick**, numa janela de
10 segundos (300 amostras a 30Hz), e publica junto da população no heartbeat.

p99 e não média: a média esconde exatamente o pico que estraga o combate. E é o
tempo de *trabalho*, não o intervalo entre ticks — esse é fixo por construção e
não diria nada.

```
id       players  tick_ms  % do orçamento
SA01/1        91     5,94            17,8
SA01/11       90     4,58            13,7
SA01/12        0     0,13             0,4
```

Noventa jogadores com mil mobs consomem **menos de um quinto** do orçamento de
tick. O gargalo desta instância nunca foi o tick.

### Quem usa esse número

**O canal**, pra pausar a própria admissão: acima de `MMO_TICK_TRAVA` (0,75)
quem chega entra na fila mesmo com vaga na contagem; volta a admitir abaixo de
`MMO_TICK_DESTRAVA` (0,60). Dois limites e não um — com um só, admitir faz a
carga subir, travar faz cair, e a porta fica abrindo e fechando. Ninguém é
expulso: só a entrada pausa.

O limite nunca **sobe** o teto de população, só desce o efetivo. Um teto que
sobe numa hora calma desaba quando o vizinho da máquina acorda, e esta VPS
divide CPU com o ERP.

**O supervisor**, pra abrir canal por carga: um canal com o tick acima de
`MMO_TICK_ALERTA` (0,70) conta como cheio mesmo com meia lotação. O alerta do
supervisor vem antes da trava do canal de propósito — o canal novo sobe
enquanto o atual ainda tem folga.

### Testado forçando

Com `MMO_TICK_TRAVA=0.15` (limiar artificial, já que 91 jogadores só chegam a
~18%):

```
tick em 16% do orcamento com 75 jogadores; pausando admissao (entra na fila)
tick em 17% do orcamento com 78 jogadores; pausando admissao (entra na fila)
tick em 21% do orcamento com 82 jogadores; pausando admissao (entra na fila)

online: 881 de 1000   ← 119 segurados por CARGA, com vaga na contagem
```

Com o limiar real de volta: 1000 online, zero pausas.

O aviso de lag que já existia (`world loop lag`) só dispara com **10 ticks de
atraso** — um terço de segundo depois de o jogador já estar sentindo. O p99 é o
sinal antecedente: sobe enquanto ainda dá tempo de parar de admitir gente.

## Área de canal único → fila

Cidade, arena, boss de mundo: abrir uma segunda instância destruiria o motivo
de existirem. Ali `MMO_CANAL_UNICO=1` mantém exatamente um processo, e quando
lota a resposta é **fila**, não recusa.

O servidor manda a posição enquanto o jogador espera:

```rust
ServerMessage::FilaDeEntrada { posicao: u32, total: u32 }
```

`GameWorld::tick_fila` roda 1×/s (não 30 — não há motivo), admite em ordem de
chegada enquanto houver vaga, e tira da fila quem desconectou. Quem entra na
fila tem o personagem já carregado guardado em `entrada_pendente`, então
admitir é entrar no mundo na hora, sem passar de novo por login e banco.

Mostrar a posição não é enfeite: sem número na tela, esperar é
indistinguível de estar travado.

## A pegadinha que os canais revelaram

Dois processos subindo juntos rodam as mesmas migrations ao mesmo tempo, e o
Postgres responde `tuple concurrently updated` — um dos dois **morre no boot**.
Metade dos canais simplesmente não sobe.

A correção é um advisory lock em volta da criação do schema
(`pg_advisory_lock(728431)` em `persistence::open_pool`). O primeiro cria, os
outros esperam e encontram tudo pronto. A trava é de sessão, então cai sozinha
se o processo morrer no meio.

Esse bug não existia com um servidor só. Aparece no primeiro dia de canais, e
sempre no pior momento: quando tudo reinicia de uma vez.

## Zonas em processos separados

Cada zona (campo, cidade, dungeon) roda no próprio processo, com `MMO_ZONA` e
`MAP_FILE` próprios:

```sh
MMO_ZONA=campo  BIND_ADDR=0.0.0.0:9000 MAP_FILE=data/maps/campo.json  ./server
MMO_ZONA=cidade BIND_ADDR=0.0.0.0:9020 MAP_FILE=data/maps/cidade.json ./server
```

Portal com `target_map` de outra zona não teleporta: manda o jogador
reconectar.

```rust
ServerMessage::TrocarZona { zona: String, host: String }
```

O host sai do **diretório de zonas** (`canais::Diretorio`), que é alimentado
pelo mesmo heartbeat que o cliente vê — e não por configuração estática, porque
canais abrem e fecham o tempo todo. Zona fora do ar: o portal não funciona e
loga um aviso. Sumir o jogador num portal quebrado é pior que o portal não
funcionar.

**A posição de chegada é gravada antes do handoff.** As zonas são processos
separados que compartilham o banco, então quem recebe lê a posição salva — sem
gravar o `target_spawn` na saída, o jogador apareceria na cidade nas
coordenadas do campo. O cliente reentra sozinho no mesmo personagem; quem joga
vê uma tela de carregamento.

Medido, ponta a ponta:

```
troca de zona: campo -> cidade em (30,25)
[map] carregado de data/maps/campo.json: 180x140
[map] carregado de data/maps/cidade.json: 60x50
SA01/1: 0    SA01/cidade: 1
```

O `MapChange` manda o nome da zona, e o cliente carrega
`data/maps/<zona>.json` — antes ele carregava sempre o mesmo arquivo e mostrava
o campo enquanto o jogador estava na cidade.

## Merge de servidores

`scripts/merge-realms.sh origem destino [--aplicar]`. Sem `--aplicar` é
simulação: lista quantas contas e personagens vão, e quais colidem.

```
 contas_a_mover | chars_a_mover | contas_em_colisao | chars_em_colisao
              3 |             1 |                 2 |                1

 personagem | vira
 bot0_c     | bot0_c-SA02
```

Como funciona: `pg_dump` da origem entra num **schema `origem`** dentro do
banco de destino, e aí o merge vira SQL comum — sem copiar linha por linha por
fora, sem transação distribuída. Falhou no meio? Dropa o schema e nada vazou.

Três detalhes que o teste revelou:

- O `pg_dump` **qualifica tudo como `public.`**, então mexer em `search_path`
  não adianta; é preciso reescrever o prefixo.
- `--section=pre-data --section=data` deixa de fora as **chaves estrangeiras**.
  Sem isso o import procura `origem.skills` (economia não vem no merge, já
  existe no destino) e falha.
- O rename tem que passar em **todas** as tabelas dependentes na mão —
  justamente porque não há FK no schema de passagem, não existe
  `ON UPDATE CASCADE` pra propagar. Trocar só `characters` deixa inventário e
  equipamento apontando pro nome velho.

A confirmação lê do **terminal**, não do stdin: os `docker exec -i` do script
comem o stdin e um `read` normal passaria direto sem ninguém responder.
`MERGE_CONFIRMA=1` pula, pra automação.

## Tela de login

O cliente tem as telas: **servidores** (um por realm — `SA01` —, com o online
somado de todas as ilhas), **login**, **personagens** e **fila** com a posição.

**Não se escolhe ilha nem canal** (decisão de 18/09/2026). Cada ilha roda em
processo próprio, mas para quem joga o servidor é um só: entra-se pela porta da
ilha inicial (`api::porta_de_entrada` — o canal dela menos cheio, sem fila se
houver) e, escolhido o personagem, o servidor o manda para a ilha onde ele está
salvo (`TrocarZona`); o cliente reconecta e reentra no mesmo personagem
sozinho. Personagem novo nasce no Bosque.

São ~200 linhas de widgets de modo imediato em `ui.rs`. O cliente Unity (morto) tinha
24.491 linhas de uGUI pra fazer menos que isso.

A lista vem de `/api/channels` por HTTP cru sobre `TcpStream`, sem dependência
nova — é uma requisição GET num endpoint conhecido.

O campo `single` do heartbeat (`MMO_CANAL_UNICO`) continua indo na lista, mas
a tela não mostra mais canal nenhum.

`MMO_HOST` pula a escolha e `MMO_CHAR` pula a seleção de personagem: é o
caminho do teste de carga, que não tem quem clique.

## Produção

```
https://mmo.brunji.com.br/api/…   nginx → 127.0.0.1:8090 (tempest-web)
ws://mmo.brunji.com.br:9000-9002  campo  (supervisor, até 3 canais de 100)
ws://mmo.brunji.com.br:9010       cidade (instância única, 40, com fila)
```

Três units, todas com `EnvironmentFile=/etc/tempest.env`: `tempest-campo`
(roda o `supervisor`), `tempest-cidade` (roda o `server` direto) e
`tempest-web`. Banco `tempest_sa01`, dono `mmo` — realm próprio, banco próprio.

Duas armadilhas que custaram tempo:

- O `web` lê **`WEB_BIND`**, não `PORT`. Com `Environment=PORT=8090` a unit
  subia `active (running)` escutando em 8080 e o nginx apontava pro lugar
  errado. Unit ativa não quer dizer escutando onde você acha.
- Existem **dois nginx** na máquina. O que serve é
  `/www/server/nginx/sbin/nginx` com
  `/www/server/nginx/conf/nginx.conf`; testar e recarregar o `/usr/sbin/nginx`
  não muda nada e não dá erro nenhum.

O cliente fala HTTP puro na porta 80 (`MMO_API=mmo.brunji.com.br:80`) e
WebSocket puro nas portas do jogo. **Senha trafega em claro** — `api.rs` e
`net.rs` ainda não fazem TLS. Antes de qualquer jogador de verdade, isso tem
que virar `https`/`wss`.

## O que falta

- **Trocar de canal em jogo** sem passar pela tela (o servidor já suporta).
- **Fila do realm**: hoje o teto do servidor só impede abrir canal novo; falta
  a fila global quando todos os canais estão cheios.
