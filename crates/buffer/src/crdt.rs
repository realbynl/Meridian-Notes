//! Spike: a sequence CRDT (RGA) for collaborative editing. See `docs/collab.md`.
//!
//! Not wired into `Buffer` or the UI. Each character is an element with a unique
//! `Id` (Lamport counter, site). Deletes leave tombstones. Replicas that have
//! applied the same set of ops hold the same text, in any delivery order, as
//! long as an op is applied after the ops it names (`Replica::receive` buffers).

use std::collections::HashSet;

/// Unique element id: Lamport counter, then site as tie-break.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id {
    pub counter: u64,
    pub site: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// Insert `ch` as `id`, right after `origin` (`None` = document start).
    Insert { id: Id, origin: Option<Id>, ch: char },
    Delete { id: Id },
}

#[derive(Clone, Debug)]
struct Elem {
    id: Id,
    ch: char,
    dead: bool,
}

/// A caret that survives concurrent edits: it sits right after element `after`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor(pub Option<Id>);

pub struct Replica {
    site: u32,
    clock: u64,
    elems: Vec<Elem>,
    seen: HashSet<Id>,
    pending: Vec<Op>,
}

impl Replica {
    pub fn new(site: u32) -> Self {
        Replica { site, clock: 0, elems: Vec::new(), seen: HashSet::new(), pending: Vec::new() }
    }

    pub fn text(&self) -> String {
        self.elems.iter().filter(|e| !e.dead).map(|e| e.ch).collect()
    }

    pub fn len(&self) -> usize {
        self.elems.iter().filter(|e| !e.dead).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Ops received but not applied yet (their dependencies are missing).
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Insert at visible char index `at`; returns the op to broadcast.
    pub fn insert(&mut self, at: usize, ch: char) -> Op {
        let origin = if at == 0 { None } else { Some(self.visible(at - 1).id) };
        self.clock += 1;
        let id = Id { counter: self.clock, site: self.site };
        let op = Op::Insert { id, origin, ch };
        self.integrate(&op);
        op
    }

    /// Delete the char at visible index `at`; returns the op to broadcast.
    pub fn delete(&mut self, at: usize) -> Op {
        let op = Op::Delete { id: self.visible(at).id };
        self.integrate(&op);
        op
    }

    /// Apply a remote op, or hold it until what it refers to has arrived.
    pub fn receive(&mut self, op: Op) {
        self.pending.push(op);
        loop {
            let before = self.pending.len();
            let queue = std::mem::take(&mut self.pending);
            for op in queue {
                if self.ready(&op) {
                    self.integrate(&op);
                } else {
                    self.pending.push(op);
                }
            }
            if self.pending.len() == before || self.pending.is_empty() {
                break;
            }
        }
    }

    pub fn anchor_at(&self, at: usize) -> Anchor {
        Anchor(if at == 0 { None } else { Some(self.visible(at - 1).id) })
    }

    /// Visible char index of an anchor. A deleted anchor element resolves to where it was.
    pub fn resolve(&self, a: Anchor) -> usize {
        let Some(id) = a.0 else { return 0 };
        let mut n = 0;
        for e in &self.elems {
            if !e.dead {
                n += 1;
            }
            if e.id == id {
                return n;
            }
        }
        n
    }

    fn visible(&self, at: usize) -> &Elem {
        self.elems.iter().filter(|e| !e.dead).nth(at).expect("index out of range")
    }

    fn ready(&self, op: &Op) -> bool {
        match op {
            Op::Insert { origin, .. } => origin.map_or(true, |o| self.seen.contains(&o)),
            Op::Delete { id } => self.seen.contains(id),
        }
    }

    fn integrate(&mut self, op: &Op) {
        match *op {
            Op::Insert { id, origin, ch } => {
                if !self.seen.insert(id) {
                    return; // duplicate delivery
                }
                self.clock = self.clock.max(id.counter);
                let mut i = match origin {
                    None => 0,
                    Some(o) => self.elems.iter().position(|e| e.id == o).expect("origin") + 1,
                };
                // RGA: skip siblings and their subtrees that were inserted later.
                while i < self.elems.len() && self.elems[i].id > id {
                    i += 1;
                }
                self.elems.insert(i, Elem { id, ch, dead: false });
            }
            Op::Delete { id } => {
                if let Some(e) = self.elems.iter_mut().find(|e| e.id == id) {
                    e.dead = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sync(a: &mut Replica, b: &mut Replica, from_a: &[Op], from_b: &[Op]) {
        for op in from_b {
            a.receive(op.clone());
        }
        for op in from_a {
            b.receive(op.clone());
        }
    }

    fn type_str(r: &mut Replica, at: usize, s: &str) -> Vec<Op> {
        s.chars().enumerate().map(|(i, c)| r.insert(at + i, c)).collect()
    }

    #[test]
    fn concurrent_inserts_at_same_spot_converge() {
        let mut a = Replica::new(1);
        let mut b = Replica::new(2);
        let base = type_str(&mut a, 0, "ac");
        for op in &base {
            b.receive(op.clone());
        }
        let oa = type_str(&mut a, 1, "XY");
        let ob = type_str(&mut b, 1, "pq");
        sync(&mut a, &mut b, &oa, &ob);
        assert_eq!(a.text(), b.text());
        assert_eq!(a.len(), 6);
        // Each author's run stays contiguous (no interleaving).
        let t = a.text();
        assert!(t.contains("XY") && t.contains("pq"), "{t}");
    }

    #[test]
    fn concurrent_delete_and_insert() {
        let mut a = Replica::new(1);
        let mut b = Replica::new(2);
        for op in type_str(&mut a, 0, "hello") {
            b.receive(op);
        }
        let oa = vec![a.delete(1), a.delete(1)]; // drop "el"
        let ob = vec![b.insert(2, '!')]; // insert inside the deleted span
        sync(&mut a, &mut b, &oa, &ob);
        assert_eq!(a.text(), b.text());
        assert_eq!(a.text(), "h!lo"); // "el" gone, the concurrent "!" survives
    }

    #[test]
    fn out_of_order_and_duplicate_delivery() {
        let mut a = Replica::new(1);
        let mut b = Replica::new(2);
        let mut ops = type_str(&mut a, 0, "abc");
        ops.push(a.delete(0));
        ops.reverse();
        for op in ops.iter().chain(ops.iter()) {
            b.receive(op.clone());
        }
        assert_eq!(b.pending(), 0);
        assert_eq!(b.text(), "bc");
    }

    #[test]
    fn cursor_follows_remote_edits() {
        let mut a = Replica::new(1);
        let mut b = Replica::new(2);
        for op in type_str(&mut a, 0, "world") {
            b.receive(op);
        }
        let caret = b.anchor_at(3); // after "wor"
        for op in type_str(&mut a, 0, "hello ") {
            b.receive(op);
        }
        assert_eq!(b.resolve(caret), 9);
        assert_eq!(&b.text()[..9], "hello wor");
        // Deleting the char the caret hangs off keeps the caret in place.
        let op = a.delete(8);
        b.receive(op);
        assert_eq!(b.resolve(caret), 8);
    }

    #[test]
    fn random_schedules_converge() {
        // Tiny deterministic LCG so the test needs no dependency.
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut rnd = move |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % n.max(1)
        };
        for _round in 0..50 {
            let mut reps: Vec<Replica> = (1..=3).map(Replica::new).collect();
            let mut outbox: Vec<Vec<Op>> = vec![Vec::new(); 3];
            for _ in 0..60 {
                let r = rnd(3);
                let len = reps[r].len();
                let op = if len > 0 && rnd(3) == 0 {
                    reps[r].delete(rnd(len))
                } else {
                    reps[r].insert(rnd(len + 1), (b'a' + rnd(26) as u8) as char)
                };
                outbox[r].push(op);
                if rnd(4) == 0 {
                    // Partial, shuffled delivery of one replica's backlog to another.
                    let to = rnd(3);
                    if to != r {
                        let mut batch = outbox[r].clone();
                        for i in (1..batch.len()).rev() {
                            batch.swap(i, rnd(i + 1));
                        }
                        for op in batch {
                            reps[to].receive(op);
                        }
                    }
                }
            }
            // Final all-to-all delivery in a random order.
            for to in 0..3 {
                for from in 0..3 {
                    if from != to {
                        let mut batch = outbox[from].clone();
                        for i in (1..batch.len()).rev() {
                            batch.swap(i, rnd(i + 1));
                        }
                        for op in batch {
                            reps[to].receive(op);
                        }
                    }
                }
            }
            let t0 = reps[0].text();
            for r in &reps {
                assert_eq!(r.pending(), 0);
                assert_eq!(r.text(), t0);
            }
        }
    }
}
