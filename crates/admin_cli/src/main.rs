//! CLI pra mandar AdminCommand pro game server sem precisar de cliente Unity.
//!
//! Uso:
//!   admin_cli --secret SECRET --target CHAR <ACAO> [args]
//!   admin_cli --secret SECRET <ACAO> [args]   # self (sender)
//!
//! Acoes:
//!   set_xp <xp>
//!   set_gold <gold>
//!   give_item <item_id> <qty>
//!   clear_inv
//!   heal
//!   grant_sp <amount>
//!
//! Env:
//!   MMORPG_WS_URL   default ws://127.0.0.1:9000 (local). Pra prod use
//!                   wss://mmo.brunji.com.br/game
//!
//! Como funciona:
//!   - Abre WS, envia Handshake (protocol_version casa do shared)
//!   - Envia AdminCommand com target_char (server resolve sid por nome)
//!   - Espera 500ms pra server processar, desconecta

use anyhow::{bail, Result};
use futures_util::{SinkExt, StreamExt};
use shared::protocol::{AdminAction, ClientMessage};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut secret: Option<String> = None;
    let mut target: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--secret" | "-s" => { secret = Some(args.get(i+1).cloned().unwrap_or_default()); i += 2; }
            "--target" | "-t" => { target = Some(args.get(i+1).cloned().unwrap_or_default()); i += 2; }
            "--help" | "-h" => { print_usage(); return Ok(()); }
            other => { positional.push(other.to_string()); i += 1; }
        }
    }
    let secret = match secret {
        Some(s) if !s.is_empty() => s,
        _ => std::env::var("MMORPG_ADMIN_SECRET")
            .map_err(|_| anyhow::anyhow!("falta --secret ou MMORPG_ADMIN_SECRET no env"))?,
    };
    if positional.is_empty() { print_usage(); bail!("falta acao"); }
    let action = parse_action(&positional)?;
    let url = std::env::var("MMORPG_WS_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:9000".to_string());

    eprintln!("→ {}", url);
    let (ws, _) = tokio_tungstenite::connect_async(&url).await?;
    let (mut tx, mut rx) = ws.split();

    // Handshake. Server kicka se protocol_version nao bater. Outros campos sao
    // opcionais — server soh exige protocol_version pra essa msg.
    let handshake = serde_json::json!({
        "type": "Handshake",
        "protocol_version": shared::PROTOCOL_VERSION,
    });
    tx.send(Message::Text(handshake.to_string())).await?;

    let admin = ClientMessage::AdminCommand { secret, target_char: target.clone(), action };
    let payload = serde_json::to_string(&admin)?;
    eprintln!("→ {}", payload);
    tx.send(Message::Text(payload)).await?;

    // Aguarda 500ms pra server processar e mandar HandshakeAck/Kick eventual
    let _ = tokio::time::timeout(Duration::from_millis(500), async {
        while let Some(Ok(msg)) = rx.next().await {
            if let Message::Text(t) = msg {
                eprintln!("← {}", t);
                if t.contains("\"Kick\"") { break; }
            }
        }
    }).await;
    let _ = tx.send(Message::Close(None)).await;
    eprintln!("✓ enviado{}", target.map(|c| format!(" pra @{}", c)).unwrap_or_default());
    Ok(())
}

fn parse_action(args: &[String]) -> Result<AdminAction> {
    let cmd = args[0].to_lowercase();
    let arg = |n: usize| args.get(n).cloned().unwrap_or_default();
    Ok(match cmd.as_str() {
        "set_xp"      => AdminAction::SetXp { xp: arg(1).parse()? },
        "set_gold"    => AdminAction::SetGold { gold: arg(1).parse()? },
        "give_item"   => AdminAction::GiveItem { item_id: arg(1).parse()?, qty: arg(2).parse()? },
        "clear_inv"   => AdminAction::ClearInventory,
        "heal"        => AdminAction::HealFull,
        "grant_sp"    => AdminAction::GrantSp { amount: arg(1).parse()? },
        "grant_stat"  => AdminAction::GrantStatPoints { amount: arg(1).parse()? },
        "set_level"   => AdminAction::SetLevel { level: arg(1).parse()? },
        other => bail!("acao desconhecida: {}", other),
    })
}

fn print_usage() {
    eprintln!("admin_cli — manda AdminCommand pro game server");
    eprintln!();
    eprintln!("uso:");
    eprintln!("  admin_cli [--secret SECRET] [--target CHAR] <acao> [args]");
    eprintln!();
    eprintln!("acoes:");
    eprintln!("  set_xp <xp>");
    eprintln!("  set_gold <gold>");
    eprintln!("  give_item <item_id> <qty>");
    eprintln!("  clear_inv");
    eprintln!("  heal");
    eprintln!("  grant_sp <amount>");
    eprintln!();
    eprintln!("env:");
    eprintln!("  MMORPG_WS_URL       (default ws://127.0.0.1:9000)");
    eprintln!("  MMORPG_ADMIN_SECRET (fallback se --secret nao passado)");
}
