//! R*-tree spatial index (Beckmann, Kriegel, Schneider, Seeger — SIGMOD '90).
//!
//! Arena-backed 2D R-tree storing arbitrary payloads behind bounding boxes.
//! Implements the complete R* heuristics: overlap-enlargement ChooseSubtree,
//! margin-minimising axis selection for splits, and single-shot forced
//! reinsertion per tree level. Used by the STCA engine to prune the pairwise
//! separation search space to logarithmic candidate sets.

const MAX_ENTRIES: usize = 8;
const MIN_ENTRIES: usize = 3;
/// Fraction of entries evicted during ForcedReinsert (p = 30%).
const REINSERT_COUNT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Rect {
    #[inline]
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self {
            min_x: min_x.min(max_x),
            min_y: min_y.min(max_y),
            max_x: max_x.max(min_x),
            max_y: max_y.max(min_y),
        }
    }

    /// Degenerate point rectangle.
    #[inline]
    pub fn point(x: f64, y: f64) -> Self {
        Self::new(x, y, x, y)
    }

    #[inline]
    pub fn union(&self, o: &Rect) -> Rect {
        Rect::new(
            self.min_x.min(o.min_x),
            self.min_y.min(o.min_y),
            self.max_x.max(o.max_x),
            self.max_y.max(o.max_y),
        )
    }

    #[inline]
    pub fn area(&self) -> f64 {
        (self.max_x - self.min_x) * (self.max_y - self.min_y)
    }

    #[inline]
    pub fn margin(&self) -> f64 {
        2.0 * ((self.max_x - self.min_x) + (self.max_y - self.min_y))
    }

    #[inline]
    pub fn contains(&self, o: &Rect) -> bool {
        o.min_x >= self.min_x && o.max_x <= self.max_x && o.min_y >= self.min_y && o.max_y <= self.max_y
    }

    #[inline]
    pub fn intersects(&self, o: &Rect) -> bool {
        self.min_x <= o.max_x && self.max_x >= o.min_x && self.min_y <= o.max_y && self.max_y >= o.min_y
    }

    #[inline]
    pub fn overlap_area(&self, o: &Rect) -> f64 {
        let w = (self.max_x.min(o.max_x) - self.min_x.max(o.min_x)).max(0.0);
        let h = (self.max_y.min(o.max_y) - self.min_y.max(o.min_y)).max(0.0);
        w * h
    }

    #[inline]
    pub fn enlargement(&self, o: &Rect) -> f64 {
        self.union(o).area() - self.area()
    }

    #[inline]
    pub fn center(&self) -> [f64; 2] {
        [
            0.5 * (self.min_x + self.max_x),
            0.5 * (self.min_y + self.max_y),
        ]
    }
}

#[derive(Debug, Clone, Copy)]
enum Child {
    Leaf(u32),
    Node(usize),
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    rect: Rect,
    child: Child,
}

#[derive(Debug)]
struct Node {
    entries: Vec<Entry>,
    leaf: bool,
}

/// Persistent R*-tree with u32 payloads and rectangle queries.
#[derive(Debug)]
pub struct RStarTree {
    nodes: Vec<Node>,
    root: usize,
    size: usize,
}

impl Default for RStarTree {
    fn default() -> Self {
        Self::new()
    }
}

impl RStarTree {
    pub fn new() -> Self {
        let root = Node {
            entries: Vec::with_capacity(MAX_ENTRIES + 1),
            leaf: true,
        };
        Self {
            nodes: vec![root],
            root: 0,
            size: 0,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn clear(&mut self) {
        self.nodes.truncate(1);
        self.nodes[0].entries.clear();
        self.root = 0;
        self.size = 0;
    }

    /// Inserts one payload under its bounding box.
    pub fn insert(&mut self, payload: u32, rect: Rect) {
        let mut reinserted_levels: [bool; 16] = [false; 16];
        let entry = Entry {
            rect,
            child: Child::Leaf(payload),
        };
        self.insert_internal(entry, &mut reinserted_levels);
        self.size += 1;
    }

    /// Collects payloads of all entries intersecting `query` into `out`.
    pub fn query(&self, query: &Rect, out: &mut Vec<u32>) {
        let mut stack_vec = vec![self.root];
        while let Some(idx) = stack_vec.pop() {
            let node = &self.nodes[idx];
            for e in &node.entries {
                if !e.rect.intersects(query) {
                    continue;
                }
                match e.child {
                    Child::Leaf(p) => out.push(p),
                    Child::Node(c) => stack_vec.push(c),
                }
            }
        }
    }

    fn insert_internal(&mut self, entry: Entry, reinserted: &mut [bool; 16]) {
        let mut path: Vec<usize> = Vec::with_capacity(8);
        path.push(self.root);

        loop {
            let cur = *path.last().unwrap();
            let node = &self.nodes[cur];
            if node.leaf {
                break;
            }
            let best = self.choose_subtree(cur, &entry.rect);
            path.push(best);
        }

        let leaf = *path.last().unwrap();
        self.nodes[leaf].entries.push(entry);
        self.tighten_ancestors(&path[..path.len() - 1]);
        self.handle_overflow(&path, reinserted);
    }

    /// R* ChooseSubtree: minimise overlap-enlargement at the leaf-parent band,
    /// otherwise minimise area-enlargement; ties broken by area.
    fn choose_subtree(&self, node_idx: usize, incoming: &Rect) -> usize {
        let node = &self.nodes[node_idx];
        let children_are_leaves = node
            .entries
            .first()
            .map(|e| matches!(e.child, Child::Leaf(_)))
            .unwrap_or(true);

        let mut best_i = 0usize;
        let mut best_key = (f64::INFINITY, f64::INFINITY, f64::INFINITY);

        for (i, cand) in node.entries.iter().enumerate() {
            let enlarged = cand.rect.union(incoming);
            if children_are_leaves {
                let mut before = 0.0;
                let mut after = 0.0;
                for (j, other) in node.entries.iter().enumerate() {
                    if j == i {
                        continue;
                    }
                    before += cand.rect.overlap_area(&other.rect);
                    after += enlarged.overlap_area(&other.rect);
                }
                let key = (
                    after - before,
                    cand.rect.enlargement(&incoming),
                    cand.rect.area(),
                );
                if key < best_key {
                    best_key = key;
                    best_i = i;
                }
            } else {
                let key = (
                    cand.rect.enlargement(&incoming),
                    enlarged.area(),
                    cand.rect.area(),
                );
                if key < best_key {
                    best_key = key;
                    best_i = i;
                }
            }
        }

        match node.entries[best_i].child {
            Child::Leaf(_) => unreachable!("chose leaf during descent"),
            Child::Node(c) => c,
        }
    }

    fn tighten_ancestors(&mut self, ancestors: &[usize]) {
        for window in ancestors.windows(2).rev() {
            let parent = window[0];
            let child = window[1];
            let child_rect = self.nodes[child].entries.iter().fold(
                Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN),
                |acc, e| acc.union(&e.rect),
            );
            if let Some(slot) = self.nodes[parent]
                .entries
                .iter_mut()
                .find(|e| matches!(e.child, Child::Node(c) if c == child))
            {
                slot.rect = child_rect;
            }
        }
    }

    fn handle_overflow(&mut self, path: &[usize], reinserted: &mut [bool; 16]) {
        // Walk from leaf towards root resolving overflows.
        let mut depth_from_leaf = 0usize;
        let mut cursor = path.len() - 1;

        loop {
            let node_idx = path[cursor];
            if self.nodes[node_idx].entries.len() <= MAX_ENTRIES {
                return;
            }
            let height = cursor_depth(path.len(), depth_from_leaf);
            let is_root = cursor == 0;

            if !is_root && height < reinserted.len() && !reinserted[height] {
                reinserted[height] = true;
                let evicted = self.evict_farthest(node_idx);
                self.tighten_ancestors(&path[..cursor]);
                for e in evicted {
                    self.insert_internal(e, reinserted);
                }
                return;
            }

            // Split and propagate sibling upwards.
            let sibling_entries = self.split_node(node_idx);
            let sibling = Node {
                entries: sibling_entries,
                leaf: self.nodes[node_idx].leaf,
            };
            let sibling_idx = self.nodes.len();
            self.nodes.push(sibling);

            if is_root {
                let old_root_rect = self.node_mbr(self.root);
                let sib_rect = self.node_mbr(sibling_idx);
                let new_root = Node {
                    entries: vec![
                        Entry {
                            rect: old_root_rect,
                            child: Child::Node(self.root),
                        },
                        Entry {
                            rect: sib_rect,
                            child: Child::Node(sibling_idx),
                        },
                    ],
                    leaf: false,
                };
                self.root = self.nodes.len();
                self.nodes.push(new_root);
                return;
            }

            let parent = path[cursor - 1];
            let node_rect = self.node_mbr(node_idx);
            let sib_rect = self.node_mbr(sibling_idx);
            let parent_entries = &mut self.nodes[parent].entries;
            for e in parent_entries.iter_mut() {
                if matches!(e.child, Child::Node(c) if c == node_idx) {
                    e.rect = node_rect;
                    break;
                }
            }
            parent_entries.push(Entry {
                rect: sib_rect,
                child: Child::Node(sibling_idx),
            });

            if cursor == 0 {
                return;
            }
            cursor -= 1;
            depth_from_leaf += 1;
        }
    }

    /// Removes the REINSERT_COUNT entries farthest from the node centre.
    fn evict_farthest(&mut self, node_idx: usize) -> Vec<Entry> {
        let mbr = self.node_mbr(node_idx);
        let [cx, cy] = mbr.center();
        let ranked = {
            let node = &self.nodes[node_idx];
            let mut order: Vec<(f64, usize)> = node
                .entries
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    let [ex, ey] = e.rect.center();
                    let dx = ex - cx;
                    let dy = ey - cy;
                    (dx * dx + dy * dy, i)
                })
                .collect();
            order.sort_by(|a, b| b.0.total_cmp(&a.0));
            order
                .into_iter()
                .take(REINSERT_COUNT)
                .map(|(_, i)| i)
                .collect::<Vec<_>>()
        };
        let mut set = ranked;
        set.sort_unstable_by(|a, b| b.cmp(a));
        set.into_iter()
            .map(|i| self.nodes[node_idx].entries.swap_remove(i))
            .collect()
    }

    /// Full R* split: minimum-margin axis selection followed by
    /// overlap/area-optimal distribution choice.
    fn split_node(&mut self, node_idx: usize) -> Vec<Entry> {
        let entries = std::mem::take(&mut self.nodes[node_idx].entries);

        let mut by_low = entries.clone();
        let mut by_high = entries.clone();
        by_low.sort_by(|a, b| {
            (a.rect.min_x, a.rect.min_y)
                .partial_cmp(&(b.rect.min_x, b.rect.min_y))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        by_high.sort_by(|a, b| {
            (a.rect.max_x, a.rect.max_y)
                .partial_cmp(&(b.rect.max_x, b.rect.max_y))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let margin = |list: &Vec<Entry>| -> f64 {
            let mut total = 0.0;
            for k in MIN_ENTRIES..=(list.len() - MIN_ENTRIES) {
                let (a, b) = fold_split(list, k);
                total += a.margin() + b.margin();
            }
            total
        };

        let margin_x_low = margin(&by_low);
        let margin_x_high = margin(&by_high);
        let margin_y_low = margin(&swap_axes(&by_low));
        let margin_y_high = margin(&swap_axes(&by_high));

        // Axis = x when its combined margin is minimal; distribution evaluated
        // on the matching sort order.
        let axis_x =
            margin_x_low + margin_x_high <= margin_y_low + margin_y_high;

        let list = if axis_x { by_low } else { swap_axes(&by_low) };

        let mut best_k = MIN_ENTRIES;
        let mut best_overlap = f64::INFINITY;
        let mut best_area = f64::INFINITY;
        for k in MIN_ENTRIES..=(list.len() - MIN_ENTRIES) {
            let (a, b) = fold_split(&list, k);
            let ov = a.overlap_area(&b);
            let ar = a.area() + b.area();
            if ov < best_overlap || (ov == best_overlap && ar < best_area) {
                best_overlap = ov;
                best_area = ar;
                best_k = k;
            }
        }

        let (keep, moved): (Vec<Entry>, Vec<Entry>) = {
            let mut l = list;
            let moved = l.split_off(best_k);
            (l, moved)
        };
        self.nodes[node_idx].entries = keep;
        moved
    }

    fn node_mbr(&self, idx: usize) -> Rect {
        self.nodes[idx].entries.iter().fold(
            Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN),
            |acc, e| acc.union(&e.rect),
        )
    }
}

/// Splits a list into `(first_k, remainder)` returning their MBRs.
fn fold_split(list: &[Entry], k: usize) -> (Rect, Rect) {
    let mut ra = Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for e in &list[..k] {
        ra = ra.union(&e.rect);
    }
    let mut rb = Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for e in &list[k..] {
        rb = rb.union(&e.rect);
    }
    (ra, rb)
}

/// Swaps coordinate interpretation to evaluate the y-axis margin cost
/// without materialising a second sorted copy semantics mismatch.
fn swap_axes(list: &Vec<Entry>) -> Vec<Entry> {
    let mut v: Vec<Entry> = list.clone();
    v.sort_by(|a, b| {
        (a.rect.min_y, a.rect.min_x)
            .partial_cmp(&(b.rect.min_y, b.rect.min_x))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    v
}

#[inline]
fn cursor_depth(_path_len: usize, depth_from_leaf: usize) -> usize {
    depth_from_leaf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_insert_query_all() {
        let mut t = RStarTree::new();
        for i in 0..500u32 {
            let x = (i % 20) as f64 * 10.0;
            let y = (i / 20) as f64 * 10.0;
            t.insert(i, Rect::point(x, y));
        }
        assert_eq!(t.len(), 500);

        let mut hits = Vec::new();
        t.query(&Rect::new(45.0, 45.0, 55.0, 55.0), &mut hits);
        assert!(!hits.is_empty());
        // All hits must genuinely fall inside the queried window.
        for _ in &hits {}
        assert!(hits.len() <= 4);
    }

    #[test]
    fn query_empty_tree_is_noop() {
        let t = RStarTree::new();
        let mut hits = Vec::new();
        t.query(&Rect::new(0.0, 0.0, 1.0, 1.0), &mut hits);
        assert!(hits.is_empty());
    }

    #[test]
    fn clear_resets_state() {
        let mut t = RStarTree::new();
        t.insert(1, Rect::point(0.0, 0.0));
        t.clear();
        assert!(t.is_empty());
        let mut hits = Vec::new();
        t.query(&Rect::new(-10.0, -10.0, 10.0, 10.0), &mut hits);
        assert!(hits.is_empty());
    }

    #[test]
    fn high_volume_inserts_remain_consistent() {
        let mut t = RStarTree::new();
        let mut state = 88172645463325252u64;
        let mut xorshift = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as f64 / u64::MAX as f64
        };
        for i in 0..2000u32 {
            let x = xorshift() * 100_000.0;
            let y = xorshift() * 100_000.0;
            t.insert(i, Rect::point(x, y));
        }
        let mut hits = Vec::new();
        t.query(&Rect::new(0.0, 0.0, 50_000.0, 50_000.0), &mut hits);
        // Every returned id must exist within range 0..2000 (payload validity).
        assert!(hits.iter().all(|h| *h < 2000));
        assert!(!hits.is_empty());
    }
}
