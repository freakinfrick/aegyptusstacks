//! BFS traverser and Line-of-Sight algorithms, ported from hex2d-dpcext-rs

/// Breadth First Search
pub mod bfs {
    use crate::Coordinate;
    use crate::Integer;

    use std::hash;
    use std::collections::VecDeque;
    use std::collections::HashMap;
    use std::collections::hash_map::Entry::{Occupied, Vacant};

    struct Visited<I: Integer> {
        prev: Coordinate<I>,
        dist: u32,
    }

    /// BFS Traverser
    ///
    /// Find closest coordinates satisfying `is_dest` reachable through
    /// coordinates where `can_pass` returns true.
    pub struct Traverser<FCanPass, FIsDest, I = i32>
    where
        I: Integer + hash::Hash,
        FCanPass: Fn(Coordinate<I>) -> bool,
        FIsDest: Fn(Coordinate<I>) -> bool,
    {
        visited: HashMap<Coordinate<I>, Visited<I>>,
        to_traverse: VecDeque<Coordinate<I>>,
        can_pass: FCanPass,
        is_dest: FIsDest,
        start: Coordinate<I>,
    }

    impl<FCanPass, FIsDest, I> Traverser<FCanPass, FIsDest, I>
    where
        I: Integer + hash::Hash + std::ops::AddAssign,
        FCanPass: Fn(Coordinate<I>) -> bool,
        FIsDest: Fn(Coordinate<I>) -> bool,
    {
        /// Create a Traverser with initial conditions
        pub fn new(can_pass: FCanPass, is_dest: FIsDest, start: Coordinate<I>) -> Self {
            let mut to_traverse = VecDeque::new();
            to_traverse.push_back(start);

            let mut visited = HashMap::new();
            visited.insert(start, Visited { prev: start, dist: 0 });

            Traverser {
                visited,
                to_traverse,
                can_pass,
                is_dest,
                start,
            }
        }

        /// Find next closest coordinate
        pub fn find(&mut self) -> Option<Coordinate<I>> {
            loop {
                let pos = match self.to_traverse.pop_front() {
                    None => return None,
                    Some(coord) => coord,
                };

                if (self.can_pass)(pos) {
                    let &Visited { dist, .. } = self.visited.get(&pos)
                        .expect("BFS: Should have been visited already");
                    let dist = dist + 1;

                    for &npos in pos.neighbors().iter() {
                        match self.visited.entry(npos) {
                            Occupied(_) => {}
                            Vacant(entry) => {
                                entry.insert(Visited { prev: pos, dist });
                                self.to_traverse.push_back(npos);
                            }
                        }
                    }
                }

                if (self.is_dest)(pos) {
                    return Some(pos);
                }
            }
        }

        /// Return one step closer to start from `pos`
        pub fn backtrace(&self, pos: Coordinate<I>) -> Option<Coordinate<I>> {
            self.visited.get(&pos).map(|entry| entry.prev)
        }

        /// Walk back to find the neighbor of start that leads to `pos`
        pub fn backtrace_last(&self, mut pos: Coordinate<I>) -> Option<Coordinate<I>> {
            loop {
                pos = match self.visited.get(&pos) {
                    None => return None,
                    Some(entry) => {
                        if entry.prev == self.start {
                            return Some(pos);
                        } else {
                            entry.prev
                        }
                    }
                }
            }
        }
    }
}

/// Recursive LoS algorithm
pub mod los {
    use crate::{Integer, Direction, Coordinate};
    use crate::Angle::{Left, Right};

    fn los_rec<FOpaqueness, FVisible, I>(
        opaqueness: &FOpaqueness,
        visible: &mut FVisible,
        light: I,
        _start_dir: Direction,
        pos: Coordinate<I>,
        main_dir: Direction,
        dir: Option<Direction>,
        pdir: Option<Direction>,
    ) where
        I: Integer + std::ops::AddAssign,
        FOpaqueness: Fn(Coordinate<I>) -> I,
        FVisible: FnMut(Coordinate<I>, I),
    {
        let mut light = light;
        let opaq = opaqueness(pos);

        if opaq >= light {
            return;
        } else {
            light = light - opaq;
        }

        visible(pos, light);

        let neighbors = match (dir, pdir) {
            (Some(dir), Some(pdir)) => {
                if dir == pdir {
                    vec![dir]
                } else {
                    vec![dir, pdir]
                }
            }
            (Some(dir), None) => {
                if main_dir == dir {
                    vec![dir, dir + Left, dir + Right]
                } else {
                    vec![dir, main_dir]
                }
            }
            _ => {
                vec![main_dir, main_dir + Left, main_dir + Right]
            }
        };

        for &d in neighbors.iter() {
            let npos = pos + d;
            match dir {
                Some(_) => los_rec(opaqueness, visible, light, d, npos, d, Some(d), dir),
                None => los_rec(opaqueness, visible, light, main_dir, npos, main_dir, Some(d), dir),
            }
        }
    }

    /// LoS from `pos` in directions `dirs`
    pub fn los<FOpaqueness, FVisible, I>(
        opaqueness: &FOpaqueness,
        visible: &mut FVisible,
        light: I,
        pos: Coordinate<I>,
        dirs: &[Direction],
    ) where
        I: Integer + std::ops::AddAssign,
        FOpaqueness: Fn(Coordinate<I>) -> I,
        FVisible: FnMut(Coordinate<I>, I),
    {
        for dir in dirs.iter() {
            los_rec(opaqueness, visible, light, *dir, pos, *dir, None, None);
        }
    }
}

/// LoS with straight line verification
pub mod los2 {
    use crate::{Integer, Direction, Coordinate};
    use crate::Angle::{Left, Right, Forward};
    use num::FromPrimitive;
    use std::collections::HashSet;
    use std::hash;
    use std::ops::Add;
    use std::cmp;

    fn los_check_line<FOpaqueness, I>(
        opaqueness: &FOpaqueness,
        light: I,
        start: Coordinate<I>,
        pos: Coordinate<I>,
    ) -> (bool, I)
    where
        I: Integer + hash::Hash + Eq + std::ops::AddAssign,
        for<'a> &'a I: Add<&'a I, Output = I>,
        FOpaqueness: Fn(Coordinate<I>) -> I,
    {
        let mut opaq_sum1: I = FromPrimitive::from_i8(0).unwrap();
        let mut last1 = start;

        let mut opaq_sum2: I = FromPrimitive::from_i8(0).unwrap();
        let mut last2 = start;

        for (c1, c2) in start.line_to_with_edge_detection_iter(pos) {
            if opaq_sum1 < light {
                let opaq1 = opaqueness(c1);
                opaq_sum1 = opaq_sum1 + opaq1;
                last1 = c1;
            }

            if opaq_sum2 < light {
                let opaq2 = opaqueness(c2);
                opaq_sum2 = opaq_sum2 + opaq2;
                last2 = c2;
            }
        }

        match (last1 == pos, last2 == pos) {
            (true, true) => (true, light - cmp::min(opaq_sum1, opaq_sum2)),
            (true, false) => (true, light - opaq_sum1),
            (false, true) => (true, light - opaq_sum2),
            (false, false) => (false, light - light),
        }
    }

    fn los_rec<FOpaqueness, FVisible, I>(
        opaqueness: &FOpaqueness,
        visible: &mut FVisible,
        light: I,
        start: Coordinate<I>,
        pos: Coordinate<I>,
        dir: Direction,
        visited: &mut HashSet<Coordinate<I>>,
    ) where
        I: Integer + hash::Hash + Eq + std::ops::AddAssign,
        for<'a> &'a I: Add<&'a I, Output = I>,
        FOpaqueness: Fn(Coordinate<I>) -> I,
        FVisible: FnMut(Coordinate<I>, I),
    {
        if visited.contains(&pos) {
            return;
        } else {
            visited.insert(pos);
        }

        let (directly_visible, v_light) = los_check_line(opaqueness, light, start, pos);

        if directly_visible {
            visible(pos, v_light);
        } else {
            let dir_to = start.direction_to_cw(pos).unwrap_or(dir);
            let neighbors = vec![Left, Right];
            for npos in neighbors.iter()
                .map(|&rd| dir_to + rd)
                .map(|d| pos + d)
            {
                let (side_visible, v_light) = los_check_line(opaqueness, light, start, npos);
                if side_visible {
                    visible(pos, v_light);
                }
            }
            return;
        }

        let neighbors = vec![Forward, Left, Right];
        for &a in neighbors.iter() {
            let npos = pos + (dir + a);
            los_rec(opaqueness, visible, light, start, npos, dir, visited);
        }
    }

    /// LoS with straight line checking
    pub fn los<FOpaqueness, FVisible, I>(
        opaqueness: &FOpaqueness,
        visible: &mut FVisible,
        light: I,
        pos: Coordinate<I>,
        dirs: &[Direction],
    ) where
        I: Integer + hash::Hash + std::ops::AddAssign,
        for<'a> &'a I: Add<&'a I, Output = I>,
        FOpaqueness: Fn(Coordinate<I>) -> I,
        FVisible: FnMut(Coordinate<I>, I),
    {
        for dir in dirs.iter() {
            let mut visited = HashSet::new();
            los_rec(opaqueness, visible, light, pos, pos, *dir, &mut visited);
        }
    }
}
