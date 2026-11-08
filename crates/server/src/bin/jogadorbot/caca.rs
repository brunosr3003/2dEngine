//! Independent hunting routes using only positions and health seen by the player.
use super::*;

#[derive(Default)]
pub struct Caca {
    passo: u32,
    patrulha: Option<(glam::Vec2, Instant)>,
    perseguindo: Option<(shared::EntityId, glam::Vec2, Instant, glam::Vec2)>,
    bloqueados: HashMap<shared::EntityId, Instant>,
}

fn ponto(nome: &str, centro: glam::Vec2, raio: f32, passo: u32) -> glam::Vec2 {
    let s = semente_do_nome(nome);
    let angulo = (s % 100_003) as f32 / 100_003.0 * std::f32::consts::TAU
        + passo as f32 * 2.399_963_2;
    let fracao = 0.30 + ((s / 100_003 + passo as u64) % 5) as f32 * 0.12;
    centro + glam::Vec2::new(angulo.cos(), angulo.sin()) * raio.max(12.0) * fracao
}

pub async fn combate(ws: &mut Ws, eu: &mut Eu, nome: &str, t: &Trilha,
    d: Destino, seq: &mut u32, tick: u32) -> Result<()> {
    let agora = Instant::now();
    eu.caca.bloqueados.retain(|_, ate| *ate > agora);
    let alvo_quest = shared::historia::def_da_historia(d.quest).map_or(0, |q| q.obj_target);
    // Main kill objectives accept their species across the island. Use the
    // player's map to assign different suitable spawn regions to the ten bots.
    let mut zonas: Vec<_> = eu.prog.zonas.iter()
        .filter(|z| !z.forte && z.lv_min as u32 <= eu.nivel + 1
            && z.bichos.iter().any(|(kind, _)| shared::quests::kill_conta(alvo_quest, *kind)))
        .collect();
    zonas.sort_by(|a, b| glam::Vec2::from(a.centro).distance_squared(d.pos)
        .total_cmp(&glam::Vec2::from(b.centro).distance_squared(d.pos)));
    let indice = std::env::var("BOT_AGENT_ID").ok().and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| semente_do_nome(nome) as usize).saturating_sub(1);
    let (centro, raio) = zonas.get(indice % zonas.len().max(1)).map_or((d.pos, d.raio),
        |z| (glam::Vec2::from(z.centro), z.raio));

    let alvo = eu.conhecidos.iter()
        .filter(|(id, tag)| matches!(tag, shared::EntityTag::Enemy)
            && eu.vidas.get(id).copied().unwrap_or(0) > 0
            && !eu.caca.bloqueados.contains_key(id)
            && eu.tipos.get(id).is_some_and(|kind| shared::quests::kill_conta(alvo_quest, *kind)))
        .filter_map(|(id, _)| eu.posicoes.get(id).map(|p| (*id, *p)))
        .filter(|(_, p)| p.distance(centro) <= raio.max(12.0) + 15.0
            && p.distance(eu.pos) <= LONGE_DEMAIS)
        .min_by(|a, b| {
            let score = |(id, p): &(shared::EntityId, glam::Vec2)| {
                let ocupados = eu.conhecidos.iter()
                    .filter(|(outro, tag)| Some(**outro) != eu.entidade && matches!(tag, shared::EntityTag::Player))
                    .filter(|(outro, _)| eu.posicoes.get(outro).is_some_and(|q| q.distance(*p) < 7.0))
                    .count();
                p.distance(eu.pos) + ocupados as f32 * 18.0
                    + ((semente_do_nome(nome) ^ id.0 as u64) % 13) as f32 * 0.25
            };
            score(a).total_cmp(&score(b))
        });
    if let Some((id, p)) = alvo {
        eu.caca.patrulha = None;
        eu.empurrao = None;
        eu.empurrao_ate = None;
        ws.send(envia(&ClientMessage::SetTarget { target: Some(id) })?).await?;
        let alcance = if eu.prog.equip.weapon == Some(402) || eu.prog.equip.weapon == Some(403) { 7.0 } else { 1.8 };
        if eu.pos.distance(p) > alcance {
            if let Some((anterior, _, desde, origem)) = eu.caca.perseguindo {
                if anterior == id && desde.elapsed() > Duration::from_secs(10)
                    && eu.pos.distance(origem) < 1.5 {
                    eu.caca.bloqueados.insert(id, agora + Duration::from_secs(45));
                    eu.caca.perseguindo = None;
                    t.registra(ev(nome, eu, "alvo_inalcancavel", false, format!("mob {}: tentando outro caminho", id.0)));
                    return Ok(());
                }
            }
            if eu.caca.perseguindo.is_none_or(|(anterior, destino, desde, _)| anterior != id
                || (destino.distance(p) > 4.0 && desde.elapsed() > Duration::from_secs(2))) {
                ws.send(envia(&ClientMessage::MoverPara { x: p.x, z: p.y })?).await?;
                eu.caca.perseguindo = Some((id, p, agora, eu.pos));
                t.registra(ev(nome, eu, "perseguiu_mob", true, format!("#{} mob {} em {:.0},{:.0}", d.quest, id.0, p.x, p.y)));
            }
        } else {
            eu.caca.perseguindo = None;
            *seq = seq.wrapping_add(1);
            ws.send(envia(&ClientMessage::Input { input: InputFrame {
                seq: *seq, tick, move_dir: glam::Vec2::ZERO,
                aim: (p - eu.pos).normalize_or_zero(), buttons: 1,
            } })?).await?;
        }
        return Ok(());
    }
    eu.caca.perseguindo = None;
    eu.empurrao = None;
    eu.empurrao_ate = None;
    if eu.caca.patrulha.is_some_and(|(p, desde)| p.distance(eu.pos) > 4.0
        && desde.elapsed() < Duration::from_secs(35)) { return Ok(()); }
    eu.caca.passo = eu.caca.passo.wrapping_add(1);
    let destino = ponto(nome, centro, raio, eu.caca.passo);
    eu.caca.patrulha = Some((destino, agora));
    ws.send(envia(&ClientMessage::SetTarget { target: None })?).await?;
    ws.send(envia(&ClientMessage::MoverPara { x: destino.x, z: destino.y })?).await?;
    t.registra(ev(nome, eu, "patrulhou_quest", true, format!("#{} procurando mobs vivos em {:.0},{:.0}", d.quest, destino.x, destino.y)));
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn dez_personagens_patrulham_pontos_distintos_dentro_da_area() {
        let nomes = ["AuricTide", "NyxCoral", "KairoAsh", "MiraMoonfin", "VesperRune", "ZephyrShell", "SableReef", "RookStarfall", "LyraTempest", "TaroSunforge"];
        let pontos: Vec<_> = nomes.iter().map(|n| ponto(n, glam::Vec2::ZERO, 40.0, 1)).collect();
        for (i, p) in pontos.iter().enumerate() {
            assert!(p.length() < 40.0);
            assert_eq!(*p, ponto(nomes[i], glam::Vec2::ZERO, 40.0, 1));
            for q in &pontos[..i] { assert!(p.distance(*q) > 1.0); }
        }
    }
}
