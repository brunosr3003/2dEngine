# Sons exclusivos das 12 skills

Cada ID possui arquivos separados de lançamento (`cast`) e impacto (`impact`).
O evento do servidor escolhe a fase; apertar um botão ou receber uma recusa
não toca a habilidade. Sons de menu/HUD não foram alterados.

| ID | Skill | Identidade sonora |
|---|---|---|
| 1 | Investida | Avanço de lâmina e choque metálico |
| 2 | Golpe Largo | Corte amplo e impacto de lâmina |
| 3 | Muralha | Metal pesado e ressonância de escudo |
| 4 | Saque | Lâmina curta e choque seco |
| 5 | Dança | Sequência de três cortes |
| 6 | Vento Cortante | Lâmina com passagem de ar mágico |
| 7 | Tiro Certeiro | Preparação metálica e disparo único |
| 8 | Rajada | Preparação e três disparos em sequência |
| 9 | Barril | Madeira/arremesso e explosão |
| 10 | Bênção | Dois toques mágicos de cura |
| 11 | Aura | Conjuração e ressonância de cura em camadas |
| 12 | Julgamento | Carga mágica e impacto pesado |

## Fontes e licença

Todos os materiais são gravações/efeitos prontos sob **CC0 1.0**:
https://creativecommons.org/publicdomain/zero/1.0/

- StarNinjas: `sword.*.ogg`, `sword_clash.*.ogg` —
  https://opengameart.org/content/20-sword-sound-effects-attacks-and-clashes
- rubberduck: `blade_*`, `metal_*`, `wood_04`, `spell_*`, `spell_fire_*` —
  https://opengameart.org/content/80-cc0-rpg-sfx
- JaggedStone: `magical_1.ogg`, `magical_3.ogg` —
  https://opengameart.org/content/magic-spell-sfx
- Ben Jaszczak, Brian Nelson, Kevin Heras e Matthew Nanney: pistola 1911 —
  https://opengameart.org/content/the-free-firearm-sound-library
  (mesmo recorte descrito em `../combat/README.md`).

Adaptações: corte, combinação de camadas, espaçamento dos disparos/cortes,
mono 44,1 kHz, ganho e fade. A receita exata de cada arquivo está em
`tools/audio/preparar_skills.py`. Nenhum som sintetizado do zero.
