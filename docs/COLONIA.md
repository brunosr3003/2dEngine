# A Colônia — a ilha própria

> "a mecanica da ilha propria eu ainda quero, mas ai vc vai receber de quest
> antes de ir pra segunda ilha, e os upgrades serao feitos upgradando a ilha,
> tamanho dela, quantidade de recursos disponiveis, banco local etc"
>
> "teleporta pelo porto mesmo, e ela rende sozinha offline com uma taxa de
> ganho baixa e coleta a cada 12 h mas acumula por 24 perdendo uma parte"
>
> "lugar onde anda"

A colônia é uma ilha de verdade, com relevo, onde o personagem **anda**. Não é
uma tela de gerência: é um lugar, e é por isso que ela custa uma zona.

## Como se chega lá

Pelo **porto**, no mesmo menu do Capitão que leva às outras ilhas — uma linha
"Minha Ilha" no fim da lista. A linha só aparece depois do passo
`historia::PASSO_DA_COLONIA` ("A escritura da ilha"), que fica **antes** de
"Rumo à Geleira": a ilha chega antes da segunda ilha, não depois.

O painel (colher, melhorar, voltar) reabre pelo Menu › AVENTURA › Minha Ilha,
de qualquer lugar. Viajar pra lá, não: isso só do porto.

## Por que instância, e não um pedaço de mundo

A colônia vive numa **instância**, como a dungeon: dois jogadores nunca se veem,
então podem ocupar as **mesmas coordenadas**. A alternativa seria um espaço de
mundo compartilhado com as colônias espalhadas nele, e aí cada vizinho vira
terreno pra gerar e silhueta pra desenhar — foi exatamente isso que encareceu o
Mar Aberto, e foi o que o dono mandou desfazer.

A instância sai do **nome do personagem** (`instancia_do_nome`, FNV). Ela é
estável: reconectar cai sempre na mesma ilha. E ela nunca é 0, porque 0 é "fora
de instância" — é assim que o resto do mundo distingue quem está na colônia.

O **relevo também é por instância**: `GameWorld::colonias[instancia]` guarda a
`Ilha` daquele jogador, gerada de `colonia::semente(nome)`. A 120 blocos de raio
o campo de altura são uns 115 KB, então sessenta colônias vivas cabem em 7 MB.
Quando a última sessão de uma instância cai, `esquece_colonia` joga o relevo
fora: regerar custa milissegundos e guardar custa memória por jogador que
deslogou — a conta só fecha de um lado.

O que persiste são **quatro números**, num JSON só (`colonia_json`): o nível de
cada eixo, quando foi a última colheita, se a quest já entregou, e de que zona o
jogador saiu. A ilha em si nunca é salva: ela é função pura da semente.

## O relógio: 12 h cheias, 24 h de teto

A regra inteira está em `shared::colonia`:

| janela | rende |
|---|---|
| 0 → 12 h | **cheio** |
| 12 → 24 h | **metade** |
| depois de 24 h | **nada** |

`horas_efetivas` é essa curva. Voltar antes das 12 h rende mais por hora do que
deixar acumular, e é essa a única decisão que a colônia pede do jogador — por
isso o painel **nomeia a fase em que você está**, em vez de só mostrar o
relógio. Um teste (`o_relogio_conta_as_tres_fases`) guarda que as três fases
são distinguíveis na tela: se duas dissessem a mesma coisa, a regra seria
invisível.

A conta começa na **entrega da escritura**, não na criação do personagem — senão
a primeira visita pagaria tudo o que "rendeu" desde 1970.

## Os três eixos

| eixo | o que sobe |
|---|---|
| **Tamanho** | o raio da ilha (`raio_blocos = 120 + 40·n`) |
| **Recursos** | quanto ela rende por hora |
| **Banco** | espaços do banco local (`10·n`) |

Cinco níveis cada, custo por peso (`custo`), sem RNG: melhorar **não falha e não
decai**. Subir o **Tamanho** muda o relevo, então a ilha é **regerada na hora** —
e não só descartada, porque sem ilha nenhuma a física cairia no mapa de tiles
velho e o jogador atravessaria o chão da própria colônia até o próximo login.
Quem estava na beirada é descido de novo, porque a costa mudou.

## Encanamento

- Zona `colonia`: `def_da_zona` devolve `None`, `world.ilha` fica `None`, e o
  passo de física resolve o terreno **por instância** com queda pra ilha da zona.
- O mapa de arquivo (`game.json`) **não** é populado aqui: as entidades dele
  caem em coordenadas que não querem dizer nada na ilha do jogador — foi o que
  aconteceu com a zona do mar (231 bichos boiando).
- O relevo do cliente chega por `AvisoColonia::Terreno { semente, raio }`, logo
  **depois** do `MapChange`, que é quem limpa o terreno velho.
- A colônia **não aparece na lista de servidores** (`/channels` filtra
  `zone <> 'colonia'`): não é um lugar que se escolhe, é pra onde o porto manda.
  Ela continua no diretório interno, que é quem o `TrocarZona` consulta.
- Processo fora do ar: o caminho de login já cai no porto desta ilha quando a
  zona salva não tem canal. Ninguém fica trancado fora do jogo.

### Produção

`tempest-prod-colonia.service` (`MMO_ZONA=colonia`, `BIND_ADDR=0.0.0.0:9200`,
`MMO_CANAL_UNICO=1`) mais `tempest-prod-tunel-colonia.service` (túnel SSH
`19200`), e na VPS um `server { listen 9200; proxy_pass 127.0.0.1:19200; }` no
`tcp/tempest.conf` com a porta liberada no `ufw`.

Canal único de propósito: aqui ninguém divide espaço com ninguém, então abrir um
segundo canal não resolveria nada que a instância já não resolva.
