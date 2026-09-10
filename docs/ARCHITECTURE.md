# Arquitetura

Cinco binários, um crate de regra no meio. A separação não é organização: cada
linha entre eles existe porque alguma coisa deu errado quando não existia.

```
                    ┌──────────────┐
                    │    shared    │  regra, terreno, protocolo
                    └──────┬───────┘
        ┌──────────────┬───┴────┬──────────────┬─────────────┐
   ┌────▼────┐   ┌─────▼────┐   │        ┌─────▼─────┐  ┌────▼─────┐
   │ server  │   │  client  │   │        │ panoptico │  │ admin_cli│
   │ (canal) │   │ (jogo)   │   │        │  (painel) │  │  (ops)   │
   └────┬────┘   └─────┬────┘   │        └─────┬─────┘  └────┬─────┘
        │   WebSocket  │        │              │ HTTP        │
        └──────────────┘   ┌────▼───┐          │             │
                           │  web   │          │             │
                           │(cadastro,│        │             │
                           │ canais) │         │             │
                           └────┬───┘          │             │
                                └──────────────┴─────────────┘
                                        Postgres
```

## `shared` — o que os dois lados precisam concordar

Regra de jogo, geração de terreno, protocolo, física de separação, forja.

O critério pra uma coisa morar aqui não é "os dois usam": é **os dois precisam
chegar à mesma resposta**. Terreno mora aqui porque o cliente desenha o chão
que o servidor colide. O plantio de árvore subiu pra cá no dia em que o tronco
passou a barrar passagem — enquanto era só desenho, o cliente decidia sozinho.

Quando essa regra foi quebrada, o resultado foi sempre o mesmo formato de bug:

* o cliente desenhava a árvore num lugar e o servidor barrava noutro;
* o arco do pulo subia 1,7 e a regra de degrau liberava 1,5;
* a câmera calculava o recuo de um jeito e o desvio de morro de outro.

Nenhum deles apareceu como "os dois discordam". Apareceram como árvore
atravessável, boneco colando no barranco e câmera subindo por um morro que não
estava lá.

## `server` — um processo por canal

Simulação autoritativa a 30 Hz. Cada canal é um processo com o seu próprio
mapa em memória; canais da mesma zona são instâncias independentes da mesma
ilha, e o supervisor abre e fecha canais por população **e por saúde de tick**
(p99 do trabalho por tick, não só lotação — canal pela metade pode já estar
sem folga). Ver [SERVIDORES_E_CANAIS](SERVIDORES_E_CANAIS.md).

O que o servidor decide, e o cliente só desenha: posição, rota (A\*), pulo,
alvo, dano, loot, level. O cliente não tem uma linha de simulação.

Custo medido em produção (VPS de 16 núcleos): 1000 jogadores em 11 canais =
3,6–4,7 núcleos e 12,8 KB/s por jogador. A banda por jogador é limitada pela
**AOI de 60 entidades**, não pela população do canal — é isso que faz o canal
escalar.

## `client` — desenho e mais nada

macroquad/miniquad. Ele gera o volume voxel da ilha em volta do jogador da
mesma semente, faz greedy meshing com oclusão de canto e desenha com culling
de cone (~35 de 143 pedaços por quadro).

Dois limites que custaram caro descobrir:

* a macroquad corta em **5.000 índices por chamada de desenho** (não 10.000
  vértices). O sintoma foi buraco quadriculado no chão, que parece bug de
  malha e não de orçamento;
* um pedaço custa 0,92 ms e 1,08 MB. O raio de carga é 5 pedaços = 131 MB de
  malha e 88 unidades de mundo — e é esse número que limita quanto a câmera
  pode deitar.

## `web` — a porta de entrada

Cadastro, login e o diretório de canais que o cliente lê antes de conectar.
Não fala com o jogo: os dois se encontram no Postgres.

## `panoptico` — o olho de cima

Painel de observabilidade, **só leitura**, em processo separado. Cada canal
publica um retrato já serializado a cada 200 ms; o painel busca e junta. O
tick nunca serve HTTP — aba de navegador não compete com a simulação.

Ele gera o mapa da ilha do mesmo `shared::terreno`, então não existe imagem
guardada pra desincronizar. Exige token e não sobe sem um.

## O que trafega

Nada de terreno. Nada de vegetação. Nada de altura.

O cliente recebe `EntityMeta` uma vez (nome, tipo, hp máximo) e `EntityState`
por tick só de quem mudou: 13 bytes com posição em 1/16 de tile, velocidade
saturada em i8 e um byte de bandeiras. A altura o cliente calcula do próprio
campo de altura — é o mesmo do servidor, então não há o que divergir e não há
o que forjar.

Ver [NETWORKING](NETWORKING.md) e [COMBATE_POR_ALVO](COMBATE_POR_ALVO.md).
