export type Item = {
  id: number;
  name: string;
  sell_price: number;
  buy_price: number | null;
  shop_order: number | null;
  stack_max: number;
  equip_slot: string | null;
  item_level: number;
  icon_col: number;
  icon_row: number;
  icon_path: string | null;
  active: boolean;
  hp_min: number; hp_max: number;
  mp_min: number; mp_max: number;
  atk_min: number; atk_max: number;
  def_min: number; def_max: number;
  dex_min: number; dex_max: number;
  wis_min: number; wis_max: number;
};

export type Enemy = {
  kind: number;
  name: string;
  hp_max: number;
  speed: number;
  attack_damage: number;
  attack_cooldown: number;
  detect_range: number;
  attack_range: number;
  kite_dist: number | null;
  proj_count: number;
  xp_reward: number;
  defense: number;
  size_scale: number;
  tint_r: number; tint_g: number; tint_b: number; tint_a: number;
  loot_item_level: number | null;
};

export type Drop = {
  id: number;
  enemy_kind: number;
  item_id: number;
  qty_min: number;
  qty_max: number;
  chance: number;
};

export type FarmKind = 'Tree' | 'Rock' | 'Flower';

export type FarmDrop = {
  id: number;
  kind: FarmKind;
  tier: number;
  item_id: number;
  qty_min: number;
  qty_max: number;
  chance: number;
};

export const SLOTS = [
  '', 'Weapon', 'Offhand', 'Armor', 'Helm', 'Legs',
  'Boots', 'Gloves', 'Belt', 'Cape', 'Necklace', 'Ring',
] as const;

export type Skill = {
  id: number;
  name: string;
  description: string;
  prof: string;            // 'Sword'/'Axe'/'Spear'/'Dagger'/'Bow'/'Staff'/'Wand'/'Unarmed'
  tier: number;            // 1..4
  is_passive: boolean;
  path: string | null;
  unlock_char_lvl: number;
  unlock_prof_lvl: number;
  usable_with: string[] | null;  // [] / null = qualquer arma
  cost_mp: number;
  cost_stamina: number;
  cooldown_s: number;
  cast_time_s: number;
  target_type: string;     // 'self' | 'projectile' | 'cone' | 'aoe_circle' | 'line' | 'none'
  range_tiles: number;
  radius_tiles: number;
  base_damage: number;
  base_heal: number;
  scaling_atk: number;
  scaling_wis: number;
  scaling_dex: number;
  per_rank_dmg_pct: number;
  per_rank_cd_pct: number;
  per_rank_cost_pct: number;
  icon_path: string | null;
  vfx_id: string | null;
  active: boolean;
};

export const PROFS = ['Sword', 'Axe', 'Spear', 'Dagger', 'Bow', 'Staff', 'Wand', 'Unarmed'] as const;
export const TARGET_TYPES = ['none', 'self', 'projectile', 'cone', 'aoe_circle', 'line'] as const;
