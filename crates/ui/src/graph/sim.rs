//! The graph's force simulation, after d3-force (which Obsidian uses): links as springs, nodes repelling through a
//! Barnes-Hut quadtree, a pull toward the middle, and an `alpha` that cools until the layout rests.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Forces {
    pub center: f32,
    pub repel: f32,
    pub link_force: f32,
    pub link_distance: f32,
}

pub struct Sim {
    pub pos: Vec<[f32; 2]>,
    vel: Vec<[f32; 2]>,
    /// Dragged nodes: the pointer owns them.
    pub fixed: Vec<bool>,
    edges: Vec<(u32, u32)>,
    deg: Vec<u32>,
    /// Heat: 1 at the start, cooling to `ALPHA_MIN`, where the layout is at rest.
    pub alpha: f32,
    /// What alpha cools toward: raised while a node is dragged so the rest follows.
    pub target: f32,
    pub forces: Forces,
}

const ALPHA_MIN: f32 = 0.001;
const ALPHA_DECAY: f32 = 0.0228;
const VELOCITY_DECAY: f32 = 0.4;
const THETA2: f32 = 0.81;
const DIST_MAX2: f32 = 4.0e6;

impl Sim {
    pub fn new(pos: Vec<[f32; 2]>, edges: Vec<(u32, u32)>, forces: Forces) -> Sim {
        let n = pos.len();
        let mut deg = vec![0u32; n];
        for &(a, b) in &edges {
            deg[a as usize] += 1;
            deg[b as usize] += 1;
        }
        Sim { vel: vec![[0.0; 2]; n], fixed: vec![false; n], pos, edges, deg, alpha: 1.0, target: 0.0, forces }
    }

    pub fn degree(&self, i: usize) -> u32 {
        self.deg[i]
    }

    pub fn settled(&self) -> bool {
        self.alpha < ALPHA_MIN && self.target == 0.0
    }

    pub fn reheat(&mut self, to: f32) {
        self.alpha = self.alpha.max(to);
    }

    /// One step. Returns false once the layout is at rest (nothing moved).
    pub fn tick(&mut self) -> bool {
        if self.settled() {
            return false;
        }
        self.alpha += (self.target - self.alpha) * ALPHA_DECAY;
        let a = self.alpha;
        let n = self.pos.len();
        let f = self.forces;
        // Center: every node is pulled toward the origin.
        let k = f.center * 0.1 * a;
        for i in 0..n {
            self.vel[i][0] -= self.pos[i][0] * k;
            self.vel[i][1] -= self.pos[i][1] * k;
        }
        // Links.
        for &(s, t) in &self.edges {
            let (s, t) = (s as usize, t as usize);
            let mut d = [self.pos[t][0] + self.vel[t][0] - self.pos[s][0] - self.vel[s][0], self.pos[t][1] + self.vel[t][1] - self.pos[s][1] - self.vel[s][1]];
            if d == [0.0, 0.0] {
                d = [1e-3 * ((s + 1) as f32), 1e-3 * ((t + 1) as f32)];
            }
            let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
            let strength = f.link_force / self.deg[s].min(self.deg[t]).max(1) as f32;
            let m = (l - f.link_distance) / l * a * strength;
            let bias = self.deg[s] as f32 / (self.deg[s] + self.deg[t]).max(1) as f32;
            self.vel[t][0] -= d[0] * m * bias;
            self.vel[t][1] -= d[1] * m * bias;
            self.vel[s][0] += d[0] * m * (1.0 - bias);
            self.vel[s][1] += d[1] * m * (1.0 - bias);
        }
        // Repulsion.
        if f.repel > 0.0 && n > 1 {
            let tree = Quad::build(&self.pos);
            let strength = -3.0 * f.repel * a;
            for i in 0..n {
                let (dx, dy) = tree.force(i, self.pos[i], strength);
                self.vel[i][0] += dx;
                self.vel[i][1] += dy;
            }
        }
        for i in 0..n {
            if self.fixed[i] {
                self.vel[i] = [0.0; 2];
                continue;
            }
            self.vel[i][0] *= 1.0 - VELOCITY_DECAY;
            self.vel[i][1] *= 1.0 - VELOCITY_DECAY;
            self.pos[i][0] += self.vel[i][0];
            self.pos[i][1] += self.vel[i][1];
        }
        true
    }
}

/// The d3 starting layout: a sunflower spiral, so no two nodes start on the same spot.
pub fn spiral(i: usize) -> [f32; 2] {
    let r = 10.0 * (0.5 + i as f32).sqrt();
    let a = i as f32 * std::f32::consts::PI * (3.0 - 5.0f32.sqrt());
    [r * a.cos(), r * a.sin()]
}

struct Cell {
    /// Centre of mass and total mass.
    cx: f32,
    cy: f32,
    mass: f32,
    /// Side of the square the cell covers.
    size: f32,
    kids: [u32; 4],
    /// The one node a leaf holds (u32::MAX for none or an inner cell).
    leaf: u32,
}

const NONE: u32 = u32::MAX;

struct Quad {
    cells: Vec<Cell>,
}

impl Quad {
    fn build(pos: &[[f32; 2]]) -> Quad {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in pos {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        let size = (x1 - x0).max(y1 - y0).max(1.0) * 1.0001;
        let mut q = Quad { cells: Vec::with_capacity(pos.len() * 2) };
        q.cells.push(Cell { cx: 0.0, cy: 0.0, mass: 0.0, size, kids: [NONE; 4], leaf: NONE });
        for (i, p) in pos.iter().enumerate() {
            q.insert(pos, i as u32, *p, x0, y0);
        }
        q.summarise(0, pos);
        q
    }

    fn insert(&mut self, pos: &[[f32; 2]], id: u32, p: [f32; 2], ox: f32, oy: f32) {
        let (mut c, mut x, mut y, mut size) = (0usize, ox, oy, self.cells[0].size);
        let mut depth = 0;
        loop {
            let inner = self.cells[c].kids.iter().any(|&k| k != NONE);
            if !inner && self.cells[c].leaf == NONE {
                self.cells[c].leaf = id;
                return;
            }
            if !inner {
                // A leaf with a node already: push that node down, unless the two sit on the same spot.
                let other = self.cells[c].leaf;
                let q = pos[other as usize];
                if depth > 40 || (q[0] == p[0] && q[1] == p[1]) {
                    return; // coincident nodes: the first one carries the mass of the spot
                }
                self.cells[c].leaf = NONE;
                self.push_down(c, other, q, x, y, size);
            }
            let half = size / 2.0;
            let (qx, qy) = ((p[0] >= x + half) as usize, (p[1] >= y + half) as usize);
            let slot = qy * 2 + qx;
            if qx == 1 {
                x += half;
            }
            if qy == 1 {
                y += half;
            }
            size = half;
            depth += 1;
            let k = self.cells[c].kids[slot];
            if k == NONE {
                self.cells.push(Cell { cx: 0.0, cy: 0.0, mass: 0.0, size, kids: [NONE; 4], leaf: id });
                let n = (self.cells.len() - 1) as u32;
                self.cells[c].kids[slot] = n;
                return;
            }
            c = k as usize;
        }
    }

    fn push_down(&mut self, c: usize, id: u32, p: [f32; 2], x: f32, y: f32, size: f32) {
        let half = size / 2.0;
        let (qx, qy) = ((p[0] >= x + half) as usize, (p[1] >= y + half) as usize);
        self.cells.push(Cell { cx: 0.0, cy: 0.0, mass: 0.0, size: half, kids: [NONE; 4], leaf: id });
        let n = (self.cells.len() - 1) as u32;
        self.cells[c].kids[qy * 2 + qx] = n;
    }

    fn summarise(&mut self, c: usize, pos: &[[f32; 2]]) {
        let kids = self.cells[c].kids;
        if kids.iter().all(|&k| k == NONE) {
            let l = self.cells[c].leaf;
            if l != NONE {
                let p = pos[l as usize];
                self.cells[c].cx = p[0];
                self.cells[c].cy = p[1];
                self.cells[c].mass = 1.0;
            }
            return;
        }
        let (mut m, mut sx, mut sy) = (0.0, 0.0, 0.0);
        for k in kids.into_iter().filter(|&k| k != NONE) {
            self.summarise(k as usize, pos);
            let kc = &self.cells[k as usize];
            m += kc.mass;
            sx += kc.cx * kc.mass;
            sy += kc.cy * kc.mass;
        }
        let cell = &mut self.cells[c];
        cell.mass = m;
        if m > 0.0 {
            cell.cx = sx / m;
            cell.cy = sy / m;
        }
    }

    /// The push on node `me` at `p` from every other node; `strength` is negative to repel.
    fn force(&self, me: usize, p: [f32; 2], strength: f32) -> (f32, f32) {
        let (mut fx, mut fy) = (0.0, 0.0);
        let mut stack = vec![0u32];
        while let Some(c) = stack.pop() {
            let cell = &self.cells[c as usize];
            if cell.mass == 0.0 {
                continue;
            }
            let (mut dx, mut dy) = (cell.cx - p[0], cell.cy - p[1]);
            let mut d2 = dx * dx + dy * dy;
            let inner = cell.kids.iter().any(|&k| k != NONE);
            if inner && cell.size * cell.size >= THETA2 * d2.max(1e-6) {
                stack.extend(cell.kids.iter().copied().filter(|&k| k != NONE));
                continue;
            }
            if !inner && cell.leaf == me as u32 {
                continue;
            }
            if d2 >= DIST_MAX2 {
                continue;
            }
            if d2 == 0.0 {
                dx = 1e-3 * (me as f32 + 1.0).sin();
                dy = 1e-3 * (me as f32 + 1.0).cos();
                d2 = dx * dx + dy * dy;
            }
            if d2 < 1.0 {
                d2 = d2.sqrt(); // d3 distanceMin: never stronger than at distance 1
            }
            let w = strength * cell.mass / d2;
            fx += dx * w;
            fy += dy * w;
        }
        (fx, fy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const F: Forces = Forces { center: 0.52, repel: 10.0, link_force: 1.0, link_distance: 250.0 };

    fn run(sim: &mut Sim) -> usize {
        let mut n = 0;
        while sim.tick() && n < 2000 {
            n += 1;
        }
        n
    }

    fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
    }

    #[test]
    fn a_pair_settles_near_the_link_distance_and_the_layout_comes_to_rest() {
        let mut s = Sim::new((0..2).map(spiral).collect(), vec![(0, 1)], F);
        let ticks = run(&mut s);
        assert!(s.settled() && ticks > 100 && ticks < 1000, "{ticks}");
        let d = dist(s.pos[0], s.pos[1]);
        assert!((150.0..300.0).contains(&d), "{d}");
    }

    #[test]
    fn unlinked_nodes_repel_and_the_centre_force_keeps_them_in_reach() {
        let n = 30;
        let mut s = Sim::new((0..n).map(spiral).collect(), vec![], F);
        run(&mut s);
        let far = s.pos.iter().map(|p| dist(*p, [0.0, 0.0])).fold(0.0, f32::max);
        assert!(far > 30.0 && far < 1500.0, "{far}");
        let mut closest = f32::MAX;
        for i in 0..n {
            for j in i + 1..n {
                closest = closest.min(dist(s.pos[i], s.pos[j]));
            }
        }
        assert!(closest > 2.0, "{closest}");
        let mut loose = Sim::new((0..n).map(spiral).collect(), vec![], Forces { center: 0.0, ..F });
        run(&mut loose);
        let far0 = loose.pos.iter().map(|p| dist(*p, [0.0, 0.0])).fold(0.0, f32::max);
        assert!(far0 > far, "no centre force spreads further: {far0} vs {far}");
    }

    #[test]
    fn a_dragged_node_stays_put_and_its_neighbour_follows() {
        let mut s = Sim::new(vec![[0.0, 0.0], [250.0, 0.0]], vec![(0, 1)], F);
        run(&mut s);
        s.fixed[0] = true;
        s.pos[0] = [1000.0, 1000.0];
        s.target = 0.3;
        s.reheat(0.3);
        for _ in 0..300 {
            s.tick();
        }
        assert_eq!(s.pos[0], [1000.0, 1000.0]);
        assert!(dist(s.pos[0], s.pos[1]) < 400.0, "pulled along");
        s.fixed[0] = false;
        s.target = 0.0;
        assert!(!s.settled());
    }

    #[test]
    fn coincident_nodes_do_not_produce_nan() {
        let mut s = Sim::new(vec![[5.0, 5.0]; 6], vec![(0, 1), (1, 2)], F);
        run(&mut s);
        assert!(s.pos.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
    }

    #[test]
    fn a_thousand_nodes_tick_fast_enough_for_a_frame() {
        let n = 1000;
        let edges: Vec<(u32, u32)> = (1..n as u32).map(|i| (i, (i * 7919) % i)).collect();
        let mut s = Sim::new((0..n).map(spiral).collect(), edges, F);
        let t = std::time::Instant::now();
        for _ in 0..20 {
            s.tick();
        }
        let per = t.elapsed().as_secs_f32() * 1000.0 / 20.0;
        eprintln!("1000 nodes: {per:.2} ms per tick (debug build)");
        assert!(per < 60.0, "{per}");
    }
}
