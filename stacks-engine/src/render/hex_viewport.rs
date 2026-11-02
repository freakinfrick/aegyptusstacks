use hex2d_stacks::Coordinate;

/// Maps hex coordinates to terminal screen positions.
/// Uses FLAT-TOP orientation to match BFS's direction conventions.
///
/// Screen layout:
///       NW    NE
///        \   /
///   W --- * --- E
///        /   \
///       SW    SE
///
/// Formula: col = 2*(dx - dy), row = -(dx + dy)
/// Each hex cell is 2 chars wide, 1 row tall.
/// E-neighbors: col+4, row+0 (2-char gap)
/// NE-neighbors: col+2, row-1 (touching, no overlap)
pub struct HexViewport {
    pub center: Coordinate<i32>,
    pub width: u16,
    pub height: u16,
}

impl HexViewport {
    pub fn new(center: Coordinate<i32>, width: u16, height: u16) -> Self {
        HexViewport { center, width, height }
    }

    /// Convert hex coordinate to terminal (col, row). Returns None if outside viewport.
    pub fn world_to_screen(&self, coord: Coordinate<i32>) -> Option<(u16, u16)> {
        let dx = coord.x - self.center.x;
        let dy = coord.y - self.center.y;

        // Flat-top hex mapping (2-char cells, 1-row tall)
        let col = 2 * (dx - dy) + (self.width as i32) / 2;
        let row = -(dx + dy) + (self.height as i32) / 2;

        if col < 0 || col + 2 > self.width as i32 || row < 0 || row >= self.height as i32 {
            return None;
        }

        Some((col as u16, row as u16))
    }

    /// All hex coordinates visible in this viewport.
    pub fn visible_coords(&self) -> Vec<Coordinate<i32>> {
        let mut coords = Vec::new();

        let w = self.width as i32;
        let h = self.height as i32;

        // col = 2*(dx-dy) + w/2 must be in [0, w-2]
        //   => diff = dx-dy in [-(w-2)/4, (w-2)/4]  (roughly)
        // row = -(dx+dy) + h/2 must be in [0, h-1]
        //   => sum = dx+dy in [-(h-1)/2, h/2]  (roughly)

        let sum_min = -(h / 2) - 1;
        let sum_max = (h / 2) + 1;
        let diff_min = -(w / 4) - 1;
        let diff_max = (w / 4) + 1;

        for sum in sum_min..=sum_max {
            for diff in diff_min..=diff_max {
                if (sum + diff) % 2 != 0 {
                    continue;
                }
                let dx = (sum + diff) / 2;
                let dy = (sum - diff) / 2;
                let coord = Coordinate::new(self.center.x + dx, self.center.y + dy);
                if self.world_to_screen(coord).is_some() {
                    coords.push(coord);
                }
            }
        }

        coords
    }
}
