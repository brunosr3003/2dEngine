# Loot dos mobs

Mobs e chefes dão cobre, materiais de craft e um pouco de poção. Não dão
armas, secundárias, armaduras ou acessórios. O cobre substitui o ouro direto
nas oito tabelas básicas; continua sendo o item Cobre usado no craft.

Todas as chances são independentes, por morte. Cobre cai sempre; materiais
coloridos desta tabela vêm na cor cinza. Este é o balanceamento inicial.

| Mob | Cobre (100%) | Materiais (quantidade; chance) | Poções (quantidade; chance) |
|---|---:|---|---|
| Lobo | 4–14 | — | Vida (1; 8%), Mana (1; 4%) |
| Urso | 15–40 | Aço (1–3; 25%) | Vida (1; 12%) |
| Pistoleiro | 10–28 | Aço (1–3; 25%), Pedra Sombra-da-Lua (1–2; 12%) | Vigor (1; 8%) |
| Tigre | 8–22 | Quintessência (1–2; 20%) | Mana (1; 8%) |
| Mago | 15–35 | Pedra do Coração Negro (1–2; 20%), Pedra de Ânima (1–2; 20%) | Mana (1; 12%) |
| Owlbear | 25–60 | Platina (1–3; 25%), Berloque de Exorcismo (1–2; 15%) | Vida maior (1; 10%) |
| Arqueiro | 10–28 | Fragmento Iluminante (1–2; 20%), Aço (1–2; 20%) | Vigor (1; 8%) |
| Chefe / Lobo Grande | 200–500 | Darksteel (5–12; 50%), Pó Cintilante (1; 5%) | Vida maior (1–2; 35%), Mana maior (1; 25%) |
| Caranguejo | 3–10 | — | Vida (1; 8%) |
| Caranguejo-rei | 12–30 | Aço (1–2; 15%) | Vida (1; 12%) |

## Chaves de craft: chefes e missões

Escama, Garra, Chifre e Couro (a chave de cada receita) **não caem de mob
comum nem da pedra**. Caem de chefe, na cor da faixa do **conteúdo** e com
chance que cai conforme a faixa sobe (`shared::chaves`). O chefe que nasce no
mundo aberto rende bem menos que o chefe de dungeon/raid:

| nível do chefe | cor | dungeon / raid | chefe do mundo |
|---|---|---:|---:|
| 1–19 | cinza | 15% | 5% |
| 20–39 | verde | 10% | 3% |
| 40–59 | azul | 6% | 2% |
| 60–79 | épica | 3% | 1% |
| 80+ | lendária | 1% | 0,3% |

Cai uma das quatro, sorteada. Missões secundárias da primeira ilha dão as
15 chaves cinzas das receitas de equipamento; duas da Geleira dão chaves verdes
para arma e armadura T2. A chave lendária existe (ids 353–356) mas ainda
não tem de onde cair: nenhum chefe passa do 60 e dungeon/raid não existem.
Missão pode dar chave de recompensa (é de propósito). Bancos antigos perdem as
linhas de chave pela migração `chaves_so_de_chefe_v1`.

Os caranguejos (kinds 8 e 9) só nascem nas **zonas de praia**: chão plano e
baixo de areia com o oceano a poucos passos, fora da cidade e do porto
(`world::sitios_de_praia`, zonas com id a partir de `ZONA_DE_PRAIA_ID`). Um em
cada quatro é rei. As zonas comuns nunca sorteiam caranguejo. O loot deles vai
ao banco pela migração `loot_caranguejos_v1`.

A tabela é por tipo de mob, inclusive quando o nível varia. A coleta de
recursos continua com sua tabela própria em `farm_node_drops`.

Implementação: `loot_mobs::BASE` migra as tabelas básicas uma vez, com
transação e marcador `loot_mobs_recursos_v1`. Reiniciar não recoloca os drops
antigos nem sobrescreve ajustes posteriores de quantidade e chance.
`EconomyConfig::roll_loot` também exclui equipamentos conhecidos pelo jogo
ou classificados como equipáveis no banco; a lista de fontes de recursos
usa o mesmo filtro. Itens desconhecidos ou inativos não saem de mobs.

Validação somente leitura do banco e de 80.000 mortes simuladas:

```sh
cargo run --release --bin audit_mob_loot
```

Requer `DATABASE_URL` no ambiente.
