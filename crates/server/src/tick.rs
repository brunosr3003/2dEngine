//! Loop do mundo em tick fixo. Recebe IncomingMessages por mpsc e avanca
//! a simulacao em `TICK_RATE_HZ`. Single-threaded (proprio tokio task).

use crate::persistence::{CharacterRow, SaveBatch};
use crate::world::{AuthCtx, GameWorld, IncomingMessage};
use anyhow::Result;
use shared::TICK_DT;
use sqlx::postgres::PgPool;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

/// Intervalo em ticks entre persistencias periodicas (1s @ 30Hz).
/// Mudancas criticas (mount/dismount/equip/inventory) tambem disparam save
/// IMMEDIATE via `world.save_dirty_now()`. Combinacao garante < 1s de perda
/// em qualquer crash, evento ou desconexao.
const SAVE_INTERVAL_TICKS: u32 = 30;

/// Janela do p99 do tick: 10 segundos a 30Hz.
///
/// Curta demais e o numero pula com qualquer save; longa demais e ele demora
/// pra reagir a uma horda chegando. Dez segundos e' o tempo que uma briga leva
/// pra virar problema.
const JANELA_TICK: usize = 300;

pub async fn run_world_loop(
    mut rx: mpsc::UnboundedReceiver<IncomingMessage>,
    characters: HashMap<String, CharacterRow>,
    save_tx: mpsc::UnboundedSender<SaveBatch>,
    auth_pool: PgPool,
    mut shutdown: oneshot::Receiver<()>,
    populacao: crate::canais::Populacao,
    saude: crate::canais::Saude,
    diretorio: crate::canais::Diretorio,
) -> Result<()> {
    // Precisa de um tx pra devolver AuthResult pro loop. Criamos um par
    // interno que e fundido com o rx original via tarefa de forward.
    let (auth_tx, mut auth_rx) = mpsc::unbounded_channel::<IncomingMessage>();
    // Map loading: ENV override OR default ao mapfile do projeto. Sem fallback
    // procedural — se o load falhar, o server aborta. O mapa procedural antigo
    // (GameWorld::new) gerava um mundinho 40x30 que confundia (parecia "parede
    // invisivel" pro player). Mantido em codigo so pra testes/legacy.
    let map_path = std::env::var("MAP_FILE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "data/maps/game.json".to_string());
    tracing::info!("loading map: {}", map_path);
    let mf = shared::mapfile::MapFile::load(&map_path).map_err(|e| {
        anyhow::anyhow!(
            "failed to load mapfile '{}': {}. \
             Set MAP_FILE env var ou garanta que data/maps/game.json existe.",
            map_path,
            e
        )
    })?;
    let mut world = GameWorld::new_from_mapfile(characters, mf);
    world.populacao = Some(populacao);
    world.saude = Some(saude.clone());
    world.diretorio = Some(diretorio);
    world.zona = crate::canais::zona();
    // Zona que e' ilha do arquipelago carrega o campo de altura. E' o mesmo
    // `Gerador` que o cliente usa pra desenhar — colisao e desenho saem da
    // mesma funcao, entao nao ha' como divergirem.
    if let Some(def) = shared::terreno::def_da_zona(&world.zona) {
        let dir = std::env::var("MMO_ILHAS").unwrap_or_else(|_| "data/ilhas".into());
        let t0 = Instant::now();
        let ilha = shared::terreno::Ilha::carregar_ou_gerar_da_ilha(&dir, def);
        tracing::info!(
            "ilha '{}' ({:?}, raio {} blocos, {} troncos/matacoes) pronta em {:?}",
            world.zona,
            def.bioma,
            def.raio_blocos,
            ilha.total_de_estorvos(),
            t0.elapsed()
        );
        world.ilha = Some(ilha);
        // Desembarque: o relevo decide, nao a coordenada herdada. E' daqui
        // que a dificuldade cresce pra fora.
        let porto = world.porto();
        world.povoar_ilha(porto);
    }
    // Mercado global: liga a saida do realm ao banco central (docs/MERCADO.md).
    crate::mercado::spawn_relay(auth_pool.clone(), auth_tx.clone());
    world.set_auth_ctx(AuthCtx {
        pool: auth_pool,
        tx: auth_tx,
    });
    let step = Duration::from_secs_f32(TICK_DT);
    let mut next = Instant::now() + step;
    let mut save_counter: u32 = 0;
    // Amostras do tempo de TRABALHO por tick (nao do intervalo entre ticks —
    // esse e' fixo por construcao e nao diria nada). Anel de tamanho fixo:
    // nada aqui pode alocar por tick.
    let mut amostras = [0u32; JANELA_TICK];
    let mut amostra_i = 0usize;
    if world.imortal {
        // Alto e claro: invencibilidade silenciosa e' o melhor jeito de
        // perder uma hora depois achando o balanceamento estranho.
        tracing::warn!("MMO_IMORTAL=1 — jogadores NAO tomam dano nesta instancia");
    }
    tracing::info!("world loop started ({}ms/tick)", step.as_millis());
    // Quando o ultimo retrato do panoptico foi tirado.
    let mut panoptico_em = f32::MIN;

    loop {
        let inicio = Instant::now();
        // Verifica shutdown antes de processar mensagens.
        if shutdown.try_recv().is_ok() {
            tracing::info!("shutdown signal received — saving all characters...");
            let rows = world.collect_character_rows();
            let mercado = world.tomar_registros_mercado(&rows);
            if !rows.is_empty() || !mercado.is_empty() {
                let _ = save_tx.send(SaveBatch { rows, mercado });
            }
            // Aguarda o writer consumir o batch (drena o canal).
            drop(save_tx);
            tokio::time::sleep(Duration::from_millis(500)).await;
            tracing::info!("graceful shutdown complete");
            return Ok(());
        }

        // Drena mensagens da rede.
        loop {
            match rx.try_recv() {
                Ok(IncomingMessage::Connected(h)) => world.on_connect(h),
                Ok(IncomingMessage::Disconnected(id)) => {
                    // Persiste o personagem antes de descartar a sessao.
                    if let Some(row) = world.take_character_for_disconnect(&id) {
                        let rows = vec![row];
                        let mercado = world.tomar_registros_mercado(&rows);
                        let _ = save_tx.send(SaveBatch { rows, mercado });
                    }
                    world.on_disconnect(id);
                }
                Ok(IncomingMessage::Message(id, m)) => world.on_message(id, m),
                Ok(IncomingMessage::Mercado(ev)) => world.on_mercado(ev),
                Ok(IncomingMessage::Presenca(ev)) => world.on_presenca(ev),
                Ok(IncomingMessage::Loja(ev)) => world.on_loja(ev),
                Ok(IncomingMessage::Social { sid, nome, avisos }) => {
                    world.on_social(sid, nome, avisos)
                }
                Ok(IncomingMessage::CorreioEntrega(e)) => world.on_correio_entrega(e),
                Ok(IncomingMessage::CorreioFim { sid, nome, texto }) => {
                    world.on_correio_fim(sid, nome, texto)
                }
                Ok(IncomingMessage::AuthResult(id, r)) => world.on_auth_result(id, r),
                Ok(IncomingMessage::CharCreated(id, row, success)) => {
                    world.on_char_created(id, *row, success);
                }
                Ok(IncomingMessage::CharReloadedForSelect(id, row, success)) => {
                    world.on_char_reloaded_for_select(id, *row, success);
                }
                Ok(IncomingMessage::CharsDaConta(id, conta, rows)) => {
                    world.on_chars_da_conta(id, conta, rows);
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    tracing::warn!("all senders dropped, exiting world loop");
                    return Ok(());
                }
            }
        }

        // Drena resultados de auth (do canal interno).
        loop {
            match auth_rx.try_recv() {
                Ok(IncomingMessage::AuthResult(id, r)) => world.on_auth_result(id, r),
                Ok(IncomingMessage::CharCreated(id, row, success)) => {
                    world.on_char_created(id, *row, success);
                }
                Ok(IncomingMessage::CharReloadedForSelect(id, row, success)) => {
                    world.on_char_reloaded_for_select(id, *row, success);
                }
                Ok(IncomingMessage::CharsDaConta(id, conta, rows)) => {
                    world.on_chars_da_conta(id, conta, rows);
                }
                Ok(IncomingMessage::Mercado(ev)) => world.on_mercado(ev),
                Ok(IncomingMessage::Presenca(ev)) => world.on_presenca(ev),
                Ok(IncomingMessage::Loja(ev)) => world.on_loja(ev),
                Ok(IncomingMessage::Social { sid, nome, avisos }) => {
                    world.on_social(sid, nome, avisos)
                }
                Ok(IncomingMessage::CorreioEntrega(e)) => world.on_correio_entrega(e),
                Ok(IncomingMessage::CorreioFim { sid, nome, texto }) => {
                    world.on_correio_fim(sid, nome, texto)
                }
                Ok(_) => {} // outros variantes nao devem chegar aqui
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => break,
            }
        }

        world.step(TICK_DT);
        world.send_snapshots();
        // Retrato pro panoptico. Sai do laco ja' serializado; quem atende a
        // requisicao nao encosta no mundo.
        crate::panoptico::publicar(&world, &mut panoptico_em);

        // Fila de entrada: 1x por segundo basta, e evita 30 varreduras/s.
        if save_counter % 30 == 0 {
            world.tick_fila();
            world.tick_diarias();
        }
        // Lotacao do canal pro HUD: a cada 5s.
        if save_counter % 150 == 0 {
            world.avisa_info_canal();
        }
        // Medidas pro panoptico (o valor de agora): a cada 30 s.
        if save_counter % 900 == 0 {
            world.medir_telemetria();
        }
        save_counter = save_counter.wrapping_add(1);
        // Trigger imediato (`save_pending`) ou periodico (1s). save_pending eh
        // setado em mudancas criticas: mount/dismount, equip, inventario, etc.
        let should_save = world.save_pending || (save_counter % SAVE_INTERVAL_TICKS == 0);
        if should_save {
            let rows = world.collect_character_rows();
            let mercado = world.tomar_registros_mercado(&rows);
            if !rows.is_empty() || !mercado.is_empty() {
                let _ = save_tx.send(SaveBatch { rows, mercado });
            }
            world.save_pending = false;
        }

        // ── Saude do tick ────────────────────────────────────────────────
        // O aviso de lag que ja' existia so' dispara com 10 ticks de atraso —
        // um terco de segundo DEPOIS do jogador ja' estar sentindo. Este p99
        // e' o sinal antecedente: sobe enquanto ainda da' tempo de parar de
        // admitir gente.
        amostras[amostra_i] = inicio.elapsed().as_micros().min(u32::MAX as u128) as u32;
        amostra_i = (amostra_i + 1) % JANELA_TICK;
        if amostra_i == 0 {
            let mut ordenado = amostras;
            ordenado.sort_unstable();
            let p99 = ordenado[JANELA_TICK * 99 / 100];
            saude.set_p99_us(p99);
            crate::telemetria::medir("tick_p99_ms", p99 as f64 / 1000.0);
            crate::telemetria::medir("montados", world.montados() as f64);
            let carga = p99 as f32 / (TICK_DT * 1_000_000.0);
            if carga >= 0.75 {
                tracing::warn!(
                    "tick p99 {:.1}ms ({:.0}% do orcamento) com {} jogadores",
                    p99 as f32 / 1000.0,
                    carga * 100.0,
                    world.dentro_pub()
                );
            }
        }

        let now = Instant::now();
        if next > now {
            tokio::time::sleep(next - now).await;
        } else {
            // Atrasamos — pular para catch-up sem acumular infinito.
            let lag = now - next;
            if lag > step * 10 {
                tracing::warn!("world loop lag {lag:?} — resetando timer");
                next = now;
            }
        }
        next += step;
    }
}
