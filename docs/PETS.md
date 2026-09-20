# Pets coletores

O saque continua caindo no chão — isso é de propósito. O que muda é quem vai
buscar: o **pet** anda fisicamente até o saquinho e credita no dono ao
encostar. Não há botão de auto-loot nem raio mágico em volta do personagem.

Mistura o que o MIR4 faz com o que o Talisman Online faz: a **cor como grau**
e a evolução por consumo vêm do primeiro; o bicho que anda junto e serve à
classe, do segundo.

## O pet é um item

**Decisão que sustenta o resto**: o pet não é posse de conta como a montaria,
é **item de bolsa**. Por isso ele é **negociável no mercado** e **combinável**
na aba Combinar do Craft, sem sistema novo nenhum.

O grau mora no `item_id`: cada espécie ocupa cinco ids seguidos
(`item_id::pet_no_grau`), a mesma convenção dos materiais coloridos. Equipado
no slot `EquipSlot::Pet`, ele nasce como entidade no mundo.

| espécie | modelo | afinidade | serve a |
|---|---|---|---|
| Lobinho | `lobo_pequeno` | DES 6 · SPD 4 | quem bate rápido |
| Ursinho | `urso` | VIT 6 · RES 4 | quem apanha de frente |
| Filhote de Tigre | `tigre` | FOR 6 · DES 4 | dano corpo a corpo |
| Corujinha-urso | `owlbear` | INT 7 · VIT 3 | arma mágica |
| Caranguejinho | `caranguejo` | RES 7 · VIT 3 | quem bloqueia |

O "combo com a classe" não precisa de regra própria: os pontos do pet entram
como **ponto alocado** (`STAT_POINT_BONUS`), exatamente como os que o jogador
distribui na Ficha. Por isso o INT da corujinha só vira ataque em quem usa
arma mágica — a mesma regra de sempre, sem exceção nova.

## O que o grau muda

| grau | velocidade | busca até | pontos |
|---|---:|---:|---:|
| Cinza | 90% | 8 tiles | 5 |
| Verde | 105% | 10 | 8 |
| Azul | 120% | 12 | 11 |
| Roxo | 135% | 14 | 14 |
| Laranja | 150% | 16 | 17 |

Velocidade é sobre `PLAYER_SPEED`. O laranja empata com `VEL_MONTADO`: só o
topo acompanha quem está montado, o resto fica para trás e é puxado pela
coleira.

Os pontos são a curva de cor que os itens já usam (`items::tier_stat_mult`)
sobre uma base de 9 — não é número novo, é a mesma escada.

## Como ele busca (servidor, `world/pets.rs`)

Três travas que existem por motivo:

- **A busca parte do DONO**, não do pet. Partindo do pet, cada saque pego
  empurraria o raio mais para longe e o bicho nunca voltaria.
- **Nada passa da coleira** (20 tiles). O raio máximo de busca (16) já é menor
  que o `AOI_RADIUS` (24) de propósito: fora da AOI o pet sumiria da tela de
  quem está olhando.
- **Bolsa cheia não vira laço.** O saque que não coube entra numa lista de
  desistência por 5 s; sem isso o pet ficaria batendo no mesmo saquinho para
  sempre.

Entrega e pickup por proximidade passam pelo **mesmo** `creditar_saque`:
equipável vai para o slot vazio, ouro para o saldo, o resto para a bolsa.
A regra não mora em dois lugares.

## Quem matou tem a frente por 2 segundos

`LootTag` guarda quem deu o golpe final. Por `LOOT_PRIORIDADE_S` (2 s) o saque
é **só dele e do pet dele**; passado isso, é de quem chegar.

Dois segundos é o suficiente pra andar até o drop e pegar na mão, e o pet
continua valendo a pena por dois motivos: dentro da janela ele já busca o que
é seu sem você sair do lugar, e fora dela ele alcança o que os outros
deixaram pra trás. Sem a janela, um pet laranja de 16 tiles limpava o drop de
quem matou o bicho antes de o dono dar dois passos.

O pickup por proximidade e o pet perguntam pra **mesma** função
(`LootTag::liberado_para`): a regra não mora em dois lugares.

Saque sem dono — coleta, item largado da bolsa, morte de jogador — nunca tem
janela: é livre desde o primeiro quadro.

## O custo de rede

Este é o ponto caro. `AOI_MAX_ENTIDADES` é 60 e a medição de
`docs/SERVIDORES_E_CANAIS.md` já mostrava ~47 estados por snapshot com o canal
lotado. Um pet por jogador é **+1 entidade móvel por jogador**.

Mitigação: na ordenação da AOI, **pet dos outros leva um empurrão para o fim
da fila** — é a primeira coisa a cair quando o teto enche, porque não muda a
leitura da briga. O pet do próprio dono nunca cai.

O efeito real disso em canal cheio **não foi medido**; precisa de bots na ilha.

## De onde vem

| fonte | o que dá |
|---|---|
| **História principal**, missão 509 (nível 12) | 1 Lobinho Cinza — todo mundo ganha um |
| **Pergaminho de Invocação: Pet**, 250 TP | espécie e grau sorteados |
| **Recompensa diária**, dias 14 e 28 | o pergaminho, **vinculado** |
| **Combinar** | 3 do mesmo grau tentam 1 do grau acima |

Chance do pergaminho: Cinza 55%, Verde 28%, Azul 12%, Roxo 4%, Laranja 1%.

O primeiro pet sai da história porque o auto-loot é **mecânica do jogo**, não
privilégio de quem paga. O pergaminho comprado acelera; não abre a porta.

## Combinar é aposta

3 do mesmo grau tentam 1 do grau de cima, cobrando cobre. **Falhar consome os
três** — mesma regra da chave de craft, e o oposto da evolução de habilidade,
que é determinística.

| para o grau | chance | cobre por tentativa |
|---|---:|---:|
| Verde | 60% | 2.000 |
| Azul | 40% | 8.000 |
| Roxo | 25% | 30.000 |
| Laranja | 10% | 120.000 |

## As duas regras escritas que isto cruzou

Anotado aqui para a documentação não passar a mentir:

1. `docs/MONTARIAS.md` dizia que item de loja de cash **não muda atributo de
   combate**. O pergaminho de pet pode entregar um pet que muda. O que segura
   a regra de pé é que o pergaminho **também cai na diária** e o primeiro pet
   vem da história: o pagante compra velocidade, não acesso.
2. `docs/CALENDARIO.md` dizia "nada de TP, equipamento ou item negociável" na
   recompensa diária. O pergaminho entra **vinculado** — não vai ao mercado.
   O pet que sai dele é negociável, que é justamente o ponto do sistema.

## O que ficou de fora

- **Pet que luta.** O auto combate não desvia porque desviar é a graça; pet
  batendo no seu lugar vai na mesma direção.
- **Nível de pet.** O grau é o único eixo. "Níveis de pet iguais aos itens"
  são as cinco cores, e é o que está implementado.
- **Prazo de validade do saque.** O drop ainda não despawna: a janela de
  prioridade fecha em 2 s, mas o saquinho fica no chão para sempre. Com muito
  jogador, isso é entidade acumulando no mapa.
