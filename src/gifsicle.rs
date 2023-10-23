
pub struct GiflossyImage<'data> {
    img: &'data [u8],
    width: u16,
    height: u16,
    interlace: bool,
    transparent: Option<u8>,
    pal: Option<&'data [RGB8]>,
}

use rgb::RGB8;

use crate::Error;
pub type LzwCode = u16;

#[derive(Clone, Copy)]
pub struct GiflossyWriter {
    pub loss: u32,
}

struct CodeTable {
    pub nodes: Vec<Node>,
    pub links_used: usize,
    pub clear_code: LzwCode,
}

struct Node {
    pub code: LzwCode,
    pub suffix: u8,
    pub children: NodeChild,
}

enum NodeChild {
    /// This emulates old quirks to keep bit-exactly same output
    ThisUsedToBeALinkedList(Vec<Node>),
    Table(Box<[Option<Node>]>),
}

type RgbDiff = rgb::RGB<i16>;

#[inline]
fn color_diff(a: RGB8, b: RGB8, a_transparent: bool, b_transparent: bool, dither: RgbDiff) -> u32 {
    if a_transparent != b_transparent {
        return (1 << 25) as u32;
    }
    if a_transparent {
        return 0;
    }
    let dith =
         ((i32::from(a.r) - i32::from(b.r) + i32::from(dither.r)) * (i32::from(a.r) - i32::from(b.r) + i32::from(dither.r))
        + (i32::from(a.g) - i32::from(b.g) + i32::from(dither.g)) * (i32::from(a.g) - i32::from(b.g) + i32::from(dither.g))
        + (i32::from(a.b) - i32::from(b.b) + i32::from(dither.b)) * (i32::from(a.b) - i32::from(b.b) + i32::from(dither.b))) as u32;
    let undith =
         ((i32::from(a.r) - i32::from(b.r) + i32::from(dither.r) / 2) * (i32::from(a.r) - i32::from(b.r) + i32::from(dither.r) / 2)
        + (i32::from(a.g) - i32::from(b.g) + i32::from(dither.g) / 2) * (i32::from(a.g) - i32::from(b.g) + i32::from(dither.g) / 2)
        + (i32::from(a.b) - i32::from(b.b) + i32::from(dither.b) / 2) * (i32::from(a.b) - i32::from(b.b) + i32::from(dither.b) / 2)) as u32;
    if dith < undith {
        dith
    } else {
        undith
    }
}
#[inline]
fn diffused_difference(
    a: RGB8,
    b: RGB8,
    a_transparent: bool,
    b_transparent: bool,
    dither: RgbDiff,
) -> RgbDiff {
    if a_transparent || b_transparent {
        RgbDiff { r: 0, g: 0, b: 0 }
    } else {
        RgbDiff {
            r: (i32::from(a.r) - i32::from(b.r) + i32::from(dither.r) * 3 / 4) as i16,
            g: (i32::from(a.g) - i32::from(b.g) + i32::from(dither.g) * 3 / 4) as i16,
            b: (i32::from(a.b) - i32::from(b.b) + i32::from(dither.b) * 3 / 4) as i16,
        }
    }
}

impl CodeTable {
    #[inline]
    fn define(&mut self, work_node: &mut Node, suffix: u8, next_code: LzwCode) {
        let next_node = Node {
            code: next_code,
            suffix,
            children: NodeChild::ThisUsedToBeALinkedList(Vec::new()),
        };
        match &mut work_node.children {
            NodeChild::ThisUsedToBeALinkedList(list) if list.len() < 4 || self.links_used + self.clear_code as usize > 0x1000 => {
                if list.is_empty() {
                    list.reserve_exact(4);
                }
                list.push(next_node);
            },
            NodeChild::Table(table) => {
                table[suffix as usize] = Some(next_node);
            },
            NodeChild::ThisUsedToBeALinkedList(list) => {
                self.links_used += self.clear_code as usize;

                let mut table = Vec::with_capacity(self.clear_code as usize);
                debug_assert_eq!(table.capacity(), self.clear_code as usize);
                table.resize_with(self.clear_code as usize, || None);
                let idx = next_node.suffix as usize;
                table[idx] = Some(next_node);

                for node in list.drain(..) {
                    let idx = node.suffix as usize;
                    table[idx] = Some(node);
                }
                work_node.children = NodeChild::Table(table.into_boxed_slice());
            },
        };
    }

    #[cold]
    fn reset(&mut self) {
        self.links_used = 0;
        self.nodes.clear();
        self.nodes.extend((0..usize::from(self.clear_code)).map(|i| Node {
            code: i as u16,
            suffix: i as u8,
            children: NodeChild::ThisUsedToBeALinkedList(Vec::new()),
        }));
    }
}

struct Lookup<'a> {
    pub pal: &'a [RGB8],
    pub image: &'a GiflossyImage<'a>,
    pub max_diff: u32,
    pub best_node: *mut Node,
    pub best_pos: usize,
    pub best_total_diff: u64,
}

impl<'a> Lookup<'a> {
    pub fn lossy_node(&mut self, pos: usize, node: &mut Node, total_diff: u64, dither: RgbDiff) {
        let Some(px) = self.image.px_at_pos(pos) else {
            return;
        };
        match &mut node.children {
            NodeChild::ThisUsedToBeALinkedList(table) => {
                table.iter_mut().rev().for_each(|node| {
                    self.try_node(
                        pos,
                        node,
                        px,
                        node.suffix,
                        dither,
                        total_diff,
                    );
                });
            },
            NodeChild::Table(table) => {
                table.iter_mut().for_each(|node| {
                    if let Some(node) = node.as_mut() {
                        self.try_node(
                            pos,
                            node,
                            px,
                            node.suffix,
                            dither,
                            total_diff,
                        );
                    }
                });
            },
        }
    }

    #[inline]
    fn try_node(
        &mut self,
        pos: usize,
        node: &mut Node,
        px: u8,
        next_px: u8,
        dither: RgbDiff,
        total_diff: u64,
    ) {
        let diff = if px == next_px {
            0
        } else {
            color_diff(
                self.pal[px as usize],
                self.pal[next_px as usize],
                Some(px) == self.image.transparent,
                Some(next_px) == self.image.transparent,
                dither,
            )
        };
        if diff <= self.max_diff {
            let new_dither = diffused_difference(
                self.pal[px as usize],
                self.pal[next_px as usize],
                Some(px) == self.image.transparent,
                Some(next_px) == self.image.transparent,
                dither,
            );
            let new_pos = pos + 1;
            let new_diff = total_diff + u64::from(diff);
            if new_pos > self.best_pos || new_pos == self.best_pos && new_diff < self.best_total_diff {
                self.best_node = node;
                self.best_pos = new_pos;
                self.best_total_diff = new_diff;
            }
            self.lossy_node(
                new_pos,
                node,
                new_diff,
                new_dither,
            );
        }
    }
}

const RUN_EWMA_SHIFT: usize = 4;
const RUN_EWMA_SCALE: usize = 19;
const RUN_INV_THRESH: usize = (1 << RUN_EWMA_SCALE) / 3000;

impl GiflossyWriter {
    pub fn write(&mut self, image: &GiflossyImage, global_pal: Option<&[RGB8]>) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::new();
        buf.try_reserve((image.height as usize * image.width as usize / 4).next_power_of_two())?;

        let mut run = 0;
        let mut run_ewma = 0;
        let mut next_code = 0;
        let pal = image.pal.or(global_pal).unwrap();

        let min_code_size = (pal.len() as u32).max(3).next_power_of_two().trailing_zeros() as u8;

        buf.push(min_code_size);
        let mut bufpos_bits = 8;

        let mut code_table = CodeTable {
            clear_code: 1 << u16::from(min_code_size),
            links_used: 0,
            nodes: Vec::new(),
        };
        code_table.reset();

        let mut cur_code_bits = min_code_size + 1;
        let mut output_code = code_table.clear_code as LzwCode;
        let mut clear_bufpos_bits = bufpos_bits;
        let mut pos = 0;
        let mut clear_pos = pos;
        loop {
            let endpos_bits = bufpos_bits + (cur_code_bits as usize);
            loop {
                if bufpos_bits & 7 != 0 {
                    buf[bufpos_bits / 8] |= (output_code << (bufpos_bits & 7)) as u8;
                } else {
                    buf.push((output_code >> (bufpos_bits + (cur_code_bits as usize) - endpos_bits)) as u8);
                }
                bufpos_bits = bufpos_bits + 8 - (bufpos_bits & 7);
                if bufpos_bits >= endpos_bits {
                    break;
                }
            }
            bufpos_bits = endpos_bits;

            if output_code == code_table.clear_code {
                cur_code_bits = min_code_size + 1;
                next_code = (code_table.clear_code + 2) as LzwCode;
                run_ewma = 1 << RUN_EWMA_SCALE;
                code_table.reset();
                clear_bufpos_bits = 0;
                clear_pos = clear_bufpos_bits;
            } else {
                if output_code == (code_table.clear_code + 1) {
                    break;
                }
                if next_code > (1 << cur_code_bits) && cur_code_bits < 12 {
                    cur_code_bits += 1;
                }
                run = (((run as u32) << RUN_EWMA_SCALE) + (1 << (RUN_EWMA_SHIFT - 1) as u32)) as usize;
                if run < run_ewma {
                    run_ewma = run_ewma - ((run_ewma - run) >> RUN_EWMA_SHIFT);
                } else {
                    run_ewma = run_ewma + ((run - run_ewma) >> RUN_EWMA_SHIFT);
                }
            }
            if let Some(px) = image.px_at_pos(pos) {
                let mut l = Lookup {
                    pal,
                    image,
                    max_diff: self.loss,
                    best_node: &mut code_table.nodes[px as usize],
                    best_pos: pos + 1,
                    best_total_diff: 0,
                };
                l.lossy_node(pos + 1, &mut code_table.nodes[px as usize], 0, RgbDiff { r: 0, g: 0, b: 0 }, );
                let selected_node = unsafe { &mut * l.best_node };
                run = l.best_pos - pos;
                pos = l.best_pos;
                output_code = selected_node.code;
                if let Some(px) = image.px_at_pos(pos) {
                    if next_code < 0x1000 {
                        code_table.define(selected_node, px, next_code);
                        next_code += 1;
                    } else {
                        next_code = 0x1001;
                    }
                    if next_code >= 0x0FFF {
                        let pixels_left = image.img.len() - pos - 1;
                        let do_clear = pixels_left != 0
                            && (run_ewma
                                < (36 << RUN_EWMA_SCALE) / (min_code_size as usize)
                                || pixels_left > (0x7FFF_FFFF * 2 + 1) / RUN_INV_THRESH
                                || run_ewma < pixels_left * RUN_INV_THRESH);
                        if (do_clear || run < 7) && clear_pos == 0 {
                            clear_pos = pos - run;
                            clear_bufpos_bits = bufpos_bits;
                        } else if !do_clear && run > 50 {
                            clear_bufpos_bits = 8; // buf contains min code
                            clear_pos = 0;
                        }
                        if do_clear {
                            output_code = code_table.clear_code;
                            pos = clear_pos;
                            bufpos_bits = clear_bufpos_bits;
                            buf.truncate((bufpos_bits + 7) / 8);
                            if buf.len() > bufpos_bits / 8 {
                                buf[bufpos_bits / 8] &= (1 << (bufpos_bits & 7)) - 1;
                            }
                            continue;
                        }
                    }
                    run = (((run as u32) << RUN_EWMA_SCALE) + (1 << (RUN_EWMA_SHIFT - 1) as u32)) as usize;
                    if run < run_ewma {
                        run_ewma = run_ewma - ((run_ewma - run) >> RUN_EWMA_SHIFT);
                    } else {
                        run_ewma = run_ewma + ((run - run_ewma) >> RUN_EWMA_SHIFT);
                    }
                }
            } else {
                run = 0;
                output_code = code_table.clear_code + 1;
            };
        }
        Ok(buf)
    }
}

impl<'a> GiflossyImage<'a> {
    #[must_use]
    pub fn new(
        img: &'a [u8],
        width: u16,
        height: u16,
        transparent: Option<u8>,
        pal: Option<&'a [RGB8]>,
    ) -> Self {
        assert_eq!(img.len(), width as usize * height as usize);
        GiflossyImage {
            img,
            width,
            height,
            interlace: false,
            transparent,
            pal,
        }
    }

    #[inline]
    fn px_at_pos(&self, pos: usize) -> Option<u8> {
        if !self.interlace {
            self.img.get(pos).copied()
        } else {
            let y = pos / self.width as usize;
            let x = pos - (y * self.width as usize);
            self.img.get(self.width as usize * interlaced_line(y, self.height as usize) + x).copied()
        }
    }
}

fn interlaced_line(line: usize, height: usize) -> usize {
    if line > height / 2 {
        line * 2 - (height | 1)
    } else if line > height / 4 {
        return line * (4) - (height & !(1) | 2);
    } else if line > height / 8 {
        return line * (8) - (height & !(3) | 4);
    } else {
        return line * 8;
    }
}
