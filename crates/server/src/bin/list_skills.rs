use anyhow::Result;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<()> {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_|
        "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".into());
    let pool = PgPoolOptions::new().max_connections(2).connect(&url).await?;
    let prof = std::env::args().nth(1).unwrap_or_default();

    let where_clause = if prof.is_empty() { String::new() } else { format!(" WHERE prof = '{}'", prof) };
    #[derive(sqlx::FromRow)]
    struct SkillRow {
        id: i32, prof: String, tier: i16, is_passive: bool, name: String,
        target_type: String, cost_mp: i32, cost_stamina: i32, cooldown_s: f32,
        base_damage: i32, base_heal: i32, scaling_wis: f32,
        range_tiles: f32, radius_tiles: f32,
        path: Option<String>, icon_path: Option<String>, active: bool,
    }
    let q = format!(
        "SELECT id, prof, tier, is_passive, name, target_type, cost_mp, cost_stamina, cooldown_s,
                base_damage, base_heal, scaling_wis, range_tiles, radius_tiles, path, icon_path, active
         FROM skills {} ORDER BY prof, tier, is_passive, id", where_clause);
    let rows: Vec<SkillRow> = sqlx::query_as(&q).fetch_all(&pool).await?;
    println!("{} skills{}", rows.len(), if prof.is_empty() {String::new()} else {format!(" (prof={})", prof)});
    for r in rows {
        let (id,prof,tier,passive,name,tgt,mp,st,cd,dmg,heal,wis,rng,rad,path,icon,active) =
            (r.id, r.prof, r.tier, r.is_passive, r.name, r.target_type, r.cost_mp, r.cost_stamina, r.cooldown_s,
             r.base_damage, r.base_heal, r.scaling_wis, r.range_tiles, r.radius_tiles, r.path, r.icon_path, r.active);
        let ap = if passive { "P" } else { "A" };
        let cost = if mp > 0 { format!("{}mp ", mp) } else { String::new() }
            + &(if st > 0 { format!("{}st", st) } else { String::new() });
        let stat = if dmg > 0 { format!("dmg{}", dmg) }
            else if heal > 0 { format!("heal{}", heal) } else { String::new() };
        let scaling = if wis > 0.0 { format!("+{:.1}wis", wis) } else { String::new() };
        let p = path.unwrap_or_default();
        let i = icon.map(|s| if s.starts_with("Items/") { s[6..].to_string() } else { s }).unwrap_or("—".into());
        let act = if active { "" } else { " [INATIVA]" };
        println!("  #{:4} {:<8} T{} {} {:<22} {:<11} {:<8} {:<8} cd{:.1}s rng{:.1} rad{:.1} {:<10} icon={:<14} path={}{}",
                 id, prof, tier, ap, name, tgt, cost, stat, cd, rng, rad, scaling, i, p, act);
    }
    Ok(())
}
