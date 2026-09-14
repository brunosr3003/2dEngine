# Loot dos mobs

Mobs e chefes dão cobre, materiais de craft e um pouco de poção. Não dão
armas, secundárias, armaduras ou acessórios. O cobre substitui o ouro direto
nas oito tabelas básicas; continua sendo o item Cobre usado no craft.

Todas as chances são independentes, por morte. Cobre cai sempre; materiais
coloridos desta tabela vêm na cor cinza. Este é o balanceamento inicial.

| Mob | Cobre (100%) | Materiais (quantidade; chance) | Poções (quantidade; chance) |
|---|---:|---|---|
| Lobo | 4–14 | Couro (1; 4%), Garra (1; 2%) | Vida (1; 8%), Mana (1; 4%) |
| Urso | 15–40 | Aço (1–3; 25%), Couro (1; 4%) | Vida (1; 12%) |
| Pistoleiro | 10–28 | Aço (1–3; 25%), Pedra Sombra-da-Lua (1–2; 12%) | Vigor (1; 8%) |
| Tigre | 8–22 | Quintessência (1–2; 20%), Garra (1; 3%) | Mana (1; 8%) |
| Mago | 15–35 | Pedra do Coração Negro (1–2; 20%), Pedra de Ânima (1–2; 20%) | Mana (1; 12%) |
| Owlbear | 25–60 | Platina (1–3; 25%), Berloque de Exorcismo (1–2; 15%), Chifre (1; 3%) | Vida maior (1; 10%) |
| Arqueiro | 10–28 | Fragmento Iluminante (1–2; 20%), Aço (1–2; 20%) | Vigor (1; 8%) |
| Chefe / Lobo Grande | 200–500 | Darksteel (5–12; 50%), Pó Cintilante (1; 5%), Escama (1; 5%) | Vida maior (1–2; 35%), Mana maior (1; 25%) |
| Caranguejo | 3–10 | Garra (1; 6%), Escama (1; 1%) | Vida (1; 8%) |
| Caranguejo-rei | 12–30 | Garra (1–2; 15%), Aço (1–2; 15%), Escama (1; 3%) | Vida (1; 12%) |

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
