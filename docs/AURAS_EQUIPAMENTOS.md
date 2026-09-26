# Auras por equipamento

As peças equipadas têm efeitos independentes: verde, azul, roxo e laranja.
Cinza não emite aura. O grau vem da instância do item, nunca do tier ou do
visual cosmético. Arma e escudo seguem suas malhas; armadura acompanha o
tronco; brinco a cabeça; colar o peito; braceletes os pulsos; cinto a cintura.
Armas mágicas sem malha emitem nas palmas. Não há aura nas ferramentas.

## Tier: principal fator visual

- T1: uma camada, brilho discreto, uma partícula.
- T2: duas camadas, volume maior, três partículas.
- T3: três camadas, seis partículas e rastro de arma ao atacar.
- T4: quatro camadas, dez partículas, volume e brilho mais fortes.

Intensidade base por tier: 0,65 / 1,15 / 1,8 / 2,6. A cor depende somente
da raridade. Refino é secundário: +0..4 ×1; +5..6 ×1,08; +7..9 ×1,15;
+10 ou mais ×1,22. Cada tier sem refino supera a intensidade máxima do
anterior. +5 e +10 acrescentam uma partícula cada. O tier também aumenta
largura e volume; o refino acrescenta apenas uma pequena margem.

## Rede e desempenho

`shared::auras` empacota sete slots em um u64 na EntityMeta (protocolo 144): por byte,
3 bits de grau, 2 de tier e 2 de patamar do refino.
O servidor deriva do equipamento e reenvia a meta aos observadores quando
muda, incluindo retirada e refinamento. Não altera combate ou atributos.
O cliente usa as matrizes animadas, inclusive montado, e omite auras de mortos.
O desenho é emissivo e respeita a profundidade do corpo, das armas e do cenário.
No backend OpenGL da miniquad, `depth_write` também controla a ativação do
teste de profundidade; o material das auras o mantém ativo para não aparecer
através de personagens. Fragmentos de alfa zero são descartados.
Além de 18 unidades reduz segmentos e remove partículas; além de 38 omite.
Orçamento global de 96 efeitos por quadro, priorizando o próprio jogador e
os próximos; lotes limitados para não exceder os índices da macroquad.

Prévia local: `MMO_PREVIA_AURAS=1 target/debug/client`, exporta
`/tmp/tempest-auras.png`. Não modifica personagens nem inventário.
