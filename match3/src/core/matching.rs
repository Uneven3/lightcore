use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use super::grid::{GRID_H, GRID_W, GridPos};
use super::light::{LightColor, LightKind};

pub(crate) type Grid = HashMap<GridPos, (Entity, LightColor, LightKind)>;
pub(crate) type EntityInfo = HashMap<Entity, (GridPos, LightColor, LightKind)>;

/// Power light activation queue entry — fires its effect after the board refills.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PowerActivation {
    pub(crate) pos: GridPos,
    pub(crate) kind: LightKind,
    pub(crate) partner_color: Option<LightColor>,
}

pub(crate) fn resolve_swap_activation(
    a: Entity,
    a_pos: GridPos,
    a_kind: LightKind,
    b: Entity,
    b_pos: GridPos,
    b_kind: LightKind,
    grid: &Grid,
    entity_info: &EntityInfo,
) -> Option<HashSet<Entity>> {
    use LightKind::*;
    if a_kind == Normal && b_kind == Normal {
        return None;
    }

    let mut r: HashSet<Entity> = HashSet::new();
    r.insert(a);
    r.insert(b);

    match (a_kind, b_kind) {
        (RayH | RayV | Cross, RayH | RayV | Cross) => {
            for x in 0..GRID_W {
                if let Some(&(e, _, _)) = grid.get(&GridPos { x, y: a_pos.y }) {
                    r.insert(e);
                }
                if let Some(&(e, _, _)) = grid.get(&GridPos { x, y: b_pos.y }) {
                    r.insert(e);
                }
            }
            for y in 0..GRID_H {
                if let Some(&(e, _, _)) = grid.get(&GridPos { x: a_pos.x, y }) {
                    r.insert(e);
                }
                if let Some(&(e, _, _)) = grid.get(&GridPos { x: b_pos.x, y }) {
                    r.insert(e);
                }
            }
        }
        (RayH | RayV | Cross, Supernova) | (Supernova, RayH | RayV | Cross) => {
            let center = if a_kind == Supernova { a_pos } else { b_pos };
            for delta in -1..=1i32 {
                for x in 0..GRID_W {
                    if let Some(&(e, _, _)) = grid.get(&GridPos {
                        x,
                        y: center.y + delta,
                    }) {
                        r.insert(e);
                    }
                }
                for y in 0..GRID_H {
                    if let Some(&(e, _, _)) = grid.get(&GridPos {
                        x: center.x + delta,
                        y,
                    }) {
                        r.insert(e);
                    }
                }
            }
        }
        (Supernova, Supernova) => {
            for dx in -2..=2i32 {
                for dy in -2..=2i32 {
                    if let Some(&(e, _, _)) = grid.get(&GridPos {
                        x: a_pos.x + dx,
                        y: a_pos.y + dy,
                    }) {
                        r.insert(e);
                    }
                }
            }
        }
        (Starburst, RayH | RayV | Cross) | (RayH | RayV | Cross, Starburst) => {
            let stripe = if matches!(a_kind, RayH | RayV | Cross) {
                a_kind
            } else {
                b_kind
            };
            let partner_pos = if a_kind == Starburst { b_pos } else { a_pos };
            if let Some(&(_, color, _)) = grid.get(&partner_pos) {
                for (&e, (pos, c, _)) in entity_info {
                    if *c == color {
                        r.insert(e);
                        // Independent (not mutually exclusive) checks so Cross sweeps both axes
                        // while a plain Ray still only sweeps its own.
                        if matches!(stripe, RayH | Cross) {
                            for x in 0..GRID_W {
                                if let Some(&(re, _, _)) = grid.get(&GridPos { x, y: pos.y }) {
                                    r.insert(re);
                                }
                            }
                        }
                        if matches!(stripe, RayV | Cross) {
                            for y in 0..GRID_H {
                                if let Some(&(re, _, _)) = grid.get(&GridPos { x: pos.x, y }) {
                                    r.insert(re);
                                }
                            }
                        }
                    }
                }
            }
        }
        (Starburst, Supernova) | (Supernova, Starburst) => {
            let partner_pos = if a_kind == Starburst { b_pos } else { a_pos };
            if let Some(&(_, color, _)) = grid.get(&partner_pos) {
                for (&e, (pos, c, _)) in entity_info {
                    if *c == color {
                        r.insert(e);
                        for dx in -1..=1i32 {
                            for dy in -1..=1i32 {
                                if let Some(&(re, _, _)) = grid.get(&GridPos {
                                    x: pos.x + dx,
                                    y: pos.y + dy,
                                }) {
                                    r.insert(re);
                                }
                            }
                        }
                    }
                }
            }
        }
        (Starburst, Starburst) => {
            for &e in entity_info.keys() {
                r.insert(e);
            }
        }
        (Starburst, Normal) => {
            if let Some(&(_, color, _)) = grid.get(&b_pos) {
                for (&e, (_, c, _)) in entity_info {
                    if *c == color {
                        r.insert(e);
                    }
                }
            }
        }
        (Normal, Starburst) => {
            if let Some(&(_, color, _)) = grid.get(&a_pos) {
                for (&e, (_, c, _)) in entity_info {
                    if *c == color {
                        r.insert(e);
                    }
                }
            }
        }
        // Blackhole already clears everything on its own (`fire_single_activation`) — combined
        // with anything, the result is the same. Without this arm the swap would fall through to
        // `_ => return None` and revert as invalid, the same class of bug `Cross` had before its
        // combos were filled in.
        (Blackhole, _) | (_, Blackhole) => {
            for &e in entity_info.keys() {
                r.insert(e);
            }
        }
        _ => return None,
    }
    Some(r)
}

/// Names each power-vs-power interaction so the visual layer can play **one** unified animation
/// for the combination instead of one animation per participating power light. Mirrors the arms
/// of [`resolve_swap_activation`] one-to-one (see [`classify_combo`]); `SuperCombo` is the extra
/// case the callers raise themselves when 3+ powers detonate at once (it has no `resolve_swap_*`
/// arm — it clears the whole board).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum ComboKind {
    /// Ray/Cross × Ray/Cross — both rows and both columns.
    DoubleLine,
    /// Ray/Cross × Supernova — a 3-wide cross band.
    LineSupernova,
    /// Supernova × Supernova — a 5×5 burst.
    DoubleSupernova,
    /// Starburst × Ray/Cross — the partner's color, each swept along its line(s).
    StarLine,
    /// Starburst × Supernova — the partner's color, each as a 3×3.
    StarSupernova,
    /// Starburst × Starburst — the whole board.
    StarStar,
    /// Starburst × Normal — clears the partner's color.
    StarColor,
    /// Blackhole × anything — the whole board.
    Blackhole,
    /// 3+ powers detonating together — the whole board (raised by the caller, not a swap arm).
    SuperCombo,
}

/// Classifies a power-vs-power pair into its [`ComboKind`], mirroring the match arms of
/// [`resolve_swap_activation`] exactly: returns `Some` for precisely the pairs that combine
/// (and `None` for the pairs that revert as an invalid swap). The parity test
/// `classify_combo_matches_resolve_swap_activation` guards the two from drifting apart.
pub(crate) fn classify_combo(a_kind: LightKind, b_kind: LightKind) -> Option<ComboKind> {
    use LightKind::*;
    if a_kind == Normal && b_kind == Normal {
        return None;
    }
    Some(match (a_kind, b_kind) {
        (RayH | RayV | Cross, RayH | RayV | Cross) => ComboKind::DoubleLine,
        (RayH | RayV | Cross, Supernova) | (Supernova, RayH | RayV | Cross) => {
            ComboKind::LineSupernova
        }
        (Supernova, Supernova) => ComboKind::DoubleSupernova,
        (Starburst, RayH | RayV | Cross) | (RayH | RayV | Cross, Starburst) => ComboKind::StarLine,
        (Starburst, Supernova) | (Supernova, Starburst) => ComboKind::StarSupernova,
        (Starburst, Starburst) => ComboKind::StarStar,
        (Starburst, Normal) | (Normal, Starburst) => ComboKind::StarColor,
        (Blackhole, _) | (_, Blackhole) => ComboKind::Blackhole,
        _ => return None,
    })
}

/// The outcome of resolving a wave of power lights firing at once. `to_remove` is the union of
/// cells to clear; `combos` and `singles` partition the powers by how they fired so the caller
/// can play one unified animation per combined pair and the standard flash+beam per lone power.
pub(crate) struct WaveResolution {
    pub(crate) to_remove: HashSet<Entity>,
    /// Pairs that combined, with the interaction they formed. `(a, b, kind)`.
    pub(crate) combos: Vec<(PowerActivation, PowerActivation, ComboKind)>,
    /// Powers that fired on their own (no adjacent partner to combine with).
    pub(crate) singles: Vec<PowerActivation>,
}

/// Resolves a wave of power lights firing at the same time (a cascade or a drained chain queue).
/// Adjacent powers (Manhattan distance 1) **combine** via `resolve_swap_activation` — so
/// Starburst+Starburst clears the board, Supernova+Supernova bursts 5x5, Ray+Ray sweeps both
/// lines, etc., not only on a direct player swap. Every power not absorbed into a pair fires on
/// its own via `fire_single_activation`. Pure: the caller handles VFX triggers, scoring and
/// chain-reaction queueing from the returned [`WaveResolution`].
pub(crate) fn resolve_wave(
    powers: &[PowerActivation],
    grid: &Grid,
    entity_info: &EntityInfo,
) -> WaveResolution {
    let mut to_remove: HashSet<Entity> = HashSet::new();
    let mut combined: HashSet<GridPos> = HashSet::new(); // powers already merged into a pair
    let mut combos: Vec<(PowerActivation, PowerActivation, ComboKind)> = Vec::new();

    for i in 0..powers.len() {
        if combined.contains(&powers[i].pos) {
            continue;
        }
        for j in (i + 1)..powers.len() {
            if combined.contains(&powers[j].pos) {
                continue;
            }
            let (a, b) = (powers[i].pos, powers[j].pos);
            if (a.x - b.x).abs() + (a.y - b.y).abs() != 1 {
                continue;
            } // not adjacent
            let (Some(&(ea, _, ka)), Some(&(eb, _, kb))) = (grid.get(&a), grid.get(&b)) else {
                continue;
            };
            if let Some(set) = resolve_swap_activation(ea, a, ka, eb, b, kb, grid, entity_info) {
                to_remove.extend(set);
                combined.insert(a);
                combined.insert(b);
                if let Some(kind) = classify_combo(ka, kb) {
                    combos.push((powers[i], powers[j], kind));
                }
                break; // `i` is paired; move to the next unpaired power
            }
        }
    }
    let mut singles: Vec<PowerActivation> = Vec::new();
    for p in powers {
        if combined.contains(&p.pos) {
            continue;
        }
        to_remove.extend(fire_single_activation(p, grid, entity_info));
        singles.push(*p);
    }
    WaveResolution {
        to_remove,
        combos,
        singles,
    }
}

pub(crate) struct MatchResult {
    pub(crate) to_remove: HashSet<Entity>,
    pub(crate) to_upgrade: Vec<(Entity, LightKind)>,
    /// Power lights that already occupied a cell chosen to host a newly created upgrade —
    /// the caller must fire their effect (excluding the host, which survives) before applying
    /// the upgrade in place.
    pub(crate) replaced_powers: Vec<PowerActivation>,
}

/// All horizontal and vertical same-color runs of length ≥3 on the board, as `(h_runs, v_runs)`.
/// Used by `scan_runs` to find runs before mapping each to its outcome.
fn detect_runs(grid: &Grid) -> (Vec<Vec<Entity>>, Vec<Vec<Entity>>) {
    let mut h_runs: Vec<Vec<Entity>> = Vec::new();
    let mut v_runs: Vec<Vec<Entity>> = Vec::new();

    for y in 0..GRID_H {
        let mut x = 0;
        while x < GRID_W {
            let Some(&(_, color, _)) = grid.get(&GridPos { x, y }) else {
                x += 1;
                continue;
            };
            let mut run = vec![grid[&GridPos { x, y }].0];
            loop {
                let nx = x + run.len() as i32;
                if nx >= GRID_W {
                    break;
                }
                match grid.get(&GridPos { x: nx, y }) {
                    Some(&(e, c, _)) if c == color => run.push(e),
                    _ => break,
                }
            }
            let len = run.len() as i32;
            if run.len() >= 3 {
                h_runs.push(run);
            }
            x += len;
        }
    }

    for x in 0..GRID_W {
        let mut y = 0;
        while y < GRID_H {
            let Some(&(_, color, _)) = grid.get(&GridPos { x, y }) else {
                y += 1;
                continue;
            };
            let mut run = vec![grid[&GridPos { x, y }].0];
            loop {
                let ny = y + run.len() as i32;
                if ny >= GRID_H {
                    break;
                }
                match grid.get(&GridPos { x, y: ny }) {
                    Some(&(e, c, _)) if c == color => run.push(e),
                    _ => break,
                }
            }
            let len = run.len() as i32;
            if run.len() >= 3 {
                v_runs.push(run);
            }
            y += len;
        }
    }

    (h_runs, v_runs)
}

pub(crate) fn scan_runs(
    grid: &Grid,
    entity_info: &EntityInfo,
    player_entity: Option<Entity>,
) -> MatchResult {
    let (h_runs, v_runs) = detect_runs(grid);

    let mut to_remove: HashSet<Entity> = HashSet::new();
    let mut upgrade_map: HashMap<Entity, LightKind> = HashMap::new();
    let mut replaced_powers: Vec<PowerActivation> = Vec::new();

    let mut entity_in_h: HashMap<Entity, usize> = HashMap::new();
    let mut entity_in_v: HashMap<Entity, usize> = HashMap::new();
    for (i, run) in h_runs.iter().enumerate() {
        for &e in run {
            entity_in_h.insert(e, i);
        }
    }
    for (i, run) in v_runs.iter().enumerate() {
        for &e in run {
            entity_in_v.insert(e, i);
        }
    }

    let mut handled_h: HashSet<usize> = HashSet::new();
    let mut handled_v: HashSet<usize> = HashSet::new();

    let intersections: Vec<(Entity, usize, usize)> = entity_in_h
        .iter()
        .filter_map(|(&e, &hi)| entity_in_v.get(&e).map(|&vi| (e, hi, vi)))
        .filter(|(_, hi, vi)| h_runs[*hi].len() >= 3 && v_runs[*vi].len() >= 3)
        .collect();

    for (e, hi, vi) in intersections {
        for &re in &h_runs[hi] {
            if re != e {
                to_remove.insert(re);
            }
        }
        for &re in &v_runs[vi] {
            if re != e {
                to_remove.insert(re);
            }
        }
        if let Some((pos, _, kind)) = entity_info.get(&e)
            && *kind != LightKind::Normal
        {
            replaced_powers.push(PowerActivation {
                pos: *pos,
                kind: *kind,
                partner_color: None,
            });
        }
        // Corner ("L") vs T/+ is whether the shared cell sits at an endpoint of BOTH runs, or
        // mid-run on at least one — see `LightKind::from_intersection`.
        let h_len = h_runs[hi].len();
        let v_len = v_runs[vi].len();
        let h_idx = h_runs[hi].iter().position(|&x| x == e).unwrap();
        let v_idx = v_runs[vi].iter().position(|&x| x == e).unwrap();
        let is_corner = (h_idx == 0 || h_idx == h_len - 1) && (v_idx == 0 || v_idx == v_len - 1);
        upgrade_map
            .entry(e)
            .or_insert(LightKind::from_intersection(h_len, v_len, is_corner));
        handled_h.insert(hi);
        handled_v.insert(vi);
    }

    let remaining: Vec<(Vec<Entity>, bool)> = h_runs
        .iter()
        .enumerate()
        .filter(|(i, _)| !handled_h.contains(i))
        .map(|(_, r)| (r.clone(), true))
        .chain(
            v_runs
                .iter()
                .enumerate()
                .filter(|(i, _)| !handled_v.contains(i))
                .map(|(_, r)| (r.clone(), false)),
        )
        .collect();

    for (entities, is_h) in &remaining {
        // A run of exactly 3 just clears; 4+ forges a power whose kind is set by `from_line`.
        if entities.len() == 3 {
            to_remove.extend(entities);
            continue;
        }
        let upgrade = player_entity
            .and_then(|pe| entities.iter().find(|&&e| e == pe).copied())
            .unwrap_or(entities[entities.len() / 2]);
        for &e in entities {
            if e != upgrade {
                to_remove.insert(e);
            }
        }
        if let Some((pos, _, kind)) = entity_info.get(&upgrade)
            && *kind != LightKind::Normal
        {
            replaced_powers.push(PowerActivation {
                pos: *pos,
                kind: *kind,
                partner_color: None,
            });
        }
        upgrade_map
            .entry(upgrade)
            .or_insert(LightKind::from_line(entities.len(), *is_h));
    }

    let to_upgrade = upgrade_map
        .into_iter()
        .filter(|(e, _)| !to_remove.contains(e))
        .collect();
    MatchResult {
        to_remove,
        to_upgrade,
        replaced_powers,
    }
}

pub(crate) fn pick_most_common_color(entity_info: &EntityInfo) -> LightColor {
    let mut counts = [0u32; 5];
    for (_, c, _) in entity_info.values() {
        let idx = match c {
            LightColor::Red => 0,
            LightColor::Green => 1,
            LightColor::Blue => 2,
            LightColor::Yellow => 3,
            LightColor::Purple => 4,
        };
        counts[idx] += 1;
    }
    LightColor::from_index(
        counts
            .iter()
            .enumerate()
            .max_by_key(|&(_, &v)| v)
            .map(|(i, _)| i)
            .unwrap_or(0),
    )
}

/// Computes the set of entities to remove when a single power light activates.
/// Uses the current grid state (called after refill, so the board has new lights).
pub(crate) fn fire_single_activation(
    activation: &PowerActivation,
    grid: &Grid,
    entity_info: &EntityInfo,
) -> HashSet<Entity> {
    let mut r = HashSet::new();
    let pos = activation.pos;
    match activation.kind {
        LightKind::RayH => {
            for x in 0..GRID_W {
                if let Some(&(e, _, _)) = grid.get(&GridPos { x, y: pos.y }) {
                    r.insert(e);
                }
            }
        }
        LightKind::RayV => {
            for y in 0..GRID_H {
                if let Some(&(e, _, _)) = grid.get(&GridPos { x: pos.x, y }) {
                    r.insert(e);
                }
            }
        }
        LightKind::Supernova => {
            for dx in -1..=1i32 {
                for dy in -1..=1i32 {
                    if let Some(&(e, _, _)) = grid.get(&GridPos {
                        x: pos.x + dx,
                        y: pos.y + dy,
                    }) {
                        r.insert(e);
                    }
                }
            }
        }
        LightKind::Cross => {
            for x in 0..GRID_W {
                if let Some(&(e, _, _)) = grid.get(&GridPos { x, y: pos.y }) {
                    r.insert(e);
                }
            }
            for y in 0..GRID_H {
                if let Some(&(e, _, _)) = grid.get(&GridPos { x: pos.x, y }) {
                    r.insert(e);
                }
            }
        }
        LightKind::Starburst => {
            let color = activation
                .partner_color
                .unwrap_or_else(|| pick_most_common_color(entity_info));
            for (&e, (_, c, _)) in entity_info {
                if *c == color {
                    r.insert(e);
                }
            }
        }
        LightKind::Blackhole => {
            for &e in entity_info.keys() {
                r.insert(e);
            }
        }
        LightKind::Normal => {}
    }
    r
}

/// Cells a single power-light activation sweeps through, in travel order — drives the
/// `TravelingLight` visual. Unlike `fire_single_activation` (unordered `HashSet`, used for
/// fast set-membership by gameplay resolution), this is presentation-only and does its own walk.
pub(crate) fn blast_path(activation: &PowerActivation, entity_info: &EntityInfo) -> Vec<GridPos> {
    let pos = activation.pos;
    match activation.kind {
        LightKind::RayH => {
            let mut path = vec![pos];
            let mut step = 1;
            loop {
                let mut any = false;
                if pos.x + step < GRID_W {
                    path.push(GridPos {
                        x: pos.x + step,
                        y: pos.y,
                    });
                    any = true;
                }
                if pos.x - step >= 0 {
                    path.push(GridPos {
                        x: pos.x - step,
                        y: pos.y,
                    });
                    any = true;
                }
                if !any {
                    break;
                }
                step += 1;
            }
            path
        }
        LightKind::RayV => {
            let mut path = vec![pos];
            let mut step = 1;
            loop {
                let mut any = false;
                if pos.y + step < GRID_H {
                    path.push(GridPos {
                        x: pos.x,
                        y: pos.y + step,
                    });
                    any = true;
                }
                if pos.y - step >= 0 {
                    path.push(GridPos {
                        x: pos.x,
                        y: pos.y - step,
                    });
                    any = true;
                }
                if !any {
                    break;
                }
                step += 1;
            }
            path
        }
        LightKind::Supernova => {
            let mut path = vec![pos];
            for dx in -1..=1i32 {
                for dy in -1..=1i32 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    path.push(GridPos {
                        x: pos.x + dx,
                        y: pos.y + dy,
                    });
                }
            }
            path
        }
        LightKind::Cross => {
            // Both arms sweep outward from the centre together (row + column).
            let mut path = vec![pos];
            let mut step = 1;
            loop {
                let mut any = false;
                if pos.x + step < GRID_W {
                    path.push(GridPos {
                        x: pos.x + step,
                        y: pos.y,
                    });
                    any = true;
                }
                if pos.x - step >= 0 {
                    path.push(GridPos {
                        x: pos.x - step,
                        y: pos.y,
                    });
                    any = true;
                }
                if pos.y + step < GRID_H {
                    path.push(GridPos {
                        x: pos.x,
                        y: pos.y + step,
                    });
                    any = true;
                }
                if pos.y - step >= 0 {
                    path.push(GridPos {
                        x: pos.x,
                        y: pos.y - step,
                    });
                    any = true;
                }
                if !any {
                    break;
                }
                step += 1;
            }
            path
        }
        LightKind::Starburst => {
            // Origin (the star) first, then the target lights sorted by distance — so the visual
            // can fire one seeking beam from the star to each target.
            let color = activation
                .partner_color
                .unwrap_or_else(|| pick_most_common_color(entity_info));
            let mut targets: Vec<GridPos> = entity_info
                .values()
                .filter(|(_, c, _)| *c == color)
                .map(|(p, _, _)| *p)
                .collect();
            targets.sort_by_key(|p| (p.x - pos.x).pow(2) + (p.y - pos.y).pow(2));
            let mut path = vec![pos];
            path.extend(targets);
            path
        }
        LightKind::Blackhole => {
            // Every light on the board, sorted by distance from the origin — same shape as
            // Starburst's target list, just unfiltered by color. Drives the collapsing-wave
            // travelling beam (`light_trail`) and, for free, the per-light pop stagger in
            // `popping::accumulate_pop_delays` (its generic `_` arm already propagates by
            // distance along this path).
            let mut cells: Vec<GridPos> = entity_info.values().map(|(p, _, _)| *p).collect();
            cells.sort_by_key(|p| (p.x - pos.x).pow(2) + (p.y - pos.y).pow(2));
            cells
        }
        LightKind::Normal => vec![],
    }
}

fn has_match(board: &[[Option<LightColor>; GRID_H as usize]; GRID_W as usize]) -> bool {
    // Check horizontal runs
    for y in 0..GRID_H as usize {
        let mut run_len = 1;
        let mut last_color = None;
        for x in 0..GRID_W as usize {
            let color = board[x][y];
            if color.is_some() && color == last_color {
                run_len += 1;
                if run_len >= 3 {
                    return true;
                }
            } else {
                run_len = 1;
                last_color = color;
            }
        }
    }
    // Check vertical runs
    for x in 0..GRID_W as usize {
        let mut run_len = 1;
        let mut last_color = None;
        for y in 0..GRID_H as usize {
            let color = board[x][y];
            if color.is_some() && color == last_color {
                run_len += 1;
                if run_len >= 3 {
                    return true;
                }
            } else {
                run_len = 1;
                last_color = color;
            }
        }
    }
    false
}

pub(crate) fn find_valid_swap(
    grid: &Grid,
    shadow: &HashSet<GridPos>,
) -> Option<(GridPos, GridPos)> {
    let mut board = [[None; GRID_H as usize]; GRID_W as usize];
    for (pos, &(_, color, _)) in grid {
        if (0..GRID_W).contains(&pos.x) && (0..GRID_H).contains(&pos.y) && !shadow.contains(pos) {
            board[pos.x as usize][pos.y as usize] = Some(color);
        }
    }

    for y in 0..GRID_H as usize {
        for x in 0..GRID_W as usize {
            for (dx, dy) in [(1, 0), (0, 1)] {
                let nx = x + dx;
                let ny = y + dy;
                if nx >= GRID_W as usize || ny >= GRID_H as usize {
                    continue;
                }
                let color_a = board[x][y];
                let color_b = board[nx][ny];
                if color_a.is_none() || color_b.is_none() {
                    continue;
                }

                board[x][y] = color_b;
                board[nx][ny] = color_a;

                let matches = has_match(&board);

                board[x][y] = color_a;
                board[nx][ny] = color_b;

                if matches {
                    return Some((
                        GridPos {
                            x: x as i32,
                            y: y as i32,
                        },
                        GridPos {
                            x: nx as i32,
                            y: ny as i32,
                        },
                    ));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(n: u32) -> Entity {
        Entity::from_raw_u32(n).unwrap()
    }

    #[test]
    fn fire_single_activation_includes_its_own_cell() {
        let pos = GridPos { x: 3, y: 2 };
        let activator = e(1);
        let mut grid: Grid = HashMap::new();
        grid.insert(pos, (activator, LightColor::Red, LightKind::RayH));
        let entity_info: EntityInfo = HashMap::new();

        let activation = PowerActivation {
            pos,
            kind: LightKind::RayH,
            partner_color: None,
        };
        let result = fire_single_activation(&activation, &grid, &entity_info);

        // Documents the behavior that caused the FASE 1 self-requeue bug: a blast's own
        // cell is always part of its own result, since the snapshot is taken before any
        // removal happens — callers must exclude activators explicitly, not assume otherwise.
        assert!(result.contains(&activator));
    }

    #[test]
    fn scan_runs_creates_upgrade_without_player_entity() {
        let mut grid: Grid = HashMap::new();
        let mut entity_info: EntityInfo = HashMap::new();
        for x in 0..4 {
            let ent = e(x as u32 + 1);
            let pos = GridPos { x, y: 0 };
            grid.insert(pos, (ent, LightColor::Red, LightKind::Normal));
            entity_info.insert(ent, (pos, LightColor::Red, LightKind::Normal));
        }

        let result = scan_runs(&grid, &entity_info, None);

        assert!(
            !result.to_upgrade.is_empty(),
            "a 4-in-a-row should always produce an upgrade, even with no player_entity"
        );
        assert_eq!(result.to_upgrade[0].1, LightKind::RayH);
    }

    #[test]
    fn scan_runs_l_corner_3_plus_3_makes_cross() {
        // Shared cell (0,0) sits at the endpoint of BOTH the 3-run (going right) and the 3-run
        // (going down) — a corner, an "L" — so it forges a Cross, not a Supernova.
        let mut grid: Grid = HashMap::new();
        let mut entity_info: EntityInfo = HashMap::new();
        let cells = [
            (GridPos { x: 0, y: 0 }, 1u32), // shared by both runs
            (GridPos { x: 1, y: 0 }, 2),
            (GridPos { x: 2, y: 0 }, 3),
            (GridPos { x: 0, y: 1 }, 4),
            (GridPos { x: 0, y: 2 }, 5),
        ];
        for (pos, n) in cells {
            let ent = e(n);
            grid.insert(pos, (ent, LightColor::Red, LightKind::Normal));
            entity_info.insert(ent, (pos, LightColor::Red, LightKind::Normal));
        }
        let shared = e(1);

        let result = scan_runs(&grid, &entity_info, None);

        let upgrade_kind = result
            .to_upgrade
            .iter()
            .find(|(ent, _)| *ent == shared)
            .map(|(_, k)| *k);
        assert_eq!(upgrade_kind, Some(LightKind::Cross));
    }

    #[test]
    fn scan_runs_t_intersection_makes_supernova() {
        // Horizontal 3-run at y=0 (x: 0,1,2); vertical 3-run at x=1 (y: -1..1 relative, i.e.
        // 0,1,2), sharing (1,0) — which is the MIDDLE of the horizontal run (not an endpoint), so
        // this is a T/+, not an L: it forges a Supernova.
        let mut grid: Grid = HashMap::new();
        let mut entity_info: EntityInfo = HashMap::new();
        let cells = [
            (GridPos { x: 0, y: 0 }, 1u32),
            (GridPos { x: 1, y: 0 }, 2), // shared — middle of the horizontal run
            (GridPos { x: 2, y: 0 }, 3),
            (GridPos { x: 1, y: 1 }, 4),
            (GridPos { x: 1, y: 2 }, 5),
        ];
        for (pos, n) in cells {
            let ent = e(n);
            grid.insert(pos, (ent, LightColor::Red, LightKind::Normal));
            entity_info.insert(ent, (pos, LightColor::Red, LightKind::Normal));
        }
        let shared = e(2);

        let result = scan_runs(&grid, &entity_info, None);

        let upgrade_kind = result
            .to_upgrade
            .iter()
            .find(|(ent, _)| *ent == shared)
            .map(|(_, k)| *k);
        assert_eq!(upgrade_kind, Some(LightKind::Supernova));
    }

    #[test]
    fn scan_runs_straight_line_5_makes_starburst() {
        // A clean line-5 with NO intersection — must forge a Starburst directly, not a Supernova.
        let mut grid: Grid = HashMap::new();
        let mut entity_info: EntityInfo = HashMap::new();
        for x in 0..5 {
            let ent = e(x as u32 + 1);
            let pos = GridPos { x, y: 0 };
            grid.insert(pos, (ent, LightColor::Red, LightKind::Normal));
            entity_info.insert(ent, (pos, LightColor::Red, LightKind::Normal));
        }

        let result = scan_runs(&grid, &entity_info, None);

        assert_eq!(result.to_upgrade.len(), 1);
        assert_eq!(result.to_upgrade[0].1, LightKind::Starburst);
    }

    #[test]
    fn scan_runs_five_line_crossing_three_makes_blackhole() {
        // A horizontal 5-run at y=2 crossing a vertical 3-run at x=2 (shared cell (2,2)) — the
        // "5+3" the user called the theoretical max. An arm that's already Starburst-worthy AND
        // crosses another run forges the ultimate Blackhole, not a plain Starburst.
        let mut grid: Grid = HashMap::new();
        let mut entity_info: EntityInfo = HashMap::new();
        let cells = [
            (GridPos { x: 0, y: 2 }, 1u32),
            (GridPos { x: 1, y: 2 }, 2),
            (GridPos { x: 2, y: 2 }, 3), // shared by the 5-run and the 3-run
            (GridPos { x: 3, y: 2 }, 4),
            (GridPos { x: 4, y: 2 }, 5),
            (GridPos { x: 2, y: 1 }, 6),
            (GridPos { x: 2, y: 3 }, 7),
        ];
        for (pos, n) in cells {
            let ent = e(n);
            grid.insert(pos, (ent, LightColor::Red, LightKind::Normal));
            entity_info.insert(ent, (pos, LightColor::Red, LightKind::Normal));
        }
        let shared = e(3);

        let result = scan_runs(&grid, &entity_info, None);

        let upgrade_kind = result
            .to_upgrade
            .iter()
            .find(|(ent, _)| *ent == shared)
            .map(|(_, k)| *k);
        assert_eq!(upgrade_kind, Some(LightKind::Blackhole));
    }

    #[test]
    fn fire_single_activation_blackhole_clears_every_color() {
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        let center = put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 3 },
            1,
            LightKind::Blackhole,
        );
        let red = put(
            &mut grid,
            &mut info,
            GridPos { x: 0, y: 0 },
            2,
            LightKind::Normal,
        );
        let blue = e(3);
        let blue_pos = GridPos { x: 7, y: 7 };
        grid.insert(blue_pos, (blue, LightColor::Blue, LightKind::Normal));
        info.insert(blue, (blue_pos, LightColor::Blue, LightKind::Normal));

        let activation = PowerActivation {
            pos: GridPos { x: 3, y: 3 },
            kind: LightKind::Blackhole,
            partner_color: None,
        };
        let removed = fire_single_activation(&activation, &grid, &info);

        assert!(removed.contains(&center) && removed.contains(&red) && removed.contains(&blue));
    }

    #[test]
    fn blast_path_starts_at_activator() {
        let pos = GridPos { x: 3, y: 2 };
        let activation = PowerActivation {
            pos,
            kind: LightKind::RayH,
            partner_color: None,
        };
        let entity_info: EntityInfo = HashMap::new();

        let path = blast_path(&activation, &entity_info);

        assert_eq!(path[0], pos);
    }

    #[test]
    fn blast_path_ray_h_stays_in_bounds() {
        let pos = GridPos { x: 0, y: 4 };
        let activation = PowerActivation {
            pos,
            kind: LightKind::RayH,
            partner_color: None,
        };
        let entity_info: EntityInfo = HashMap::new();

        let path = blast_path(&activation, &entity_info);

        assert!(
            path.iter()
                .all(|p| p.x >= 0 && p.x < GRID_W && p.y == pos.y)
        );
        assert_eq!(path.len(), GRID_W as usize);
    }

    fn put(
        grid: &mut Grid,
        info: &mut EntityInfo,
        pos: GridPos,
        n: u32,
        kind: LightKind,
    ) -> Entity {
        let ent = e(n);
        grid.insert(pos, (ent, LightColor::Red, kind));
        info.insert(ent, (pos, LightColor::Red, kind));
        ent
    }

    #[test]
    fn resolve_wave_adjacent_starbursts_clear_everything() {
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        let s1 = put(
            &mut grid,
            &mut info,
            GridPos { x: 0, y: 0 },
            1,
            LightKind::Starburst,
        );
        let s2 = put(
            &mut grid,
            &mut info,
            GridPos { x: 1, y: 0 },
            2,
            LightKind::Starburst,
        );
        let n3 = put(
            &mut grid,
            &mut info,
            GridPos { x: 5, y: 5 },
            3,
            LightKind::Normal,
        );

        let powers = [
            PowerActivation {
                pos: GridPos { x: 0, y: 0 },
                kind: LightKind::Starburst,
                partner_color: None,
            },
            PowerActivation {
                pos: GridPos { x: 1, y: 0 },
                kind: LightKind::Starburst,
                partner_color: None,
            },
        ];
        let removed = resolve_wave(&powers, &grid, &info).to_remove;

        // Star+Star adjacent ⇒ combined "clear the whole board" — even a far-away normal light.
        assert!(removed.contains(&s1) && removed.contains(&s2) && removed.contains(&n3));
    }

    #[test]
    fn resolve_wave_adjacent_rays_sweep_both_columns() {
        // Two adjacent RayH: combined they clear their shared row AND both columns; fired alone
        // each would only clear the row. The off-row column cells prove the combo happened.
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 3 },
            1,
            LightKind::RayH,
        );
        put(
            &mut grid,
            &mut info,
            GridPos { x: 4, y: 3 },
            2,
            LightKind::RayH,
        );
        let off_col3 = put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 5 },
            3,
            LightKind::Normal,
        );
        let off_col4 = put(
            &mut grid,
            &mut info,
            GridPos { x: 4, y: 6 },
            4,
            LightKind::Normal,
        );

        let powers = [
            PowerActivation {
                pos: GridPos { x: 3, y: 3 },
                kind: LightKind::RayH,
                partner_color: None,
            },
            PowerActivation {
                pos: GridPos { x: 4, y: 3 },
                kind: LightKind::RayH,
                partner_color: None,
            },
        ];
        let removed = resolve_wave(&powers, &grid, &info).to_remove;

        assert!(removed.contains(&off_col3) && removed.contains(&off_col4));
    }

    #[test]
    fn resolve_wave_non_adjacent_rays_fire_individually() {
        // Same two RayH but two cells apart ⇒ NOT combined ⇒ only their shared row clears, the
        // off-row column cells survive.
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 3 },
            1,
            LightKind::RayH,
        );
        put(
            &mut grid,
            &mut info,
            GridPos { x: 5, y: 3 },
            2,
            LightKind::RayH,
        );
        let off_col3 = put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 5 },
            3,
            LightKind::Normal,
        );

        let powers = [
            PowerActivation {
                pos: GridPos { x: 3, y: 3 },
                kind: LightKind::RayH,
                partner_color: None,
            },
            PowerActivation {
                pos: GridPos { x: 5, y: 3 },
                kind: LightKind::RayH,
                partner_color: None,
            },
        ];
        let removed = resolve_wave(&powers, &grid, &info).to_remove;

        assert!(!removed.contains(&off_col3));
    }

    #[test]
    fn classify_combo_matches_resolve_swap_activation() {
        // `classify_combo` must return `Some` for exactly the pairs that combine in
        // `resolve_swap_activation` (and `None` for the ones that revert) — the visual layer
        // relies on this parity so every valid combo gets a unified animation and no invalid
        // swap fakes one. Guards the two `match`es from drifting apart.
        use LightKind::*;
        let kinds = [Normal, RayH, RayV, Supernova, Cross, Starburst, Blackhole];
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        for (i, &k) in kinds.iter().enumerate() {
            put(
                &mut grid,
                &mut info,
                GridPos { x: i as i32, y: 0 },
                i as u32 + 1,
                k,
            );
        }
        for (i, &ka) in kinds.iter().enumerate() {
            for (j, &kb) in kinds.iter().enumerate() {
                let (ea, eb) = (e(i as u32 + 1), e(j as u32 + 1));
                let (pa, pb) = (GridPos { x: i as i32, y: 0 }, GridPos { x: j as i32, y: 0 });
                let resolves =
                    resolve_swap_activation(ea, pa, ka, eb, pb, kb, &grid, &info).is_some();
                assert_eq!(
                    classify_combo(ka, kb).is_some(),
                    resolves,
                    "classify_combo disagrees with resolve_swap_activation for ({ka:?}, {kb:?})"
                );
            }
        }
    }

    // ─── Canonical tier table (forma → kind via `from_line` / `from_intersection`) ──────────────

    #[test]
    fn from_line_maps_each_case() {
        assert_eq!(LightKind::from_line(4, true), LightKind::RayH);
        assert_eq!(LightKind::from_line(4, false), LightKind::RayV);
        assert_eq!(LightKind::from_line(5, true), LightKind::Starburst);
        assert_eq!(LightKind::from_line(8, true), LightKind::Starburst);
    }

    #[test]
    fn from_intersection_shape_sets_the_baseline() {
        assert_eq!(
            LightKind::from_intersection(3, 3, false),
            LightKind::Supernova
        ); // T/+
        assert_eq!(LightKind::from_intersection(3, 3, true), LightKind::Cross); // L corner
        assert_eq!(LightKind::from_intersection(4, 3, true), LightKind::Cross);
    }

    #[test]
    fn from_intersection_big_match_still_escalates() {
        // A T (not a corner) with enough total pieces still climbs past Supernova — a big match
        // should never feel weaker than a small one of the same shape.
        assert_eq!(LightKind::from_intersection(4, 3, false), LightKind::Cross);
    }

    #[test]
    fn from_intersection_starburst_worthy_arm_makes_blackhole() {
        // An arm that's already a Starburst on its own (5+), crossing another run, is the rarest
        // shape on the board — it forges a Blackhole, more potent than a plain Starburst.
        // Corner-ness doesn't matter once an arm reaches 5.
        assert_eq!(
            LightKind::from_intersection(5, 3, false),
            LightKind::Blackhole
        );
        assert_eq!(
            LightKind::from_intersection(5, 3, true),
            LightKind::Blackhole
        );
        assert_eq!(
            LightKind::from_intersection(3, 5, false),
            LightKind::Blackhole
        );
    }

    #[test]
    fn scan_runs_l_4_plus_3_makes_cross() {
        // Horizontal arm of 4 (y=0, x 0..4) + vertical arm of 3 sharing the corner (0,0) = 6
        // pieces → tier 4 → Cross.
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        for x in 0..4 {
            put(
                &mut grid,
                &mut info,
                GridPos { x, y: 0 },
                x as u32 + 1,
                LightKind::Normal,
            );
        }
        put(
            &mut grid,
            &mut info,
            GridPos { x: 0, y: 1 },
            10,
            LightKind::Normal,
        );
        put(
            &mut grid,
            &mut info,
            GridPos { x: 0, y: 2 },
            11,
            LightKind::Normal,
        );
        let result = scan_runs(&grid, &info, None);
        let kind = result
            .to_upgrade
            .iter()
            .find(|(en, _)| *en == e(1))
            .map(|(_, k)| *k);
        assert_eq!(kind, Some(LightKind::Cross));
    }

    #[test]
    fn cross_sweeps_row_and_column() {
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        let center = put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 3 },
            1,
            LightKind::Cross,
        );
        let same_row = put(
            &mut grid,
            &mut info,
            GridPos { x: 7, y: 3 },
            2,
            LightKind::Normal,
        );
        let same_col = put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 7 },
            3,
            LightKind::Normal,
        );
        let off_both = put(
            &mut grid,
            &mut info,
            GridPos { x: 6, y: 6 },
            4,
            LightKind::Normal,
        );

        let activation = PowerActivation {
            pos: GridPos { x: 3, y: 3 },
            kind: LightKind::Cross,
            partner_color: None,
        };
        let removed = fire_single_activation(&activation, &grid, &info);

        assert!(
            removed.contains(&center) && removed.contains(&same_row) && removed.contains(&same_col)
        );
        assert!(
            !removed.contains(&off_both),
            "a cell off both the row and column survives"
        );
    }

    // ─── Cross combos (resolve_swap_activation / resolve_wave) ──────────────────

    #[test]
    fn resolve_wave_adjacent_cross_plus_cross_sweeps_both_pluses() {
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 3 },
            1,
            LightKind::Cross,
        );
        put(
            &mut grid,
            &mut info,
            GridPos { x: 4, y: 3 },
            2,
            LightKind::Cross,
        );
        let on_a_col = put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 6 },
            3,
            LightKind::Normal,
        );
        let on_b_col = put(
            &mut grid,
            &mut info,
            GridPos { x: 4, y: 7 },
            4,
            LightKind::Normal,
        );
        let off_both = put(
            &mut grid,
            &mut info,
            GridPos { x: 0, y: 7 },
            5,
            LightKind::Normal,
        );

        let powers = [
            PowerActivation {
                pos: GridPos { x: 3, y: 3 },
                kind: LightKind::Cross,
                partner_color: None,
            },
            PowerActivation {
                pos: GridPos { x: 4, y: 3 },
                kind: LightKind::Cross,
                partner_color: None,
            },
        ];
        let removed = resolve_wave(&powers, &grid, &info).to_remove;

        assert!(removed.contains(&on_a_col) && removed.contains(&on_b_col));
        assert!(!removed.contains(&off_both));
    }

    #[test]
    fn resolve_wave_adjacent_cross_plus_ray_sweeps_both_columns() {
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 3 },
            1,
            LightKind::Cross,
        );
        put(
            &mut grid,
            &mut info,
            GridPos { x: 4, y: 3 },
            2,
            LightKind::RayH,
        );
        let on_a_col = put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 6 },
            3,
            LightKind::Normal,
        );
        let on_b_col = put(
            &mut grid,
            &mut info,
            GridPos { x: 4, y: 7 },
            4,
            LightKind::Normal,
        );

        let powers = [
            PowerActivation {
                pos: GridPos { x: 3, y: 3 },
                kind: LightKind::Cross,
                partner_color: None,
            },
            PowerActivation {
                pos: GridPos { x: 4, y: 3 },
                kind: LightKind::RayH,
                partner_color: None,
            },
        ];
        let removed = resolve_wave(&powers, &grid, &info).to_remove;

        assert!(removed.contains(&on_a_col) && removed.contains(&on_b_col));
    }

    #[test]
    fn resolve_wave_adjacent_cross_plus_supernova_bursts_3_band_plus() {
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 3 },
            1,
            LightKind::Cross,
        );
        put(
            &mut grid,
            &mut info,
            GridPos { x: 4, y: 3 },
            2,
            LightKind::Supernova,
        );
        // Within the 3-row-band / 3-col-band cross centered on the Supernova (4,3).
        let in_band = put(
            &mut grid,
            &mut info,
            GridPos { x: 7, y: 4 },
            3,
            LightKind::Normal,
        );
        // Outside the band entirely.
        let off_band = put(
            &mut grid,
            &mut info,
            GridPos { x: 7, y: 7 },
            4,
            LightKind::Normal,
        );

        let powers = [
            PowerActivation {
                pos: GridPos { x: 3, y: 3 },
                kind: LightKind::Cross,
                partner_color: None,
            },
            PowerActivation {
                pos: GridPos { x: 4, y: 3 },
                kind: LightKind::Supernova,
                partner_color: None,
            },
        ];
        let removed = resolve_wave(&powers, &grid, &info).to_remove;

        assert!(removed.contains(&in_band));
        assert!(!removed.contains(&off_band));
    }

    #[test]
    fn resolve_wave_adjacent_cross_plus_starburst_sweeps_both_axes() {
        let mut grid: Grid = HashMap::new();
        let mut info: EntityInfo = HashMap::new();
        put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 3 },
            1,
            LightKind::Cross,
        );
        let star = put(
            &mut grid,
            &mut info,
            GridPos { x: 4, y: 3 },
            2,
            LightKind::Starburst,
        );
        // Same color (the `put` helper always uses Red) as the Cross at (3,3), the Starburst's
        // partner — its row AND column must both clear, not just one.
        let same_row = put(
            &mut grid,
            &mut info,
            GridPos { x: 0, y: 3 },
            3,
            LightKind::Normal,
        );
        let same_col = put(
            &mut grid,
            &mut info,
            GridPos { x: 3, y: 0 },
            4,
            LightKind::Normal,
        );

        let powers = [
            PowerActivation {
                pos: GridPos { x: 3, y: 3 },
                kind: LightKind::Cross,
                partner_color: None,
            },
            PowerActivation {
                pos: GridPos { x: 4, y: 3 },
                kind: LightKind::Starburst,
                partner_color: None,
            },
        ];
        let removed = resolve_wave(&powers, &grid, &info).to_remove;

        assert!(removed.contains(&star));
        assert!(
            removed.contains(&same_row),
            "Cross's row must clear, not just its column"
        );
        assert!(
            removed.contains(&same_col),
            "Cross's column must clear, not just its row"
        );
    }
}
