//! O que o dicionário português → inglês tem que respeitar.
//!
//! Este teste é o que impede a classe de erro que NÃO aparece no compilador e
//! só aparece na tela do jogador: verbete com buraco a mais, buraco a menos,
//! buraco renomeado, ou chave repetida (a segunda nunca seria usada).

use shared::idioma::{self, en, Idioma};
use std::collections::HashMap;

/// Todos os pares, com o nome da parte de onde vieram, pra a mensagem de erro
/// dizer onde consertar.
fn todos() -> Vec<(&'static str, &'static str, &'static str)> {
    let nomes = ["cliente", "servidor", "dados", "missoes", "historia"];
    let mut v = Vec::new();
    for (i, parte) in en::PARTES.iter().enumerate() {
        for (pt, en) in parte.iter() {
            v.push((nomes.get(i).copied().unwrap_or("?"), *pt, *en));
        }
    }
    v
}

#[test]
fn nenhuma_chave_repetida() {
    let mut visto: HashMap<&str, &str> = HashMap::new();
    let mut erros = Vec::new();
    for (parte, pt, _) in todos() {
        if let Some(antes) = visto.insert(pt, parte) {
            erros.push(format!("{pt:?} está em {antes} e de novo em {parte}"));
        }
    }
    assert!(erros.is_empty(), "chave repetida (a segunda nunca é usada):\n{}", erros.join("\n"));
}

#[test]
fn os_buracos_do_portugues_batem_com_os_do_ingles() {
    let mut erros = Vec::new();
    for (parte, pt, en) in todos() {
        let a = idioma::buracos(pt);
        let b = idioma::buracos(en);
        if a.len() != b.len() {
            erros.push(format!("[{parte}] {pt:?} tem {} buraco(s), o inglês tem {}", a.len(), b.len()));
            continue;
        }
        // Buraco nomeado tem que existir dos dois lados com o MESMO nome: é o
        // nome que permite trocar a ordem na tradução.
        for nome in a.iter().flatten() {
            if !b.contains(&Some(nome)) {
                erros.push(format!("[{parte}] {pt:?}: o inglês não tem o buraco {{{nome}}}"));
            }
        }
        for nome in b.iter().flatten() {
            if !a.contains(&Some(nome)) {
                erros.push(format!("[{parte}] {en:?}: o buraco {{{nome}}} não existe no português"));
            }
        }
        // Mistura de anônimo com nomeado no mesmo verbete: a substituição
        // usaria ordem pra um e nome pro outro, e a chance de sair trocado é
        // grande. Melhor proibir.
        let anon_pt = a.iter().filter(|n| n.is_none()).count();
        let anon_en = b.iter().filter(|n| n.is_none()).count();
        if anon_pt != anon_en {
            erros.push(format!("[{parte}] {pt:?}: {anon_pt} buraco(s) anônimo(s) no pt, {anon_en} no en"));
        }
    }
    assert!(erros.is_empty(), "{} problema(s) de buraco:\n{}", erros.len(), erros.join("\n"));
}

#[test]
fn nenhuma_traducao_vazia_ou_igual_por_engano() {
    let mut erros = Vec::new();
    for (parte, pt, en) in todos() {
        if en.trim().is_empty() {
            erros.push(format!("[{parte}] {pt:?} traduz pra vazio"));
        }
        // Frase igual nos dois idiomas é legítima ("Katana", "Tier {}"), mas
        // então o verbete não serve pra nada e só custa busca. Só reclama se
        // tiver acento — aí é português que ficou sem traduzir.
        if pt == en && pt.chars().any(|c| "áéíóúâêôãõçÁÉÍÓÚÂÊÔÃÕÇ".contains(c)) {
            erros.push(format!("[{parte}] {pt:?} está igual no inglês, com acento"));
        }
    }
    assert!(erros.is_empty(), "{}", erros.join("\n"));
}

#[test]
fn o_dicionario_traduz_de_ponta_a_ponta() {
    // Não é sobre uma frase: é sobre o caminho inteiro (construir, indexar,
    // achar) funcionar com o dicionário de verdade.
    let (pt, en) = todos()
        .into_iter()
        .find(|(_, pt, _)| idioma::buracos(pt).is_empty())
        .map(|(_, pt, en)| (pt, en))
        .expect("nenhum verbete sem buraco no dicionário");
    assert_eq!(idioma::tr_em(Idioma::En, pt), en);
    assert_eq!(idioma::tr_em(Idioma::Pt, pt), pt);
}

#[test]
fn frase_montada_de_verdade_casa() {
    // Um verbete com buraco, pego do dicionário, preenchido e conferido.
    assert_eq!(idioma::tr_em(Idioma::En, "Nível 12"), "Level 12");
    assert_eq!(idioma::tr_em(Idioma::En, "Requer nível 30"), "Requires level 30");
}

/// A troca de idioma vale pro processo inteiro, na hora.
///
/// Este teste guarda o defeito que a captura de tela pegou: o cliente definia o
/// idioma DEPOIS dos ramos de prévia, e cada prévia termina em `return` — a
/// captura em inglês saía idêntica à em português, byte por byte, e nada
/// acusava. `definir` tem que valer pra quem desenhar em seguida.
#[test]
fn definir_vale_na_hora_e_para_quem_desenhar_depois() {
    idioma::definir(Idioma::Pt);
    assert_eq!(idioma::atual(), Idioma::Pt);
    assert_eq!(idioma::tr("Bolsa"), "Bolsa");

    idioma::definir(Idioma::En);
    assert_eq!(idioma::atual(), Idioma::En);
    assert_eq!(idioma::tr("Bolsa"), "Bag");

    // E volta.
    idioma::definir(Idioma::Pt);
    assert_eq!(idioma::tr("Bolsa"), "Bolsa");
}

/// Verbete que não traduz nada não entra.
///
/// Dois casos, e o segundo é o perigoso:
///
/// - tradução IDÊNTICA ao português: ocupa busca e não muda nada;
/// - modelo cujo texto fixo não tem letra nenhuma (`"{a} {b}"`, `"{q}× {}"`):
///   casa QUALQUER frase com um espaço — ou com um `×` —, e passa na frente do
///   verbete certo. Vinte destes entraram na primeira leva do dicionário, todos
///   com tradução idêntica, e só apareceram quando um teste pediu tradução de
///   uma frase inventada e recebeu uma resposta.
#[test]
fn nenhum_modelo_casa_qualquer_coisa() {
    let mut erros = Vec::new();
    for (parte, pt, _) in todos() {
        if idioma::buracos(pt).is_empty() {
            continue;
        }
        let fixo: String = {
            // Tira os buracos e olha o que sobra de texto fixo.
            let mut fora = String::new();
            let (mut resto, mut dentro) = (pt, false);
            while let Some(i) = resto.find(if dentro { '}' } else { '{' }) {
                if !dentro {
                    fora.push_str(&resto[..i]);
                }
                resto = &resto[i + 1..];
                dentro = !dentro;
            }
            if !dentro {
                fora.push_str(resto);
            }
            fora
        };
        if !fixo.chars().any(char::is_alphabetic) {
            erros.push(format!("[{parte}] {pt:?}: o texto fixo {fixo:?} não tem letra nenhuma"));
        }
    }
    assert!(erros.is_empty(), "{} modelo(s) que casam qualquer frase:\n{}", erros.len(), erros.join("\n"));
}

/// Buracos colados (`"…{}{}"`) não entram no dicionário.
///
/// Entre dois buracos vizinhos o pedaço fixo é vazio, e não existe resposta
/// certa pra onde um termina e o outro começa — o matcher recusa. Sem este
/// teste, a recusa é SILENCIOSA: o verbete fica no arquivo, parece traduzido na
/// revisão, e a frase sai em português no jogo. Foi assim que
/// "Grátis hoje {}/{}  ·  Passes {}{}" passou pela revisão e apareceu em
/// português numa captura de tela da Ilha Mágica.
///
/// A saída é escrever um verbete por forma que a frase tem no ar.
#[test]
fn nenhum_verbete_tem_buracos_colados() {
    let mut erros = Vec::new();
    for (parte, pt, en) in todos() {
        for (lado, frase) in [("pt", pt), ("en", en)] {
            // Dois buracos colados aparecem como "}{" no texto.
            if frase.contains("}{") {
                erros.push(format!("[{parte}] ({lado}) {frase:?}"));
            }
        }
    }
    assert!(
        erros.is_empty(),
        "{} verbete(s) com buracos colados — separe em uma forma por frase real:\n{}",
        erros.len(),
        erros.join("\n")
    );
}
