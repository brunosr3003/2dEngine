//! Admin de economia — CRUD via web pra editar items, drops e enemy_kinds.
//!
//! Auth: senha em `ECON_ADMIN_PASSWORD` (ou fallback `PIXEL_ADMIN_PASSWORD`).
//! POST `/api/econ/login` retorna JWT (cookie `econ_auth` + campo `token`).
//! Demais endpoints exigem header `Authorization: Bearer <token>` ou cookie.
//!
//! Após cada write o handler bumpa `economy_version.version` — o game server
//! detecta no polling de 5s e recarrega `economy::cell()` sem restart.
//!
//! Endpoints:
//!   POST   /api/econ/login                       { password }
//!   POST   /api/econ/verify
//!   GET    /api/econ/items                       lista todos
//!   POST   /api/econ/items                       cria/upsert
//!   PUT    /api/econ/items/:id                   atualiza
//!   DELETE /api/econ/items/:id                   remove
//!   GET    /api/econ/enemies                     lista
//!   PUT    /api/econ/enemies/:kind               atualiza
//!   GET    /api/econ/drops                       lista (opcional ?kind=X)
//!   POST   /api/econ/drops                       cria
//!   PUT    /api/econ/drops/:id                   atualiza
//!   DELETE /api/econ/drops/:id                   remove

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct EconState {
    pub pool: Arc<PgPool>,
    pub admin_password: Arc<String>,
    pub jwt_secret: Arc<String>,
    /// Pasta com PNGs disponíveis pro picker de ícone (env `ICONS_DIR`).
    /// Tipicamente aponta pra `MMORPG/Assets/_Project/Resources/Items/`.
    pub icons_dir: Arc<Option<std::path::PathBuf>>,
}

#[derive(Serialize, Deserialize)]
struct JwtClaims {
    sub: String,
    exp: u64,
    iat: u64,
}

impl EconState {
    pub fn from_env(pool: Arc<PgPool>) -> Self {
        // Senha admin: prefere ECON_ADMIN_PASSWORD, senão reaproveita PIXEL.
        let admin_password = std::env::var("ECON_ADMIN_PASSWORD")
            .or_else(|_| std::env::var("PIXEL_ADMIN_PASSWORD"))
            .unwrap_or_else(|_| "Nop1nop2!".into());
        let jwt_secret = std::env::var("ECON_JWT_SECRET")
            .unwrap_or_else(|_| format!("econ-jwt-{}", admin_password));
        let icons_dir = std::env::var("ICONS_DIR")
            .ok()
            .map(std::path::PathBuf::from);
        Self {
            pool,
            admin_password: Arc::new(admin_password),
            jwt_secret: Arc::new(jwt_secret),
            icons_dir: Arc::new(icons_dir),
        }
    }

    fn issue_token(&self) -> anyhow::Result<String> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let claims = JwtClaims {
            sub: "econ-admin".into(),
            iat: now,
            exp: now + 7 * 24 * 3600,
        };
        let tok = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret.as_bytes()),
        )?;
        Ok(tok)
    }

    fn is_authed(&self, headers: &HeaderMap) -> bool {
        if let Some(auth) = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
        {
            if let Some(tok) = auth.strip_prefix("Bearer ") {
                if decode::<JwtClaims>(
                    tok.trim(),
                    &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
                    &Validation::default(),
                )
                .is_ok()
                {
                    return true;
                }
            }
        }
        if let Some(cookie) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
            for part in cookie.split(';').map(|p| p.trim()) {
                if let Some(tok) = part.strip_prefix("econ_auth=") {
                    if decode::<JwtClaims>(
                        tok,
                        &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
                        &Validation::default(),
                    )
                    .is_ok()
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}

pub fn router(state: EconState) -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/verify", post(verify))
        .route("/items", get(items_list).post(items_upsert))
        .route("/items/:id", put(items_update).delete(items_delete))
        .route("/skills", get(skills_list).post(skills_upsert))
        .route("/skills/:id", put(skills_update).delete(skills_delete))
        .route("/enemies", get(enemies_list))
        .route("/enemies/:kind", put(enemies_update))
        .route("/drops", get(drops_list).post(drops_create))
        .route("/drops/:id", put(drops_update).delete(drops_delete))
        .route("/farm-drops", get(farm_drops_list).post(farm_drops_create))
        .route(
            "/farm-drops/:id",
            put(farm_drops_update).delete(farm_drops_delete),
        )
        .route("/icons", get(icons_list))
        .route("/icons/:name", get(icon_file))
        .route("/report/summary", get(report_summary))
        .route("/report/items", get(report_items))
        .route("/report/items/:id", get(report_item_detail))
        .route("/report/players", get(report_players))
        .route("/report/drops", get(report_drops))
        .with_state(state)
}

// ── helpers ──────────────────────────────────────────────────────────────

fn unauth() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": "nao autenticado"})),
    )
        .into_response()
}

fn ise(msg: impl std::fmt::Display) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": msg.to_string()})),
    )
        .into_response()
}

fn bad(msg: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": msg.into()})),
    )
        .into_response()
}

/// Bumpa `economy_version.version` pra disparar hot-reload no game server.
async fn bump_version(pool: &PgPool) -> anyhow::Result<i64> {
    let v: i64 = sqlx::query_scalar(
        "UPDATE economy_version SET version = version + 1, updated_at = NOW() \
         WHERE id = 1 RETURNING version",
    )
    .fetch_one(pool)
    .await?;
    Ok(v)
}

// ── auth ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LoginReq {
    password: String,
}

async fn login(State(s): State<EconState>, Json(req): Json<LoginReq>) -> Response {
    if req.password != *s.admin_password {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "senha invalida"})),
        )
            .into_response();
    }
    let token = match s.issue_token() {
        Ok(t) => t,
        Err(e) => return ise(format!("jwt: {e}")),
    };
    let cookie = format!("econ_auth={}; Path=/; SameSite=Lax; Max-Age=604800", token);
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({"ok": true, "token": token})),
    )
        .into_response()
}

async fn verify(State(s): State<EconState>, headers: HeaderMap) -> Response {
    if s.is_authed(&headers) {
        Json(serde_json::json!({"ok": true})).into_response()
    } else {
        unauth()
    }
}

// ── items ────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct ItemRow {
    id: i32,
    name: String,
    sell_price: i32,
    buy_price: Option<i32>,
    shop_order: Option<i32>,
    stack_max: i32,
    equip_slot: Option<String>,
    item_level: i32,
    icon_col: i32,
    icon_row: i32,
    icon_path: Option<String>,
    active: bool,
    hp_min: i32,
    hp_max: i32,
    mp_min: i32,
    mp_max: i32,
    atk_min: i32,
    atk_max: i32,
    def_min: i32,
    def_max: i32,
    dex_min: i32,
    dex_max: i32,
    wis_min: i32,
    wis_max: i32,
}

#[derive(Deserialize)]
struct ItemPayload {
    id: i32,
    name: String,
    sell_price: i32,
    buy_price: Option<i32>,
    shop_order: Option<i32>,
    stack_max: i32,
    equip_slot: Option<String>,
    item_level: i32,
    icon_col: i32,
    icon_row: i32,
    icon_path: Option<String>,
    #[serde(default = "default_true_payload")]
    active: bool,
    hp_min: i32,
    hp_max: i32,
    mp_min: i32,
    mp_max: i32,
    atk_min: i32,
    atk_max: i32,
    def_min: i32,
    def_max: i32,
    dex_min: i32,
    dex_max: i32,
    wis_min: i32,
    wis_max: i32,
}

fn default_true_payload() -> bool {
    true
}

async fn items_list(State(s): State<EconState>, headers: HeaderMap) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    let rows: Vec<ItemRow> = match sqlx::query_as(
        "SELECT id, name, sell_price, buy_price, shop_order, stack_max, \
                equip_slot, item_level, icon_col, icon_row, icon_path, active, \
                hp_min, hp_max, mp_min, mp_max, atk_min, atk_max, \
                def_min, def_max, dex_min, dex_max, wis_min, wis_max \
         FROM items ORDER BY id",
    )
    .fetch_all(s.pool.as_ref())
    .await
    {
        Ok(r) => r,
        Err(e) => return ise(e),
    };
    Json(serde_json::json!({"items": rows})).into_response()
}

async fn items_upsert(
    State(s): State<EconState>,
    headers: HeaderMap,
    Json(p): Json<ItemPayload>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    if p.id <= 0 || p.id > 65000 {
        return bad("id fora do range");
    }
    if p.name.trim().is_empty() {
        return bad("nome vazio");
    }
    if let Err(e) = sqlx::query(
        "INSERT INTO items \
         (id, name, sell_price, buy_price, shop_order, stack_max, \
          equip_slot, item_level, icon_col, icon_row, icon_path, active, \
          hp_min, hp_max, mp_min, mp_max, atk_min, atk_max, \
          def_min, def_max, dex_min, dex_max, wis_min, wis_max) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24) \
         ON CONFLICT (id) DO UPDATE SET \
            name = EXCLUDED.name, sell_price = EXCLUDED.sell_price, \
            buy_price = EXCLUDED.buy_price, shop_order = EXCLUDED.shop_order, \
            stack_max = EXCLUDED.stack_max, \
            equip_slot = EXCLUDED.equip_slot, item_level = EXCLUDED.item_level, \
            icon_col = EXCLUDED.icon_col, icon_row = EXCLUDED.icon_row, \
            icon_path = EXCLUDED.icon_path, active = EXCLUDED.active, \
            hp_min = EXCLUDED.hp_min, hp_max = EXCLUDED.hp_max, \
            mp_min = EXCLUDED.mp_min, mp_max = EXCLUDED.mp_max, \
            atk_min = EXCLUDED.atk_min, atk_max = EXCLUDED.atk_max, \
            def_min = EXCLUDED.def_min, def_max = EXCLUDED.def_max, \
            dex_min = EXCLUDED.dex_min, dex_max = EXCLUDED.dex_max, \
            wis_min = EXCLUDED.wis_min, wis_max = EXCLUDED.wis_max"
    )
    .bind(p.id).bind(&p.name).bind(p.sell_price).bind(p.buy_price)
    .bind(p.shop_order).bind(p.stack_max.max(1))
    .bind(&p.equip_slot).bind(p.item_level.max(1)).bind(p.icon_col).bind(p.icon_row)
    .bind(&p.icon_path).bind(p.active)
    .bind(p.hp_min).bind(p.hp_max).bind(p.mp_min).bind(p.mp_max)
    .bind(p.atk_min).bind(p.atk_max).bind(p.def_min).bind(p.def_max)
    .bind(p.dex_min).bind(p.dex_max).bind(p.wis_min).bind(p.wis_max)
    .execute(s.pool.as_ref()).await { return ise(e); }
    let v = bump_version(s.pool.as_ref()).await.unwrap_or(0);
    Json(serde_json::json!({"ok": true, "version": v})).into_response()
}

async fn items_update(
    State(s): State<EconState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(mut p): Json<ItemPayload>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    p.id = id;
    items_upsert(State(s), headers, Json(p)).await
}

async fn items_delete(
    State(s): State<EconState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    if let Err(e) = sqlx::query("DELETE FROM items WHERE id = $1")
        .bind(id)
        .execute(s.pool.as_ref())
        .await
    {
        return ise(e);
    }
    let v = bump_version(s.pool.as_ref()).await.unwrap_or(0);
    Json(serde_json::json!({"ok": true, "version": v})).into_response()
}

// ── enemies ──────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct EnemyRow {
    kind: i32,
    name: String,
    hp_max: i32,
    speed: f32,
    attack_damage: i32,
    attack_cooldown: f32,
    detect_range: f32,
    attack_range: f32,
    kite_dist: Option<f32>,
    proj_count: i32,
    xp_reward: i64,
    defense: i32,
    size_scale: f32,
    tint_r: f32,
    tint_g: f32,
    tint_b: f32,
    tint_a: f32,
    loot_item_level: Option<i32>,
}

#[derive(Deserialize)]
struct EnemyPayload {
    name: String,
    hp_max: i32,
    speed: f32,
    attack_damage: i32,
    attack_cooldown: f32,
    detect_range: f32,
    attack_range: f32,
    kite_dist: Option<f32>,
    proj_count: i32,
    xp_reward: i64,
    defense: i32,
    size_scale: f32,
    tint_r: f32,
    tint_g: f32,
    tint_b: f32,
    tint_a: f32,
    loot_item_level: Option<i32>,
}

async fn enemies_list(State(s): State<EconState>, headers: HeaderMap) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    let rows: Vec<EnemyRow> = match sqlx::query_as(
        "SELECT kind, name, hp_max, speed, attack_damage, attack_cooldown, detect_range, \
                attack_range, kite_dist, proj_count, xp_reward, defense, size_scale, \
                tint_r, tint_g, tint_b, tint_a, loot_item_level \
         FROM enemy_kinds ORDER BY kind",
    )
    .fetch_all(s.pool.as_ref())
    .await
    {
        Ok(r) => r,
        Err(e) => return ise(e),
    };
    Json(serde_json::json!({"enemies": rows})).into_response()
}

async fn enemies_update(
    State(s): State<EconState>,
    headers: HeaderMap,
    Path(kind): Path<i32>,
    Json(p): Json<EnemyPayload>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    if let Err(e) = sqlx::query(
        "UPDATE enemy_kinds SET \
            name = $2, hp_max = $3, speed = $4, attack_damage = $5, attack_cooldown = $6, \
            detect_range = $7, attack_range = $8, kite_dist = $9, proj_count = $10, \
            xp_reward = $11, defense = $12, size_scale = $13, \
            tint_r = $14, tint_g = $15, tint_b = $16, tint_a = $17, \
            loot_item_level = $18 \
         WHERE kind = $1",
    )
    .bind(kind)
    .bind(&p.name)
    .bind(p.hp_max)
    .bind(p.speed)
    .bind(p.attack_damage)
    .bind(p.attack_cooldown)
    .bind(p.detect_range)
    .bind(p.attack_range)
    .bind(p.kite_dist)
    .bind(p.proj_count.max(1))
    .bind(p.xp_reward)
    .bind(p.defense)
    .bind(p.size_scale)
    .bind(p.tint_r)
    .bind(p.tint_g)
    .bind(p.tint_b)
    .bind(p.tint_a)
    .bind(p.loot_item_level)
    .execute(s.pool.as_ref())
    .await
    {
        return ise(e);
    }
    let v = bump_version(s.pool.as_ref()).await.unwrap_or(0);
    Json(serde_json::json!({"ok": true, "version": v})).into_response()
}

// ── drops ────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct DropRow {
    id: i32,
    enemy_kind: i32,
    item_id: i32,
    qty_min: i32,
    qty_max: i32,
    chance: f32,
}

#[derive(Deserialize)]
struct DropPayload {
    enemy_kind: i32,
    item_id: i32,
    qty_min: i32,
    qty_max: i32,
    chance: f32,
}

#[derive(Deserialize)]
struct DropsQuery {
    kind: Option<i32>,
}

async fn drops_list(
    State(s): State<EconState>,
    headers: HeaderMap,
    Query(q): Query<DropsQuery>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    let rows: Vec<DropRow> = match q.kind {
        Some(k) => {
            sqlx::query_as(
                "SELECT id, enemy_kind, item_id, qty_min, qty_max, chance \
             FROM loot_drops WHERE enemy_kind = $1 ORDER BY id",
            )
            .bind(k)
            .fetch_all(s.pool.as_ref())
            .await
        }
        None => {
            sqlx::query_as(
                "SELECT id, enemy_kind, item_id, qty_min, qty_max, chance \
             FROM loot_drops ORDER BY enemy_kind, id",
            )
            .fetch_all(s.pool.as_ref())
            .await
        }
    }
    .unwrap_or_default();
    Json(serde_json::json!({"drops": rows})).into_response()
}

async fn drops_create(
    State(s): State<EconState>,
    headers: HeaderMap,
    Json(p): Json<DropPayload>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    if p.qty_min < 0 || p.qty_max < p.qty_min {
        return bad("qty_min/max invalido");
    }
    let chance = p.chance.clamp(0.0, 1.0);
    let id: i32 = match sqlx::query_scalar(
        "INSERT INTO loot_drops (enemy_kind, item_id, qty_min, qty_max, chance) \
         VALUES ($1,$2,$3,$4,$5) RETURNING id",
    )
    .bind(p.enemy_kind)
    .bind(p.item_id)
    .bind(p.qty_min)
    .bind(p.qty_max)
    .bind(chance)
    .fetch_one(s.pool.as_ref())
    .await
    {
        Ok(v) => v,
        Err(e) => return ise(e),
    };
    let v = bump_version(s.pool.as_ref()).await.unwrap_or(0);
    Json(serde_json::json!({"ok": true, "id": id, "version": v})).into_response()
}

async fn drops_update(
    State(s): State<EconState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(p): Json<DropPayload>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    if p.qty_min < 0 || p.qty_max < p.qty_min {
        return bad("qty_min/max invalido");
    }
    let chance = p.chance.clamp(0.0, 1.0);
    if let Err(e) = sqlx::query(
        "UPDATE loot_drops SET enemy_kind = $2, item_id = $3, qty_min = $4, qty_max = $5, chance = $6 \
         WHERE id = $1"
    )
    .bind(id).bind(p.enemy_kind).bind(p.item_id).bind(p.qty_min).bind(p.qty_max).bind(chance)
    .execute(s.pool.as_ref()).await { return ise(e); }
    let v = bump_version(s.pool.as_ref()).await.unwrap_or(0);
    Json(serde_json::json!({"ok": true, "version": v})).into_response()
}

async fn drops_delete(
    State(s): State<EconState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    if let Err(e) = sqlx::query("DELETE FROM loot_drops WHERE id = $1")
        .bind(id)
        .execute(s.pool.as_ref())
        .await
    {
        return ise(e);
    }
    let v = bump_version(s.pool.as_ref()).await.unwrap_or(0);
    Json(serde_json::json!({"ok": true, "version": v})).into_response()
}

// ── farm-drops (Tree/Rock/Flower × tier 1..4) ───────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct FarmDropRow {
    id: i32,
    kind: String,
    tier: i32,
    item_id: i32,
    qty_min: i32,
    qty_max: i32,
    chance: f32,
}

#[derive(Deserialize)]
struct FarmDropPayload {
    kind: String,
    tier: i32,
    item_id: i32,
    qty_min: i32,
    qty_max: i32,
    chance: f32,
}

#[derive(Deserialize)]
struct FarmDropsQuery {
    kind: Option<String>,
}

fn validate_farm_kind(kind: &str) -> bool {
    matches!(kind, "Tree" | "Rock" | "Flower")
}

async fn farm_drops_list(
    State(s): State<EconState>,
    headers: HeaderMap,
    Query(q): Query<FarmDropsQuery>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    let rows: Vec<FarmDropRow> = match q.kind.as_deref() {
        Some(k) => {
            sqlx::query_as(
                "SELECT id, kind, tier, item_id, qty_min, qty_max, chance \
             FROM farm_node_drops WHERE kind = $1 ORDER BY tier, id",
            )
            .bind(k)
            .fetch_all(s.pool.as_ref())
            .await
        }
        None => {
            sqlx::query_as(
                "SELECT id, kind, tier, item_id, qty_min, qty_max, chance \
             FROM farm_node_drops ORDER BY kind, tier, id",
            )
            .fetch_all(s.pool.as_ref())
            .await
        }
    }
    .unwrap_or_default();
    Json(serde_json::json!({"farm_drops": rows})).into_response()
}

async fn farm_drops_create(
    State(s): State<EconState>,
    headers: HeaderMap,
    Json(p): Json<FarmDropPayload>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    if !validate_farm_kind(&p.kind) {
        return bad("kind deve ser Tree/Rock/Flower");
    }
    if p.tier < 1 || p.tier > 4 {
        return bad("tier deve estar em 1..4");
    }
    if p.qty_min < 0 || p.qty_max < p.qty_min {
        return bad("qty_min/max invalido");
    }
    let chance = p.chance.clamp(0.0, 1.0);
    let id: i32 = match sqlx::query_scalar(
        "INSERT INTO farm_node_drops (kind, tier, item_id, qty_min, qty_max, chance) \
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(&p.kind)
    .bind(p.tier)
    .bind(p.item_id)
    .bind(p.qty_min)
    .bind(p.qty_max)
    .bind(chance)
    .fetch_one(s.pool.as_ref())
    .await
    {
        Ok(v) => v,
        Err(e) => return ise(e),
    };
    let v = bump_version(s.pool.as_ref()).await.unwrap_or(0);
    Json(serde_json::json!({"ok": true, "id": id, "version": v})).into_response()
}

async fn farm_drops_update(
    State(s): State<EconState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(p): Json<FarmDropPayload>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    if !validate_farm_kind(&p.kind) {
        return bad("kind deve ser Tree/Rock/Flower");
    }
    if p.tier < 1 || p.tier > 4 {
        return bad("tier deve estar em 1..4");
    }
    if p.qty_min < 0 || p.qty_max < p.qty_min {
        return bad("qty_min/max invalido");
    }
    let chance = p.chance.clamp(0.0, 1.0);
    if let Err(e) = sqlx::query(
        "UPDATE farm_node_drops SET kind = $2, tier = $3, item_id = $4, \
         qty_min = $5, qty_max = $6, chance = $7 WHERE id = $1",
    )
    .bind(id)
    .bind(&p.kind)
    .bind(p.tier)
    .bind(p.item_id)
    .bind(p.qty_min)
    .bind(p.qty_max)
    .bind(chance)
    .execute(s.pool.as_ref())
    .await
    {
        return ise(e);
    }
    let v = bump_version(s.pool.as_ref()).await.unwrap_or(0);
    Json(serde_json::json!({"ok": true, "version": v})).into_response()
}

async fn farm_drops_delete(
    State(s): State<EconState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    if let Err(e) = sqlx::query("DELETE FROM farm_node_drops WHERE id = $1")
        .bind(id)
        .execute(s.pool.as_ref())
        .await
    {
        return ise(e);
    }
    let v = bump_version(s.pool.as_ref()).await.unwrap_or(0);
    Json(serde_json::json!({"ok": true, "version": v})).into_response()
}

// ── icons (browser do picker) ───────────────────────────────────────────

async fn icons_list(State(s): State<EconState>, headers: HeaderMap) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    let Some(dir) = s.icons_dir.as_ref().clone() else {
        return Json(
            serde_json::json!({"icons": [], "configured": false, "resources_prefix": "Items"}),
        )
        .into_response();
    };
    let mut icons: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if let Some(n) = e.file_name().to_str() {
                if n.to_lowercase().ends_with(".png") {
                    icons.push(
                        n.trim_end_matches(".png")
                            .trim_end_matches(".PNG")
                            .to_string(),
                    );
                }
            }
        }
    }
    icons.sort();
    // Deriva o prefixo Resources a partir do path absoluto do ICONS_DIR.
    // Cliente precisa do path RELATIVO a Assets/_Project/Resources/ pra
    // popular icon_path do item — Unity Resources.Load resolve assim.
    // Ex: ICONS_DIR=/.../Resources/Icons/sliced → "Icons/sliced".
    let prefix = dir.to_string_lossy().to_string();
    let resources_prefix = if let Some(idx) = prefix.find("/Resources/") {
        prefix[idx + "/Resources/".len()..]
            .trim_end_matches('/')
            .to_string()
    } else if let Some(idx) = prefix.find("\\Resources\\") {
        prefix[idx + "\\Resources\\".len()..]
            .trim_end_matches('\\')
            .replace('\\', "/")
    } else {
        // Fallback: usa o nome do diretorio final.
        dir.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Items")
            .to_string()
    };
    Json(serde_json::json!({
        "icons": icons,
        "configured": true,
        "resources_prefix": resources_prefix,
    }))
    .into_response()
}

// ── relatório / observabilidade ─────────────────────────────────────────

async fn report_summary(State(s): State<EconState>, headers: HeaderMap) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    let pool = s.pool.as_ref();
    // Gold em circulação = soma de qty onde item_id=1 em inventory + vault
    // (equipment não comporta gold). Se faltar, fica 0.
    let gold: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT SUM(qty)::BIGINT FROM inventory WHERE item_id=1),0) \
              + COALESCE((SELECT SUM(qty)::BIGINT FROM vault     WHERE item_id=1),0)",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    let players: i64 = sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM characters")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let inv_slots: i64 = sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM inventory WHERE qty>0")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let vault_slots: i64 = sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM vault WHERE qty>0")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let equipped: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM equipment WHERE item_id>0")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
    let drops_total: i64 = sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM item_drops_log")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let drops_24h: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM item_drops_log WHERE ts > NOW() - INTERVAL '24 hours'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    Json(serde_json::json!({
        "gold_total":    gold,
        "players":       players,
        "inv_slots":     inv_slots,
        "vault_slots":   vault_slots,
        "equipped":      equipped,
        "drops_total":   drops_total,
        "drops_24h":     drops_24h,
    }))
    .into_response()
}

#[derive(Serialize, sqlx::FromRow)]
struct ItemReportRow {
    item_id: i32,
    name: Option<String>,
    in_inventory: i64,
    in_vault: i64,
    in_equipment: i64,
    dropped_total: i64,
    dropped_qty: i64,
}

async fn report_items(State(s): State<EconState>, headers: HeaderMap) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    let rows: Vec<ItemReportRow> = sqlx::query_as(
        "WITH inv  AS (SELECT item_id, SUM(qty)::BIGINT AS qty FROM inventory WHERE qty>0 GROUP BY item_id), \
              v    AS (SELECT item_id, SUM(qty)::BIGINT AS qty FROM vault     WHERE qty>0 GROUP BY item_id), \
              eq   AS (SELECT item_id, COUNT(*)::BIGINT AS qty FROM equipment WHERE item_id>0 GROUP BY item_id), \
              dr   AS (SELECT item_id, COUNT(*)::BIGINT AS rolls, SUM(qty)::BIGINT AS qty \
                       FROM item_drops_log GROUP BY item_id), \
              all_ids AS ( \
                SELECT item_id FROM inv UNION SELECT item_id FROM v UNION \
                SELECT item_id FROM eq  UNION SELECT item_id FROM dr) \
         SELECT a.item_id, i.name, \
                COALESCE(inv.qty, 0)::BIGINT AS in_inventory, \
                COALESCE(v.qty,   0)::BIGINT AS in_vault, \
                COALESCE(eq.qty,  0)::BIGINT AS in_equipment, \
                COALESCE(dr.rolls,0)::BIGINT AS dropped_total, \
                COALESCE(dr.qty,  0)::BIGINT AS dropped_qty \
         FROM all_ids a \
         LEFT JOIN inv USING (item_id) \
         LEFT JOIN v   USING (item_id) \
         LEFT JOIN eq  USING (item_id) \
         LEFT JOIN dr  USING (item_id) \
         LEFT JOIN items i ON i.id = a.item_id \
         ORDER BY a.item_id"
    ).fetch_all(s.pool.as_ref()).await.unwrap_or_default();
    Json(serde_json::json!({"items": rows})).into_response()
}

#[derive(Serialize, sqlx::FromRow)]
struct PlayerReportRow {
    name: String,
    xp: i64,
    gold: i64,
    inv_count: i64,
    vault_count: i64,
    equipment_count: i64,
}

async fn report_players(State(s): State<EconState>, headers: HeaderMap) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    let rows: Vec<PlayerReportRow> = match sqlx::query_as(
        "SELECT c.name, c.xp, \
                COALESCE((SELECT SUM(qty)::BIGINT FROM inventory i \
                          WHERE i.character_name = c.name AND i.item_id = 1), 0)::BIGINT \
                  + COALESCE((SELECT SUM(qty)::BIGINT FROM vault v \
                          WHERE v.character_name = c.name AND v.item_id = 1), 0)::BIGINT \
                  AS gold, \
                COALESCE((SELECT COUNT(*)::BIGINT FROM inventory i \
                          WHERE i.character_name = c.name AND i.qty > 0), 0)::BIGINT AS inv_count, \
                COALESCE((SELECT COUNT(*)::BIGINT FROM vault v \
                          WHERE v.character_name = c.name AND v.qty > 0), 0)::BIGINT AS vault_count, \
                COALESCE((SELECT COUNT(*)::BIGINT FROM equipment e \
                          WHERE e.character_name = c.name AND e.item_id > 0), 0)::BIGINT \
                  AS equipment_count \
         FROM characters c \
         ORDER BY gold DESC, c.name ASC"
    ).fetch_all(s.pool.as_ref()).await {
        Ok(r) => r, Err(e) => { tracing::warn!("report_players: {e}"); Vec::new() },
    };
    Json(serde_json::json!({"players": rows})).into_response()
}

// ── relatório detalhado de UM item (com instance_data parseado) ────────

#[derive(Serialize)]
struct ItemHolding {
    character: String,
    location: String, // "inventory" | "vault" | "equipment"
    slot: String,     // index (inv/vault) ou nome do equip slot
    qty: i32,
    rarity: Option<u8>, // None = sem instance (legacy/stack)
    refinement: Option<u8>,
    item_level: Option<u16>,
    hp_max: i32,
    mp_max: i32,
    attack: i32,
    defense: i32,
    dex: i32,
    wis: i32,
    sockets: u8,
    affix_count: u8,
}

#[derive(Serialize)]
struct ItemDetailReport {
    item_id: i32,
    holdings: Vec<ItemHolding>,
    rarity_counts: [i64; 5],
    refinement_counts: std::collections::BTreeMap<u8, i64>,
    item_level_hist: std::collections::BTreeMap<u16, i64>,
    total_qty: i64,
    inv_qty: i64,
    vault_qty: i64,
    equip_qty: i64,
    no_instance: i64,
    by_player_qty: std::collections::BTreeMap<String, i64>,
}

async fn report_item_detail(
    State(s): State<EconState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    let pool = s.pool.as_ref();

    // Helper pra parsear instance_data e gerar ItemHolding
    fn parse(
        character: String,
        location: &str,
        slot: String,
        qty: i32,
        raw: Option<String>,
    ) -> ItemHolding {
        let inst: Option<shared::items::ItemInstance> =
            raw.as_deref().and_then(|s| serde_json::from_str(s).ok());
        match inst {
            Some(i) => ItemHolding {
                character,
                location: location.into(),
                slot,
                qty,
                rarity: Some(i.rarity),
                refinement: Some(i.refinement),
                item_level: Some(i.item_level),
                hp_max: i.hp_max,
                mp_max: i.mp_max,
                attack: i.attack_damage,
                defense: i.defense,
                dex: i.dex,
                wis: i.wis,
                sockets: i.sockets,
                affix_count: i.affixes.iter().filter(|a| !a.is_empty()).count() as u8,
            },
            None => ItemHolding {
                character,
                location: location.into(),
                slot,
                qty,
                rarity: None,
                refinement: None,
                item_level: None,
                hp_max: 0,
                mp_max: 0,
                attack: 0,
                defense: 0,
                dex: 0,
                wis: 0,
                sockets: 0,
                affix_count: 0,
            },
        }
    }

    let mut holdings: Vec<ItemHolding> = Vec::new();
    // Inventory
    let inv: Vec<(String, i32, i32, Option<String>)> = sqlx::query_as(
        "SELECT character_name, slot, qty, instance_data FROM inventory \
         WHERE item_id = $1 AND qty > 0",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (ch, sl, q, raw) in inv {
        holdings.push(parse(ch, "inventory", sl.to_string(), q, raw));
    }

    // Vault
    let v: Vec<(String, i32, i32, Option<String>)> = sqlx::query_as(
        "SELECT character_name, slot, qty, instance_data FROM vault \
         WHERE item_id = $1 AND qty > 0",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (ch, sl, q, raw) in v {
        holdings.push(parse(ch, "vault", sl.to_string(), q, raw));
    }

    // Equipment (slot é texto: Weapon, Helm, etc.)
    let eq: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT character_name, slot, instance_data FROM equipment \
         WHERE item_id = $1",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (ch, sl, raw) in eq {
        holdings.push(parse(ch, "equipment", sl, 1, raw));
    }

    // Aggregations
    let mut rarity_counts = [0i64; 5];
    let mut refinement_counts: std::collections::BTreeMap<u8, i64> = Default::default();
    let mut item_level_hist: std::collections::BTreeMap<u16, i64> = Default::default();
    let mut by_player_qty: std::collections::BTreeMap<String, i64> = Default::default();
    let mut total_qty = 0i64;
    let mut inv_qty = 0i64;
    let mut vault_qty = 0i64;
    let mut equip_qty = 0i64;
    let mut no_instance = 0i64;
    for h in &holdings {
        let q = h.qty as i64;
        total_qty += q;
        match h.location.as_str() {
            "inventory" => inv_qty += q,
            "vault" => vault_qty += q,
            "equipment" => equip_qty += q,
            _ => {}
        }
        *by_player_qty.entry(h.character.clone()).or_insert(0) += q;
        match h.rarity {
            Some(r) if (r as usize) < rarity_counts.len() => rarity_counts[r as usize] += q,
            None => no_instance += q,
            _ => {}
        }
        if let Some(rf) = h.refinement {
            *refinement_counts.entry(rf).or_insert(0) += q;
        }
        if let Some(lv) = h.item_level {
            *item_level_hist.entry(lv).or_insert(0) += q;
        }
    }

    Json(ItemDetailReport {
        item_id: id,
        holdings,
        rarity_counts,
        refinement_counts,
        item_level_hist,
        total_qty,
        inv_qty,
        vault_qty,
        equip_qty,
        no_instance,
        by_player_qty,
    })
    .into_response()
}

#[derive(Serialize, sqlx::FromRow)]
struct DropLogRow {
    id: i64,
    ts: chrono::DateTime<chrono::Utc>,
    enemy_kind: i32,
    item_id: i32,
    qty: i32,
    rarity: i16,
    item_level: i32,
    refinement: i16,
}

#[derive(Deserialize)]
struct DropsLogQuery {
    limit: Option<i64>,
    item: Option<i32>,
    kind: Option<i32>,
    rarity: Option<i16>,
}

async fn report_drops(
    State(s): State<EconState>,
    headers: HeaderMap,
    Query(q): Query<DropsLogQuery>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    let limit = q.limit.unwrap_or(200).clamp(1, 2000);
    // Filtros opcionais — passamos NULL pra ignorar
    let rows: Vec<DropLogRow> = sqlx::query_as(
        "SELECT id, ts, enemy_kind, item_id, qty, rarity, item_level, refinement \
         FROM item_drops_log \
         WHERE ($1::int IS NULL OR item_id = $1) \
           AND ($2::int IS NULL OR enemy_kind = $2) \
           AND ($3::int IS NULL OR rarity = $3) \
         ORDER BY ts DESC \
         LIMIT $4",
    )
    .bind(q.item)
    .bind(q.kind)
    .bind(q.rarity)
    .bind(limit)
    .fetch_all(s.pool.as_ref())
    .await
    .unwrap_or_default();
    Json(serde_json::json!({"drops": rows})).into_response()
}

async fn icon_file(
    State(s): State<EconState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    // Sanitização: só A-Z a-z 0-9 _ - dot, sem path traversal.
    if name.is_empty()
        || name.len() > 128
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        || name.contains("..")
    {
        return bad("nome inválido");
    }
    let Some(dir) = s.icons_dir.as_ref().clone() else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "ICONS_DIR não configurado"})),
        )
            .into_response();
    };
    let mut path = dir.join(&name);
    if path.extension().is_none() {
        path.set_extension("png");
    }
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("{e}")})),
            )
                .into_response()
        }
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/png".to_string()),
            (header::CACHE_CONTROL, "public, max-age=300".to_string()),
        ],
        bytes,
    )
        .into_response()
}

// ── skills ───────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct SkillRow {
    id: i32,
    name: String,
    description: String,
    prof: String,
    tier: i16,
    is_passive: bool,
    path: Option<String>,
    unlock_char_lvl: i16,
    unlock_prof_lvl: i16,
    usable_with: Option<Vec<String>>,
    cost_mp: i32,
    cost_stamina: i32,
    cooldown_s: f32,
    cast_time_s: f32,
    target_type: String,
    range_tiles: f32,
    radius_tiles: f32,
    base_damage: i32,
    base_heal: i32,
    scaling_atk: f32,
    scaling_wis: f32,
    scaling_dex: f32,
    per_rank_dmg_pct: f32,
    per_rank_cd_pct: f32,
    per_rank_cost_pct: f32,
    icon_path: Option<String>,
    vfx_id: Option<String>,
    active: bool,
}

#[derive(Deserialize)]
struct SkillPayload {
    id: i32,
    name: String,
    description: String,
    prof: String,
    tier: i16,
    is_passive: bool,
    path: Option<String>,
    unlock_char_lvl: i16,
    unlock_prof_lvl: i16,
    /// `None` ou array vazio = todas as armas. Server filtra empty antes de
    /// gravar pra preservar `NULL` semantics no Postgres.
    usable_with: Option<Vec<String>>,
    cost_mp: i32,
    cost_stamina: i32,
    cooldown_s: f32,
    cast_time_s: f32,
    target_type: String,
    range_tiles: f32,
    radius_tiles: f32,
    base_damage: i32,
    base_heal: i32,
    scaling_atk: f32,
    scaling_wis: f32,
    scaling_dex: f32,
    per_rank_dmg_pct: f32,
    per_rank_cd_pct: f32,
    per_rank_cost_pct: f32,
    icon_path: Option<String>,
    vfx_id: Option<String>,
    #[serde(default = "default_true_payload")]
    active: bool,
}

async fn skills_list(State(s): State<EconState>, headers: HeaderMap) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    let rows: Vec<SkillRow> = match sqlx::query_as(
        "SELECT id, name, description, prof, tier, is_passive, path,
                unlock_char_lvl, unlock_prof_lvl, usable_with,
                cost_mp, cost_stamina, cooldown_s, cast_time_s,
                target_type, range_tiles, radius_tiles,
                base_damage, base_heal, scaling_atk, scaling_wis, scaling_dex,
                per_rank_dmg_pct, per_rank_cd_pct, per_rank_cost_pct,
                icon_path, vfx_id, active
         FROM skills ORDER BY prof, tier, is_passive, id",
    )
    .fetch_all(s.pool.as_ref())
    .await
    {
        Ok(r) => r,
        Err(e) => return ise(e),
    };
    Json(serde_json::json!({"skills": rows})).into_response()
}

async fn skills_upsert(
    State(s): State<EconState>,
    headers: HeaderMap,
    Json(p): Json<SkillPayload>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    if p.id <= 0 {
        return bad("id inválido");
    }
    if p.name.trim().is_empty() {
        return bad("nome vazio");
    }
    if !matches!(p.tier, 1..=4) {
        return bad("tier 1..4");
    }
    let valid_profs = [
        "Sword", "Axe", "Spear", "Dagger", "Bow", "Staff", "Wand", "Unarmed",
    ];
    if !valid_profs.contains(&p.prof.as_str()) {
        return bad("prof inválida");
    }
    let valid_targets = ["none", "self", "projectile", "cone", "aoe_circle", "line"];
    if !valid_targets.contains(&p.target_type.as_str()) {
        return bad("target_type inválido");
    }

    // Empty array → NULL (= "qualquer arma"). Postgres distingue, e o cliente
    // espera NULL pra significar "universal".
    let usable: Option<Vec<String>> =
        p.usable_with
            .and_then(|v| if v.is_empty() { None } else { Some(v) });

    if let Err(e) = sqlx::query(
        "INSERT INTO skills
         (id, name, description, prof, tier, is_passive, path,
          unlock_char_lvl, unlock_prof_lvl, usable_with,
          cost_mp, cost_stamina, cooldown_s, cast_time_s,
          target_type, range_tiles, radius_tiles,
          base_damage, base_heal, scaling_atk, scaling_wis, scaling_dex,
          per_rank_dmg_pct, per_rank_cd_pct, per_rank_cost_pct,
          icon_path, vfx_id, active)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                 $18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28)
         ON CONFLICT (id) DO UPDATE SET
            name = EXCLUDED.name, description = EXCLUDED.description,
            prof = EXCLUDED.prof, tier = EXCLUDED.tier,
            is_passive = EXCLUDED.is_passive, path = EXCLUDED.path,
            unlock_char_lvl = EXCLUDED.unlock_char_lvl,
            unlock_prof_lvl = EXCLUDED.unlock_prof_lvl,
            usable_with = EXCLUDED.usable_with,
            cost_mp = EXCLUDED.cost_mp, cost_stamina = EXCLUDED.cost_stamina,
            cooldown_s = EXCLUDED.cooldown_s, cast_time_s = EXCLUDED.cast_time_s,
            target_type = EXCLUDED.target_type,
            range_tiles = EXCLUDED.range_tiles, radius_tiles = EXCLUDED.radius_tiles,
            base_damage = EXCLUDED.base_damage, base_heal = EXCLUDED.base_heal,
            scaling_atk = EXCLUDED.scaling_atk, scaling_wis = EXCLUDED.scaling_wis,
            scaling_dex = EXCLUDED.scaling_dex,
            per_rank_dmg_pct = EXCLUDED.per_rank_dmg_pct,
            per_rank_cd_pct  = EXCLUDED.per_rank_cd_pct,
            per_rank_cost_pct= EXCLUDED.per_rank_cost_pct,
            icon_path = EXCLUDED.icon_path, vfx_id = EXCLUDED.vfx_id,
            active = EXCLUDED.active",
    )
    .bind(p.id)
    .bind(&p.name)
    .bind(&p.description)
    .bind(&p.prof)
    .bind(p.tier)
    .bind(p.is_passive)
    .bind(&p.path)
    .bind(p.unlock_char_lvl)
    .bind(p.unlock_prof_lvl)
    .bind(usable)
    .bind(p.cost_mp)
    .bind(p.cost_stamina)
    .bind(p.cooldown_s)
    .bind(p.cast_time_s)
    .bind(&p.target_type)
    .bind(p.range_tiles)
    .bind(p.radius_tiles)
    .bind(p.base_damage)
    .bind(p.base_heal)
    .bind(p.scaling_atk)
    .bind(p.scaling_wis)
    .bind(p.scaling_dex)
    .bind(p.per_rank_dmg_pct)
    .bind(p.per_rank_cd_pct)
    .bind(p.per_rank_cost_pct)
    .bind(&p.icon_path)
    .bind(&p.vfx_id)
    .bind(p.active)
    .execute(s.pool.as_ref())
    .await
    {
        return ise(e);
    }
    let v = bump_version(s.pool.as_ref()).await.unwrap_or(0);
    Json(serde_json::json!({"ok": true, "version": v})).into_response()
}

async fn skills_update(
    State(s): State<EconState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(mut p): Json<SkillPayload>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    p.id = id;
    skills_upsert(State(s), headers, Json(p)).await
}

async fn skills_delete(
    State(s): State<EconState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> Response {
    if !s.is_authed(&headers) {
        return unauth();
    }
    if let Err(e) = sqlx::query("DELETE FROM skills WHERE id = $1")
        .bind(id)
        .execute(s.pool.as_ref())
        .await
    {
        return ise(e);
    }
    let v = bump_version(s.pool.as_ref()).await.unwrap_or(0);
    Json(serde_json::json!({"ok": true, "version": v})).into_response()
}
