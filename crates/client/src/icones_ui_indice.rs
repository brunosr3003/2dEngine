// GERADO por tools/icones/gerar_icones_ui.py — nao edite a mao.
// Rode `python3 tools/icones/gerar_icones_ui.py` pra refazer atlas e indice.

pub const LADO_UI: u32 = 64;
pub const COLUNAS_UI: u32 = 8;
pub const LARGURA_UI: u32 = 512;
pub const ALTURA_UI: u32 = 320;

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
    ("clan", 10),
    ("coleta", 11),
    ("configuracoes", 12),
    ("conquistas", 13),
    ("coroa", 14),
    ("correio", 15),
    ("craft", 16),
    ("diarias", 17),
    ("encantar", 18),
    ("engrenagem", 19),
    ("fechar", 20),
    ("ficha", 21),
    ("forja", 22),
    ("grupo", 23),
    ("habilidades", 24),
    ("loja_tp", 25),
    ("lojas", 26),
    ("mais", 27),
    ("mapa", 28),
    ("menu", 29),
    ("mercado", 30),
    ("missoes", 31),
    ("montaria", 32),
    ("paleta", 33),
    ("pulo", 34),
    ("recuperar_xp", 35),
    ("sair", 36),
    ("todas_missoes", 37),
    ("trocar_personagem", 38),
    ("voltar", 39),
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
