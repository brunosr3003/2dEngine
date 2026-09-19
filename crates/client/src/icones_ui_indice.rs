// GERADO por tools/icones/gerar_icones_ui.py — nao edite a mao.
// Rode `python3 tools/icones/gerar_icones_ui.py` pra refazer atlas e indice.

pub const LADO_UI: u32 = 64;
pub const COLUNAS_UI: u32 = 8;
pub const LARGURA_UI: u32 = 512;
pub const ALTURA_UI: u32 = 384;

pub const LADO_MAPA: u32 = 48;
pub const COLUNAS_MAPA: u32 = 8;
pub const LARGURA_MAPA: u32 = 384;
pub const ALTURA_MAPA: u32 = 96;

pub const LADO_SKILLS: u32 = 96;
pub const COLUNAS_SKILLS: u32 = 6;
pub const LARGURA_SKILLS: u32 = 576;
pub const ALTURA_SKILLS: u32 = 192;

pub const LADO_LOJA: u32 = 128;
pub const COLUNAS_LOJA: u32 = 4;
pub const LARGURA_LOJA: u32 = 512;
pub const ALTURA_LOJA: u32 = 256;

/// (nome, celula), ordenado por nome.
pub const UI: &[(&str, u16)] = &[
    ("amigos", 0),
    ("atacar", 1),
    ("auto_coleta", 2),
    ("auto_combate", 3),
    ("aventuras", 4),
    ("avisos", 5),
    ("banco", 6),
    ("barra_itens", 7),
    ("bolsa", 8),
    ("cadeado", 9),
    ("caneca", 10),
    ("clan", 11),
    ("coleta", 12),
    ("configuracoes", 13),
    ("conquistas", 14),
    ("coroa", 15),
    ("correio", 16),
    ("craft", 17),
    ("diarias", 18),
    ("encantar", 19),
    ("engrenagem", 20),
    ("fechar", 21),
    ("ficha", 22),
    ("forja", 23),
    ("grupo", 24),
    ("habilidades", 25),
    ("loja_tp", 26),
    ("lojas", 27),
    ("mais", 28),
    ("mapa", 29),
    ("menu", 30),
    ("mercado", 31),
    ("missoes", 32),
    ("montaria", 33),
    ("paleta", 34),
    ("pulo", 35),
    ("recuperar_xp", 36),
    ("sair", 37),
    ("todas_missoes", 38),
    ("trocar_personagem", 39),
    ("voltar", 40),
];

/// (nome, celula), ordenado por nome.
pub const MAPA: &[(&str, u16)] = &[
    ("arqueiro", 0),
    ("caranguejo", 1),
    ("chefe", 2),
    ("cidade", 3),
    ("destino", 4),
    ("jogador", 5),
    ("lobo", 6),
    ("madeira", 7),
    ("mago", 8),
    ("npc", 9),
    ("owlbear", 10),
    ("pedra", 11),
    ("pistoleiro", 12),
    ("porto", 13),
    ("tigre", 14),
    ("urso", 15),
];

/// (id da skill, celula), ordenado por id.
pub const SKILLS: &[(u32, u16)] = &[
    (1, 0),
    (2, 1),
    (3, 2),
    (4, 3),
    (5, 4),
    (6, 5),
    (7, 6),
    (8, 7),
    (9, 8),
    (10, 9),
    (11, 10),
    (12, 11),
];

/// (nome, celula), ordenado por nome. Arte colorida da loja (TP, pacotes, ouro).
pub const LOJA: &[(&str, u16)] = &[
    ("ouro", 0),
    ("tp", 1),
    ("tp_1", 2),
    ("tp_2", 3),
    ("tp_3", 4),
    ("tp_4", 5),
];
