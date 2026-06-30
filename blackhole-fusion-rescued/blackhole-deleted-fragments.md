# Respaldo del código de fusión Blackhole borrado (2026-06-24)

`game-lab` NO está bajo git. Esto preserva el código de tiers/fusión que se borró por error.
El archivo completo `gameplay/blackhole.rs` está en `scratchpad/blackhole.rs`. Acá van los
fragmentos que vivían dentro de archivos compartidos.

---

## 1. `src/core/matching.rs` — bloque Blackhole (iba antes de `pick_most_common_color`)

```rust
// ─── Blackhole mode ───────────────────────────────────────────────────────────
//
// Blackhole reuses `detect_runs` but maps a match to a *tier token* instead of a `LightKind`
// power: `tier = pieces − 2`, clamped to 2..=5. Power tokens never detonate — they are fused by
// swapping two adjacent same-tier tokens (`resolve_blackhole_swap`). A run of exactly 3 just clears.

/// Outcome of scanning the board in Blackhole mode. `to_upgrade` hosts become `BlackholeTier`
/// tokens of the given tier (`horizontal` only matters for the tier-2 look); everything in
/// `to_remove` clears normally (including any token caught in a color match — it loses its tier).
pub(crate) struct BlackholeMatch {
    pub(crate) to_remove: HashSet<Entity>,
    pub(crate) to_upgrade: Vec<(Entity, u8, bool)>,
}

/// `tier = pieces − 2`, clamped to the token range 2..=5. Only ever called with `pieces ≥ 4`
/// (lines) or `≥ 5` (intersections); a 3-run clears without producing a token.
fn blackhole_tier(pieces: usize) -> u8 {
    pieces.saturating_sub(2).clamp(2, 5) as u8
}

pub(crate) fn scan_runs_blackhole(grid: &Grid, player_entity: Option<Entity>) -> BlackholeMatch {
    let (h_runs, v_runs) = detect_runs(grid);
    let mut to_remove: HashSet<Entity> = HashSet::new();
    let mut to_upgrade: Vec<(Entity, u8, bool)> = Vec::new();

    let mut entity_in_h: HashMap<Entity, usize> = HashMap::new();
    let mut entity_in_v: HashMap<Entity, usize> = HashMap::new();
    for (i, run) in h_runs.iter().enumerate() { for &e in run { entity_in_h.insert(e, i); } }
    for (i, run) in v_runs.iter().enumerate() { for &e in run { entity_in_v.insert(e, i); } }

    let mut handled_h: HashSet<usize> = HashSet::new();
    let mut handled_v: HashSet<usize> = HashSet::new();

    let intersections: Vec<(Entity, usize, usize)> = entity_in_h
        .iter()
        .filter_map(|(&e, &hi)| entity_in_v.get(&e).map(|&vi| (e, hi, vi)))
        .filter(|(_, hi, vi)| h_runs[*hi].len() >= 3 && v_runs[*vi].len() >= 3)
        .collect();

    for (e, hi, vi) in intersections {
        for &re in &h_runs[hi] { if re != e { to_remove.insert(re); } }
        for &re in &v_runs[vi] { if re != e { to_remove.insert(re); } }
        let pieces = h_runs[hi].len() + v_runs[vi].len() - 1; // shared cell counted once
        to_upgrade.push((e, blackhole_tier(pieces), true));
        handled_h.insert(hi);
        handled_v.insert(vi);
    }

    let remaining: Vec<(Vec<Entity>, bool)> = h_runs
        .iter().enumerate().filter(|(i, _)| !handled_h.contains(i)).map(|(_, r)| (r.clone(), true))
        .chain(v_runs.iter().enumerate().filter(|(i, _)| !handled_v.contains(i)).map(|(_, r)| (r.clone(), false)))
        .collect();

    for (entities, is_h) in &remaining {
        if entities.len() == 3 {
            to_remove.extend(entities);
        } else {
            let host = player_entity
                .and_then(|pe| entities.iter().find(|&&e| e == pe).copied())
                .unwrap_or(entities[entities.len() / 2]);
            for &e in entities { if e != host { to_remove.insert(e); } }
            to_upgrade.push((host, blackhole_tier(entities.len()), *is_h));
        }
    }

    to_upgrade.retain(|(e, _, _)| !to_remove.contains(e));
    BlackholeMatch { to_remove, to_upgrade }
}

/// Result of swapping two adjacent Blackhole tokens. Same tier ⇒ fuse to `tier+1`; two tier-5 ⇒
/// the Blackhole; anything else ⇒ the swap doesn't fuse (caller reverts it).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum BlackholeSwap {
    Fuse(u8),
    Blackhole,
    Invalid,
}

pub(crate) fn resolve_blackhole_swap(a: u8, b: u8) -> BlackholeSwap {
    if a != b { return BlackholeSwap::Invalid; }
    if a >= 5 { BlackholeSwap::Blackhole } else { BlackholeSwap::Fuse(a + 1) }
}
```

## 2. `src/core/matching.rs` — los 5 tests del módulo `tests`

```rust
    #[test]
    fn blackhole_line4_makes_tier2_horizontal() {
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        for x in 0..4 { put(&mut grid, &mut info, GridPos { x, y: 0 }, x as u32 + 1, LightKind::Normal); }
        let m = scan_runs_blackhole(&grid, None);
        assert_eq!(m.to_upgrade.len(), 1);
        assert_eq!(m.to_upgrade[0].1, 2);
        assert!(m.to_upgrade[0].2, "a horizontal run produces a horizontal tier-2");
    }

    #[test]
    fn blackhole_line5_makes_tier3() {
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        for x in 0..5 { put(&mut grid, &mut info, GridPos { x, y: 0 }, x as u32 + 1, LightKind::Normal); }
        let m = scan_runs_blackhole(&grid, None);
        assert_eq!(m.to_upgrade.len(), 1);
        assert_eq!(m.to_upgrade[0].1, 3);
    }

    #[test]
    fn blackhole_l_4_plus_3_makes_tier4() {
        // Horizontal arm of 4 (y=0, x 0..4) + vertical arm of 3 sharing the corner (0,0).
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        for x in 0..4 { put(&mut grid, &mut info, GridPos { x, y: 0 }, x as u32 + 1, LightKind::Normal); }
        put(&mut grid, &mut info, GridPos { x: 0, y: 1 }, 10, LightKind::Normal);
        put(&mut grid, &mut info, GridPos { x: 0, y: 2 }, 11, LightKind::Normal);
        let m = scan_runs_blackhole(&grid, None);
        let tier = m.to_upgrade.iter().find(|(en, _, _)| *en == e(1)).map(|(_, t, _)| *t);
        assert_eq!(tier, Some(4), "4 + 3 − 1 = 6 pieces → tier 4");
    }

    #[test]
    fn blackhole_t_5_plus_3_makes_tier5() {
        // Horizontal arm of 5 (y=0, x 0..5) + vertical arm of 3 sharing the center (2,0).
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        for x in 0..5 { put(&mut grid, &mut info, GridPos { x, y: 0 }, x as u32 + 1, LightKind::Normal); }
        put(&mut grid, &mut info, GridPos { x: 2, y: 1 }, 20, LightKind::Normal);
        put(&mut grid, &mut info, GridPos { x: 2, y: 2 }, 21, LightKind::Normal);
        let m = scan_runs_blackhole(&grid, None);
        let tier = m.to_upgrade.iter().find(|(en, _, _)| *en == e(3)).map(|(_, t, _)| *t);
        assert_eq!(tier, Some(5), "5 + 3 − 1 = 7 pieces → tier 5");
    }

    #[test]
    fn resolve_blackhole_swap_rules() {
        assert_eq!(resolve_blackhole_swap(2, 2), BlackholeSwap::Fuse(3));
        assert_eq!(resolve_blackhole_swap(4, 4), BlackholeSwap::Fuse(5));
        assert_eq!(resolve_blackhole_swap(5, 5), BlackholeSwap::Blackhole);
        assert_eq!(resolve_blackhole_swap(3, 4), BlackholeSwap::Invalid);
    }
```

## 3. `src/core/components.rs` — `BlackholeTier` (iba antes de `Shadow`)

```rust
/// Blackhole-mode only: the power tier a `Light` carries, read as its number of lightcores
/// (`tier` ∈ 2..=5; a plain light has no `BlackholeTier`). Created by 4+ matches
/// (`tier = pieces − 2`) and raised by fusing two same-tier lights. Classic mode never uses this
/// (it uses `LightKind` instead). `horizontal` only matters for the tier-2 "Haz" look (2 cores
/// playing along a row vs a column).
#[derive(Component, Clone, Copy)]
pub(crate) struct BlackholeTier {
    pub(crate) tier: u8,
    pub(crate) horizontal: bool,
}
```

## 4. `src/visuals/core_motion.rs` — `core_layout_blackhole` + `rebuild_blackhole_cores` (iban antes de `core_local`)

```rust
/// Core cluster for a Blackhole token: `tier` cores (2..=5), so the player reads the tier by
/// counting points of light. Tier-2 is the "Haz" — two cores in a line along its match axis.
fn core_layout_blackhole(tier: u8, horizontal: bool) -> Vec<CoreSpec> {
    match tier {
        2 => (0..2)
            .map(|i| {
                let off = (i as f32 - 0.5) * 2.0 * (TILE * 0.13); // -d, +d
                let base = if horizontal { Vec2::new(off, 0.0) } else { Vec2::new(0.0, off) };
                let pattern = if horizontal { CorePattern::LineH } else { CorePattern::LineV };
                CoreSpec { base, pattern, radius: 0.0 }
            })
            .collect(),
        3 => (0..3).map(|_| CoreSpec { base: Vec2::ZERO, pattern: CorePattern::RadialPulse, radius: TILE * 0.16 }).collect(),
        4 => (0..4).map(|_| CoreSpec { base: Vec2::ZERO, pattern: CorePattern::RadialPulse, radius: TILE * 0.17 }).collect(),
        _ => (0..5).map(|_| CoreSpec { base: Vec2::ZERO, pattern: CorePattern::Orbit, radius: TILE * 0.18 }).collect(),
    }
}

/// Blackhole sibling of `rebuild_cores`: rebuilds a token's cores whenever its `BlackholeTier`
/// changes (forged or fused). Keyed on `Changed<BlackholeTier>` so it never touches a Classic
/// light. Replaces the single normal core (built by `rebuild_cores` at spawn) with the tier cluster.
pub(crate) fn rebuild_blackhole_cores(
    mut commands: Commands,
    cache: Res<VisualCache>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    changed: Query<(Entity, &BlackholeTier, &LightColor, &BreathPhase, Option<&Children>), Changed<BlackholeTier>>,
    cores: Query<(), With<LightCore>>,
) {
    for (light, bt, color, phase, children) in &changed {
        if let Some(children) = children {
            for c in children.iter() {
                if cores.contains(c) { commands.entity(c).despawn(); }
            }
        }
        let core_mesh = if bt.tier >= 5 { &cache.star_core_mesh } else { &cache.core_mesh };
        let specs = core_layout_blackhole(bt.tier, bt.horizontal);
        let count = specs.len() as u8;
        for (i, spec) in specs.iter().enumerate() {
            let core = commands.spawn((
                LightCore,
                Breathing { base: color.glow_color(), phase: phase.0 + i as f32 * 0.25 },
                CoreMotion {
                    pattern: spec.pattern,
                    index: i as u8,
                    count,
                    phase: phase.0 + i as f32 * 0.8,
                    base: spec.base,
                    radius: spec.radius,
                },
                Mesh2d(core_mesh.clone()),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(color.glow_color()))),
                Transform::from_xyz(spec.base.x, spec.base.y, CORE_Z),
            )).id();
            commands.entity(light).add_child(core);
        }
    }
}
```

## Cómo se registraba (para restaurar el wiring)
- `gameplay/mod.rs`: `pub(crate) mod blackhole;`, `in_classic`/`in_blackhole` run-conditions,
  `.add_observer(blackhole::on_swap_happened_blackhole)`, `.add_observer(blackhole::on_blackhole_triggered)`,
  `chain::check_chain_matches.run_if(in_classic)`, `blackhole::check_chain_blackhole.run_if(in_blackhole)`.
- `gameplay/swap.rs`: guard `if *mode != GameMode::Classic { return; }` + param `mode: Res<GameMode>` + `GameMode` en el import.
- `gameplay/lifecycle.rs`: la rama Blackhole de `setup_match` usaba `moves.0 = u32::MAX` sin tocar `*level`.
- `visuals/mod.rs`: `core_motion::rebuild_blackhole_cores` en el tuple de `add_systems(Update, ...)`.
