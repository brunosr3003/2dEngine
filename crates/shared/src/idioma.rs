//! O idioma do jogo, e a tradução do português para o inglês.
//!
//! O jogo nasceu em português com o texto escrito DIRETO no código — rótulo de
//! HUD, nome de item, fala de NPC, aviso do servidor, tudo literal no meio da
//! chamada que desenha. São ~2.900 frases espalhadas por 162 arquivos.
//!
//! O jeito de sempre (trocar cada literal por uma chave, `t!("hud.bolsa")`)
//! custaria 726 pontos de chamada reescritos, e cada reescrita é uma chance de
//! mexer na lógica sem querer. Aqui o caminho é outro: **a chave é a própria
//! frase em português**. O código não muda; quem traduz é o desenho.
//!
//! Isso só é barato porque TODO texto do cliente passa por um gargalo — as
//! funções de `hud_estilo` (`texto`, `texto_centro`, `medir`…). Traduzir lá
//! dentro pega de uma vez o rótulo estático, o nome de item que veio do banco
//! e o aviso que o servidor mandou, sem tocar no protocolo e sem coluna nova.
//!
//! Três consequências que valem entender:
//!
//! - **A lógica continua em português.** A tradução acontece no último
//!   instante, ao desenhar e ao medir. Nada compara, salva ou envia inglês, e
//!   por isso nenhuma comparação de nome quebra.
//! - **Frase sem tradução aparece em português.** Não há `panic!` nem
//!   `"???"`: falta de verbete degrada pra língua original, que é legível.
//! - **Frase montada também traduz.** `format!("Nível {n} de {t}")` chega aqui
//!   já preenchida ("Nível 5 de 12"), então além do dicionário exato existem
//!   os MODELOS: o verbete guarda os pedaços fixos e casa o que sobrou.
//!
//! O dicionário mora em [`en`] (gerado e revisado à mão). Quem acrescenta
//! frase nova no jogo acrescenta o par aqui — e o teste `toda_frase_do_jogo_tem_verbete`
//! em `crates/shared/tests/` cobra os que faltam.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

pub mod pt;

/// As línguas que o jogo fala.
///
/// `En` is the default and the ORIGINAL: with `En` the translation is the
/// identity and costs nothing (the dictionary is not even built).
///
/// It was the other way round until the inversion: the source was Portuguese
/// and English was the translation. What moved was the source, not the
/// mechanism — `tr` is still "the phrase in the code, looked up for the
/// language in use".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Idioma {
    #[default]
    En,
    Pt,
}

impl Idioma {
    /// O código curto que vai pro arquivo de preferências e pro `Accept-Language`.
    pub fn codigo(self) -> &'static str {
        match self {
            Self::Pt => "pt",
            Self::En => "en",
        }
    }

    /// O nome da língua NA PRÓPRIA LÍNGUA — quem procura inglês numa tela em
    /// português procura por "English", não por "Inglês".
    pub fn nome(self) -> &'static str {
        match self {
            Self::Pt => "Português",
            Self::En => "English",
        }
    }

    /// Aceita `pt`, `pt-BR`, `en`, `en-US`… Qualquer outra coisa é `None`, e
    /// quem chama decide o padrão.
    pub fn do_codigo(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        let base = s.split(['-', '_', ';', ',']).next().unwrap_or("");
        match base {
            "pt" => Some(Self::Pt),
            "en" => Some(Self::En),
            _ => None,
        }
    }

    /// A próxima da roda, pro botão que alterna sem abrir lista.
    pub fn proxima(self) -> Self {
        match self {
            Self::Pt => Self::En,
            Self::En => Self::Pt,
        }
    }

    /// Todas, na ordem em que aparecem na tela de opções.
    pub const TODAS: &'static [Idioma] = &[Idioma::Pt, Idioma::En];
}

// ───────────────────────────── o idioma corrente ─────────────────────────────

/// Um átomo, e não um `RwLock`: isto é lido uma vez por texto desenhado, ou
/// seja centenas de vezes por quadro. Escrever é raro (o jogador mexeu nas
/// opções) e ler tem que ser livre de trava.
static ATUAL: AtomicU8 = AtomicU8::new(0);

pub fn atual() -> Idioma {
    match ATUAL.load(Ordering::Relaxed) {
        1 => Idioma::Pt,
        _ => Idioma::En,
    }
}

pub fn definir(i: Idioma) {
    // 0 is the default, and the default is English: a process that never
    // calls `definir` must not start in Portuguese.
    ATUAL.store(
        match i {
            Idioma::En => 0,
            Idioma::Pt => 1,
        },
        Ordering::Relaxed,
    );
}

// ───────────────────────────────── traduzir ─────────────────────────────────

/// Traduz `s` pro idioma corrente.
///
/// `Cow` porque o caso comum não aloca: em português devolve o que recebeu, e
/// em inglês o verbete exato é `&'static str`. Só a frase MONTADA (a que casou
/// por modelo) precisa de `String`, e essa é a minoria.
pub fn tr(s: &str) -> Cow<'_, str> {
    tr_em(atual(), s)
}

/// A mesma tradução, com o idioma dito na mão.
///
/// Existe porque o `web` manda e-mail: lá não há "idioma corrente" do processo,
/// há o idioma DAQUELA conta. Um global ali seria uma corrida entre dois
/// cadastros simultâneos.
pub fn tr_em(idioma: Idioma, s: &str) -> Cow<'_, str> {
    match idioma {
        Idioma::En => Cow::Borrowed(s),
        Idioma::Pt => match dicionario().traduz(s) {
            Achado::Exato(v) => Cow::Borrowed(v),
            Achado::Montado(v) => Cow::Owned(v),
            Achado::Nada => Cow::Borrowed(s),
        },
    }
}

/// A FEMININE form, for a word English has only one of.
///
/// Portuguese agrees in gender; English does not. `chaves::nome_da_cor`
/// describes a *chave* (feminine) and `forja::Grau` describes an *item*
/// (masculine), and both are the same word in English — "Epic", "Legendary",
/// "Purple". Once English is the source, the Portuguese form cannot be
/// recovered from the word alone: `tr("Epic")` has no way to know whether the
/// noun beside it is feminine.
///
/// So the call site says. A place that describes a feminine noun asks for the
/// feminine form, and English ignores the request — the table is only
/// consulted in Portuguese, and anything missing from it falls through to the
/// ordinary dictionary.
///
/// This is deliberately a short table and not a general gender system. Only
/// the grade words hit the problem, because only they are pinned to a noun
/// that varies; the rest of the collapses (Todas/Todos, seu/sua) are labels
/// where one form reads fine.
static FEMININO: &[(&str, &str)] = &[
    ("Purple", "Roxa"),
    ("Epic", "Épica"),
    ("Legendary", "Lendária"),
    ("Rare", "Rara"),
    ("Fine", "Fina"),
];

/// `tr`, asking for the feminine form where Portuguese has one.
///
/// Safe to use before the locale inversion as well as after it: while the
/// source is still Portuguese the lookup misses (the table is keyed by the
/// English word) and it behaves exactly like `tr`.
pub fn tr_f(s: &str) -> Cow<'_, str> {
    tr_f_em(atual(), s)
}

/// The same, with the language said by hand.
///
/// It exists for the same reason `tr_em` does — the `web` process has no
/// "current language", it has the language of that account — and for one
/// more: the current language is a process global, so a test that sets it
/// races every other test in the binary. Asking explicitly is the only way
/// to pin the behaviour of both languages in one test.
pub fn tr_f_em(idioma: Idioma, s: &str) -> Cow<'_, str> {
    if idioma == Idioma::Pt {
        if let Some((_, f)) = FEMININO.iter().find(|(en, _)| *en == s) {
            return Cow::Borrowed(f);
        }
    }
    tr_em(idioma, s)
}

/// O separador de MILHAR e o DECIMAL do idioma em uso, nesta ordem.
///
/// Os dois andam juntos e por isso saem da mesma funcao: pt escreve 1.234,56 e
/// en escreve 1,234.56. Trocar so' um foi exatamente como nasceu o
/// `R$ 1.234.56` que `loja::preco_brl` conta — o mesmo sinal valendo milhar e
/// centavo na mesma linha.
pub fn separadores() -> (char, char) {
    separadores_em(atual())
}

/// O mesmo, com o idioma dito na mao.
///
/// Existe pelo motivo que `tr_em` existe: o idioma atual e' um global do
/// processo, e um teste que mexe nele corre junto com todos os outros do
/// binario. Perguntar explicitamente e' o unico jeito de prender os dois
/// idiomas num teste so'.
pub fn separadores_em(idioma: Idioma) -> (char, char) {
    match idioma {
        Idioma::Pt => ('.', ','),
        Idioma::En => (',', '.'),
    }
}

/// 1234567 -> "1.234.567" em pt, "1,234,567" em en.
///
/// Nao e' frase de dicionario: o separador mora DENTRO do numero, e verbete
/// nenhum alcanca ele.
pub fn milhar(v: u64) -> String {
    milhar_em(atual(), v)
}

/// O mesmo, com o idioma dito na mao. Ver `separadores_em`.
pub fn milhar_em(idioma: Idioma, v: u64) -> String {
    let (milhar, _) = separadores_em(idioma);
    let mut r = v.to_string();
    let mut i = r.len() as i32 - 3;
    while i > 0 {
        r.insert(i as usize, milhar);
        i -= 3;
    }
    r
}

/// Traduz só se houver verbete, e diz quando não houve.
///
/// É o que a varredura de largura e os testes usam pra separar "está em
/// português porque o jogador quis" de "está em português porque falta
/// verbete".
pub fn tr_estrito(idioma: Idioma, s: &str) -> Option<String> {
    if idioma == Idioma::En {
        return Some(s.to_string());
    }
    match dicionario().traduz(s) {
        Achado::Exato(v) => Some(v.to_string()),
        Achado::Montado(v) => Some(v),
        Achado::Nada => None,
    }
}

enum Achado {
    Exato(&'static str),
    Montado(String),
    Nada,
}

// ─────────────────────────────── o dicionário ───────────────────────────────

/// Construído uma vez, na primeira frase traduzida, e nunca mais tocado —
/// depois disso é só leitura, sem trava.
static DICIONARIO: OnceLock<Dicionario> = OnceLock::new();

fn dicionario() -> &'static Dicionario {
    DICIONARIO.get_or_init(|| Dicionario::das_partes(pt::PARTES))
}

pub struct Dicionario {
    /// Frase inteira → frase inteira. O caminho de todo dia.
    exatos: HashMap<&'static str, &'static str>,
    /// Modelos agrupados pelo PRIMEIRO PEDAÇO FIXO.
    ///
    /// Sem esse índice, casar uma frase montada custaria varrer os ~630
    /// modelos. Com ele, a busca olha só os modelos que começam com o mesmo
    /// texto da entrada — quase sempre nenhum ou um.
    por_inicio: HashMap<&'static str, Vec<Modelo>>,
    /// Os modelos que COMEÇAM com buraco ("{} entrou no grupo"), que não têm
    /// início fixo pra indexar. São poucos, e esses sim são varridos.
    sem_inicio: Vec<Modelo>,
}

/// Um verbete com buraco: `"Nível {n} de {t}"` → `"Level {n} of {t}"`.
#[derive(Clone, Debug)]
struct Modelo {
    /// Os trechos fixos do português, na ordem. `"a {x} b {y}"` dá
    /// `["a ", " b ", ""]` — sempre um a mais que o número de buracos.
    pedacos: Vec<&'static str>,
    /// Os nomes dos buracos do português, na ordem em que aparecem. Buraco sem
    /// nome (`{}`, `{:.1}`) entra como `None`.
    nomes: Vec<Option<&'static str>>,
    /// O inglês, já partido do mesmo jeito. Partir na construção e não a cada
    /// quadro: isto roda dentro do laço de desenho.
    saida_pedacos: Vec<&'static str>,
    saida_nomes: Vec<Option<&'static str>>,
}

impl Dicionario {
    /// Junta as partes do dicionário numa só. Uma vez por processo.
    pub fn das_partes(partes: &'static [&'static [(&'static str, &'static str)]]) -> Self {
        Self::novo_de(partes.iter().flat_map(|p| p.iter()))
    }

    pub fn novo(verbetes: &'static [(&'static str, &'static str)]) -> Self {
        Self::novo_de(verbetes.iter())
    }

    fn novo_de(
        verbetes: impl Iterator<Item = &'static (&'static str, &'static str)>,
    ) -> Self {
        let mut exatos = HashMap::new();
        let mut por_inicio: HashMap<&'static str, Vec<Modelo>> = HashMap::new();
        let mut sem_inicio = Vec::new();

        for (pt, en) in verbetes {
            let (pedacos, nomes) = parte(pt);
            if nomes.is_empty() {
                exatos.insert(*pt, *en);
                continue;
            }
            // MODELO SEM TEXTO QUE DISCRIMINE NÃO ENTRA.
            //
            // `"{a} {b}"` tem como texto fixo um espaço, e casa qualquer frase
            // do mundo que tenha um espaço — engolindo a busca de quem viria
            // depois. Modelos assim sempre nasceram de tradução IDÊNTICA
            // (`"{q}× {}"` → `"{q}× {}"`), ou seja, não traduzem nada: o custo
            // é todo risco e nenhum ganho.
            //
            // Recusar aqui, e não só no teste, porque o dicionário também é
            // construído em `Dicionario::novo` por quem estiver testando.
            if !pedacos.iter().any(|p| p.chars().any(char::is_alphabetic)) {
                continue;
            }
            // BURACOS COLADOS (`"{}{}"`) NÃO ENTRAM.
            //
            // Entre eles o pedaço fixo é vazio, e não existe resposta certa
            // pra onde um termina e o outro começa. O matcher recusava em
            // silêncio: o verbete ficava no dicionário sem nunca casar, e a
            // frase saía em português sem nada acusar. Melhor recusar aqui,
            // onde o teste vê.
            if pedacos[1..pedacos.len().saturating_sub(1)]
                .iter()
                .any(|p| p.is_empty())
            {
                continue;
            }
            let (saida_pedacos, saida_nomes) = parte(en);
            let m = Modelo {
                pedacos,
                nomes,
                saida_pedacos,
                saida_nomes,
            };
            match m.pedacos.first().copied().filter(|p| !p.is_empty()) {
                Some(inicio) => por_inicio.entry(inicio).or_default().push(m),
                None => sem_inicio.push(m),
            }
        }
        // Modelo mais específico (mais texto fixo) primeiro: "Nível {} de {}"
        // tem que ser tentado antes de "Nível {}", senão o segundo casaria a
        // frase inteira e engoliria " de 12" dentro do buraco.
        for v in por_inicio.values_mut() {
            v.sort_by_key(|m| std::cmp::Reverse(m.fixo()));
        }
        sem_inicio.sort_by_key(|m| std::cmp::Reverse(m.fixo()));
        Self {
            exatos,
            por_inicio,
            sem_inicio,
        }
    }

    fn traduz(&self, s: &str) -> Achado {
        if let Some(v) = self.exatos.get(s) {
            return Achado::Exato(v);
        }
        // Índice por início: o maior prefixo da entrada que seja chave. Testar
        // todos os tamanhos seria O(n) em bytes; na prática os inícios são
        // curtos, então vai do maior pro menor até achar um grupo.
        for fim in (1..=s.len()).rev() {
            if !s.is_char_boundary(fim) {
                continue;
            }
            if let Some(grupo) = self.por_inicio.get(&s[..fim]) {
                for m in grupo {
                    if let Some(v) = m.casa(s, self) {
                        return Achado::Montado(v);
                    }
                }
            }
        }
        for m in &self.sem_inicio {
            if let Some(v) = m.casa(s, self) {
                return Achado::Montado(v);
            }
        }
        Achado::Nada
    }

    /// A tradução de um pedaço capturado: só verbete EXATO.
    ///
    /// Nunca outro modelo. Modelo dentro de modelo é caminho pra recursão sem
    /// fim, e o ganho real está nos nomes — ilha, item, zona —, que são
    /// verbetes exatos.
    fn traduz_valor(&self, v: &str) -> Option<&'static str> {
        self.exatos.get(v).copied()
    }

    /// Quantos verbetes exatos e quantos modelos — o teste e a varredura
    /// querem saber.
    pub fn tamanho(&self) -> (usize, usize) {
        (
            self.exatos.len(),
            self.por_inicio.values().map(Vec::len).sum::<usize>() + self.sem_inicio.len(),
        )
    }
}

impl Modelo {
    /// Quanto texto fixo o modelo exige. Serve de medida de especificidade.
    fn fixo(&self) -> usize {
        self.pedacos.iter().map(|p| p.len()).sum()
    }

    /// Casa a entrada inteira contra os pedaços fixos e devolve o inglês
    /// preenchido. `None` se não casou.
    ///
    /// A varredura é da esquerda pra direita e pega a PRIMEIRA ocorrência de
    /// cada pedaço seguinte — o buraco fica o menor possível. Com o último
    /// pedaço a exigência é diferente: ele tem que FECHAR a frase, senão
    /// "Nível 5" casaria com "Nível 5 de 12" e sobraria texto por traduzir.
    fn casa(&self, s: &str, dic: &Dicionario) -> Option<String> {
        let primeiro = self.pedacos[0];
        let mut resto = s.strip_prefix(primeiro)?;
        let mut valores: Vec<&str> = Vec::with_capacity(self.nomes.len());

        for (i, pedaco) in self.pedacos[1..].iter().enumerate() {
            let ultimo = i + 2 == self.pedacos.len();
            if ultimo {
                if pedaco.is_empty() {
                    // Buraco no fim: leva tudo o que sobrou. Vazio não serve —
                    // "Nível " cru não é "Nível {n}".
                    if resto.is_empty() {
                        return None;
                    }
                    valores.push(resto);
                    resto = "";
                } else {
                    let corte = resto.strip_suffix(pedaco)?;
                    if corte.is_empty() {
                        return None;
                    }
                    valores.push(corte);
                    resto = "";
                }
            } else {
                let em = resto.find(pedaco)?;
                if em == 0 {
                    return None; // buraco vazio no meio
                }
                valores.push(&resto[..em]);
                resto = &resto[em + pedaco.len()..];
            }
        }
        if !resto.is_empty() {
            return None;
        }
        Some(self.preenche(&valores, dic))
    }

    /// Põe os valores de volta nos buracos do inglês.
    ///
    /// Pelo NOME quando o buraco tem nome, e por ORDEM quando não tem. É por
    /// isso que o verbete que troca a ordem das partes na tradução é obrigado
    /// a nomear os buracos: `"{a} de {b}"` → `"{b}'s {a}"` funciona,
    /// `"{} de {}"` → `"{}'s {}"` sairia trocado.
    fn preenche(&self, valores: &[&str], dic: &Dicionario) -> String {
        let pedacos_en = &self.saida_pedacos;
        let nomes_en = &self.saida_nomes;
        let mut fora = String::with_capacity(
            pedacos_en.iter().map(|p| p.len()).sum::<usize>() + 16,
        );
        for (i, pedaco) in pedacos_en.iter().enumerate() {
            fora.push_str(pedaco);
            if i >= nomes_en.len() {
                continue;
            }
            let valor = match nomes_en[i] {
                Some(nome) => self
                    .nomes
                    .iter()
                    .position(|n| *n == Some(nome))
                    .and_then(|k| valores.get(k)),
                None => valores.get(i),
            };
            // Buraco do inglês sem par no português: devolve o buraco cru em
            // vez de comer o valor. Aparece na tela, e aparecer é o que faz
            // alguém consertar o verbete.
            match valor {
                // O VALOR TAMBEM TRADUZ.
                //
                // "Ilha Mágica I · mobs nv 20-23 · poder 2.708" casa o modelo
                // e sai "Ilha Mágica I · mobs lv 20-23 · power 2.708": metade
                // em cada língua, porque o pedaço capturado voltava cru. Nome
                // de ilha, de item e de zona quase sempre chegam assim, por
                // dentro de uma frase montada.
                //
                // Uma volta só, e nunca em número puro: o que vem no buraco é
                // muitas vezes "12" ou "2.708", e procurar verbete pra isso é
                // só busca perdida.
                Some(v) if v.chars().any(char::is_alphabetic) => {
                    match dic.traduz_valor(v) {
                        Some(t) => fora.push_str(t),
                        None => fora.push_str(v),
                    }
                }
                Some(v) => fora.push_str(v),
                None => {
                    fora.push('{');
                    if let Some(n) = nomes_en[i] {
                        fora.push_str(n);
                    }
                    fora.push('}');
                }
            }
        }
        fora
    }
}

/// Os buracos de um modelo, na ordem, com o nome de cada um (`None` quando é
/// anônimo).
///
/// Público porque o teste do dicionário cobra a paridade entre o português e o
/// inglês de cada verbete — e cobrar isso de fora exige ver os buracos.
pub fn buracos(modelo: &str) -> Vec<Option<&str>> {
    parte(modelo).1
}

/// Parte um modelo nos pedaços fixos e nos nomes dos buracos.
///
/// Entende o que o `format!` entende e o jogo usa: `{}`, `{nome}`, `{0}`,
/// `{:.1}`, `{nome:>8}`. `{{` e `}}` são chave escapada, e não buraco.
fn parte(modelo: &str) -> (Vec<&str>, Vec<Option<&str>>) {
    let mut pedacos = Vec::new();
    let mut nomes = Vec::new();
    let bytes = modelo.as_bytes();
    let mut inicio = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if bytes.get(i + 1) == Some(&b'{') {
                i += 2;
                continue;
            }
            let Some(fim) = modelo[i..].find('}').map(|k| i + k) else {
                break;
            };
            pedacos.push(&modelo[inicio..i]);
            let dentro = &modelo[i + 1..fim];
            let nome = dentro.split(':').next().unwrap_or("");
            nomes.push(if nome.is_empty() { None } else { Some(nome) });
            i = fim + 1;
            inicio = i;
        } else {
            i += 1;
        }
    }
    pedacos.push(&modelo[inicio..]);
    (pedacos, nomes)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn dic(v: &'static [(&'static str, &'static str)]) -> Dicionario {
        Dicionario::novo(v)
    }

    #[test]
    fn portugues_e_identidade() {
        assert_eq!(tr_em(Idioma::Pt, "Bolsa"), "Bolsa");
        // Nem passa pelo dicionário: frase que não existe sai igual.
        assert_eq!(tr_em(Idioma::Pt, "frase que ninguém escreveu"), "frase que ninguém escreveu");
    }

    /// OS DOIS SEPARADORES ANDAM JUNTOS, e cada idioma tem o seu.
    ///
    /// `bolsa::milhar` e `morte::milhar` tinham cada um a sua copia com o ponto
    /// FIXO, entao o ingles mostrava "power 2.708" — dois inteiros e sete
    /// decimos, onde o jogo queria dizer dois mil setecentos e oito. Os testes
    /// daquelas copias chegaram a cravar `"Recover XP · 1.200 gold"`: frase em
    /// ingles com pontuacao portuguesa, o defeito escrito como se fosse regra.
    ///
    /// Pergunta pelos dois idiomas de proposito: o idioma atual e' um global do
    /// processo, e um teste que o troca corre junto com o resto do binario.
    #[test]
    fn cada_idioma_pontua_o_numero_do_seu_jeito() {
        assert_eq!(milhar_em(Idioma::En, 2708), "2,708");
        assert_eq!(milhar_em(Idioma::Pt, 2708), "2.708");
        assert_eq!(milhar_em(Idioma::En, 1_234_567), "1,234,567");
        assert_eq!(milhar_em(Idioma::Pt, 1_234_567), "1.234.567");
        // Curto demais pra separador nenhum: os dois dizem a mesma coisa.
        for i in [Idioma::En, Idioma::Pt] {
            assert_eq!(milhar_em(i, 12), "12");
            assert_eq!(milhar_em(i, 999), "999");
        }
        // O decimal e' o OUTRO em cada idioma — trocar so' um foi como nasceu
        // o `R$ 1.234.56`, com o mesmo sinal valendo milhar e centavo.
        assert_eq!(separadores_em(Idioma::En), (',', '.'));
        assert_eq!(separadores_em(Idioma::Pt), ('.', ','));
    }

    #[test]
    fn verbete_exato() {
        let d = dic(&[("Bolsa", "Bag")]);
        assert!(matches!(d.traduz("Bolsa"), Achado::Exato("Bag")));
        assert!(matches!(d.traduz("Bolso"), Achado::Nada));
    }

    #[test]
    fn frase_montada_casa_e_preenche() {
        let d = dic(&[("Nível {n}", "Level {n}")]);
        match d.traduz("Nível 12") {
            Achado::Montado(v) => assert_eq!(v, "Level 12"),
            _ => panic!("não casou"),
        }
    }

    #[test]
    fn o_modelo_mais_especifico_ganha() {
        // O curto casaria a frase toda com " de 12" dentro do buraco.
        let d = dic(&[("Nível {n}", "Level {n}"), ("Nível {n} de {t}", "Level {n} of {t}")]);
        match d.traduz("Nível 5 de 12") {
            Achado::Montado(v) => assert_eq!(v, "Level 5 of 12"),
            _ => panic!("não casou"),
        }
    }

    #[test]
    fn buraco_nomeado_pode_trocar_de_ordem() {
        let d = dic(&[("{quem} matou {oque}", "{oque} was killed by {quem}")]);
        match d.traduz("Brunji matou o Lobo") {
            Achado::Montado(v) => assert_eq!(v, "o Lobo was killed by Brunji"),
            _ => panic!("não casou"),
        }
    }

    #[test]
    fn buraco_sem_nome_segue_a_ordem() {
        let d = dic(&[("{} ganhou {} de ouro", "{} earned {} gold")]);
        match d.traduz("Ana ganhou 300 de ouro") {
            Achado::Montado(v) => assert_eq!(v, "Ana earned 300 gold"),
            _ => panic!("não casou"),
        }
    }

    #[test]
    fn modelo_exige_a_frase_inteira() {
        // Sobrou texto no fim: não é este verbete.
        let d = dic(&[("Nível {n} de {t}", "Level {n} of {t}")]);
        assert!(matches!(d.traduz("Nível 5 de 12 e mais coisa aqui"), Achado::Montado(_)));
        let d2 = dic(&[("Você precisa de {n} de ouro", "You need {n} gold")]);
        assert!(matches!(d2.traduz("Você precisa de 30 de ouro"), Achado::Montado(_)));
        assert!(matches!(d2.traduz("Você precisa de ouro"), Achado::Nada));
    }

    #[test]
    fn buraco_vazio_nao_casa() {
        let d = dic(&[("Nível {n}", "Level {n}")]);
        assert!(matches!(d.traduz("Nível "), Achado::Nada));
    }

    #[test]
    fn chave_escapada_nao_e_buraco() {
        let (pedacos, nomes) = parte("use {{chaves}} assim");
        assert!(nomes.is_empty());
        assert_eq!(pedacos, vec!["use {{chaves}} assim"]);
    }

    #[test]
    fn formato_com_precisao_e_um_buraco_sem_nome() {
        let (_, nomes) = parte("{:.0}% de vida");
        assert_eq!(nomes, vec![None]);
        let (_, nomes) = parte("{vida:.1} de {total}");
        assert_eq!(nomes, vec![Some("vida"), Some("total")]);
    }

    /// O que a captura de tela da Ilha Mágica mostrou: o modelo casava, mas o
    /// nome da ilha voltava cru, e a linha saía metade em cada língua.
    #[test]
    fn o_valor_capturado_tambem_traduz() {
        let d = dic(&[
            ("{} · mobs nv {}-{}", "{} · mobs lv {}-{}"),
            ("Ilha Mágica I", "Magic Island I"),
        ]);
        match d.traduz("Ilha Mágica I · mobs nv 20-23") {
            Achado::Montado(v) => assert_eq!(v, "Magic Island I · mobs lv 20-23"),
            _ => panic!("não casou"),
        }
    }

    /// Número no buraco não vira busca, e o que não tem verbete volta como veio.
    #[test]
    fn valor_sem_verbete_volta_igual() {
        let d = dic(&[("Nível {n} de {t}", "Level {n} of {t}")]);
        match d.traduz("Nível 5 de Brunji") {
            Achado::Montado(v) => assert_eq!(v, "Level 5 of Brunji"),
            _ => panic!("não casou"),
        }
    }

    #[test]
    fn sem_verbete_devolve_o_original() {
        // O contrato que segura o jogo: falta de tradução não some com o
        // texto. Depois da inversão o original é o INGLÊS, então quem pode
        // ficar sem verbete é o português.
        assert_eq!(tr_em(Idioma::Pt, "\u{1}frase inexistente\u{1}"), "\u{1}frase inexistente\u{1}");
        assert_eq!(tr_estrito(Idioma::Pt, "\u{1}frase inexistente\u{1}"), None);
        // E o inglês nunca fica sem: ele é a fonte.
        assert_eq!(tr_estrito(Idioma::En, "\u{1}frase inexistente\u{1}"),
                   Some("\u{1}frase inexistente\u{1}".to_string()));
    }

    #[test]
    fn codigo_de_idioma_aceita_regiao() {
        assert_eq!(Idioma::do_codigo("pt-BR"), Some(Idioma::Pt));
        assert_eq!(Idioma::do_codigo("en_US"), Some(Idioma::En));
        assert_eq!(Idioma::do_codigo("en-GB,en;q=0.9"), Some(Idioma::En));
        assert_eq!(Idioma::do_codigo("fr"), None);
    }

    #[test]
    fn o_dicionario_de_verdade_constroi() {
        let (exatos, modelos) = dicionario().tamanho();
        assert!(exatos > 1500, "poucos verbetes exatos: {exatos}");
        assert!(modelos > 200, "poucos modelos: {modelos}");
    }
}
