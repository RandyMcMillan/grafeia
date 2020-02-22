use super::{Entry, StreamVec, FlexMeasure, ItemMeasure};
//use layout::style::{Style};
use crate::units::{Length, Size};
use std::fmt::{self, Debug};
use crate::content::{Font, Tag};
use crate::draw::RenderItem;

#[derive(Copy, Clone, Debug, Default)]
struct LineBreak {
    prev:   usize, // index to previous line-break
    path:   u64, // one bit for each branch taken (1) or not (0)
    factor: f32,
    score:  f32,
    height: Length,
}

#[derive(Copy, Clone, Debug, Default)]
struct ColumnBreak {
    prev:   usize, // index to previous column-break
    score:  f32,
}
    
#[derive(Copy, Clone, Debug, Default)]
struct Break {
    line:   LineBreak,
    column: Option<ColumnBreak>
}

#[derive(Debug)]
pub struct ParagraphStyle {
    pub font: Font,
    pub leading: f32,
    pub par_indent: f32
}

pub struct ParagraphLayout {
    items:      Vec<Entry>,
    nodes:      Vec<Option<LineBreak>>,
    width:      Length,
    last:       usize
}
pub struct ColumnLayout {
    para:       ParagraphLayout,
    nodes_col:  Vec<Option<ColumnBreak>>,
    height:     Length
}
impl Debug for ColumnLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ColumnLayout")
    }
}
impl Debug for ParagraphLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ParagraphLayout")
    }
}

struct Context {
    measure:    FlexMeasure,
    path:       u64,    // one bit for each branch on this line
    begin:      usize,  // begin of line or branch
    pos:        usize,  // calculation starts here
    score:      f32,    // score at pos
    branches:   u8,     // number of branches so far (<= 64)
    overflow:   FlexMeasure, // how much to overflow into the margin
}
impl Context {
    fn new(start: usize, score: f32) -> Context {
        Context {
            measure:    FlexMeasure::zero(),
            path:       0,
            begin:      start,
            pos:        start,
            branches:   0,
            score:      score,
            overflow:   FlexMeasure::zero(),
        }
    }
    fn add_item(&mut self, measure: ItemMeasure, is_first: bool) {
        if !is_first {
            self.measure += measure.left;
        }
        self.measure += self.overflow + measure.content;
        self.overflow = measure.right;
    }
    fn line(&self) -> FlexMeasure {
        self.measure
    }
    fn fill(&mut self, width: Length) {
        self.measure = self.line();
        self.measure.extend(width);
        self.overflow = FlexMeasure::zero();
    }
}

impl ParagraphLayout {
    pub fn new(items: StreamVec, width: Length) -> ParagraphLayout {
        let limit = items.0.len();
        let mut nodes = vec![None; limit+1];
        nodes[0] = Some(LineBreak::default());

        let mut layout = ParagraphLayout {
            nodes,
            items: items.0,
            width,
            last: 0
        };
        layout.run();
        layout
    }
    fn run(&mut self) {
        let mut last = 0;
        for start in 0 .. self.items.len() {
            match self.nodes[start] {
                Some(b) => {
                    last = self.complete_line(
                        start,
                        Context::new(start, b.score)
                    );
                },
                None => {}
            }
        }

        if self.nodes[last].is_none() {
            for i in 0 .. last {
                println!("{:3} {:?}", i, self.items[i]);
                if let Some(b) = self.nodes[i] {
                    println!("     {:?}", b);
                }
            }
        }

        self.last = last;
    }

    fn complete_line(&mut self, start: usize, mut c: Context) -> usize {
        let mut last = c.begin;
        
        while c.pos < self.items.len() {
            let n = c.pos;
            let is_first = start == n;
            match self.items[n] {
                Entry::Item(m, _, _) => c.add_item(m, is_first),
                Entry::Space(s, breaking) => {
                    if breaking {
                        // breaking case:
                        // width is not added yet!
                        self.maybe_update(&c, n+1);
                        last = n+1;
                    }
                    
                    // add width now.
                    c.measure += s;
                }
                Entry::Linebreak(fill) => {
                    if fill {
                        c.fill(self.width);
                    }
                    
                    self.maybe_update(&c, n+1);
                    last = n+1;
                    break;
                },
                Entry::BranchEntry(len) => {
                    // b
                    let b_last = self.complete_line(
                        start,
                        Context {
                            pos:        n + 1,
                            path:       c.path | (1 << c.branches),
                            branches:   c.branches + 1,
                            ..          c
                        }
                    );
                    if b_last > last {
                        last = b_last;
                    }
                    
                    // a follows here
                    c.pos += len;
                    c.branches += 1;
                },
                Entry::BranchExit(skip) => {
                    c.pos += skip;
                }
            }
            
            if c.measure.shrink > self.width {
                break; // too full
            }
            
            c.pos += 1;
        }
        
        last
    }

    fn maybe_update(&mut self, c: &Context, n: usize) {
        let (factor, score) = match c.line().factor(self.width) {
            Some(factor) => (factor, -factor * factor),
            None => (1.0, -1000.)
        };

        let break_score = c.score + score;
        let break_point = LineBreak {
            prev:   c.begin,
            path:   c.path,
            factor: factor,
            score:  break_score,
            height: c.measure.height
        };
        self.nodes[n] = Some(match self.nodes[n] {
            Some(line) if break_score <= line.score => line,
            _ => break_point
        });
    }
    pub fn lines(&self) -> Column {
        Column::new(0, self.last, self)
    }
}
impl ColumnLayout {
    pub fn new(items: StreamVec, width: Length, height: Length) -> ColumnLayout {
        let limit = items.0.len();
        let mut nodes = vec![None; limit+1];
        let mut nodes_col = vec![None; limit+1];
        nodes[0] = Some(LineBreak::default());
        nodes_col[0] = Some(ColumnBreak::default());

        let mut layout = ColumnLayout {
            para: ParagraphLayout {
                nodes,
                items: items.0,
                width,
                last: 0
            },
            nodes_col,
            height,
        };
        layout.run();
        layout
    }
    pub fn columns(self) -> Columns {
        Columns::new(self)
    }
    fn run(&mut self) {
        let mut last = 0;
        for start in 0 .. self.para.items.len() {
            match self.para.nodes[start] {
                Some(b) => {
                    last = self.para.complete_line(
                        start,
                        Context::new(start, b.score)
                    );
                    self.compute_column(start, false);
                },
                None => {}
            }
        }
        self.compute_column(last, true);

        if self.nodes_col[last].is_none() {
            for i in 0 .. last {
                println!("{:3} {:?}", i, self.para.items[i]);
                if let Some(b) = self.para.nodes[i] {
                    println!("     {:?}", b);
                }
                if let Some(l) = self.nodes_col[i] {
                    println!("     {:?}", l);
                }
            }
        }

        self.para.last = last;
    }

    fn num_lines_penalty(&self, n: usize) -> f32 {
        match n {
            1 => -20.0,
            2 => -2.0,
            _ => 0.0
        }
    }
    fn fill_penalty(&self, fill: Length) -> f32 {
        -10.0 * ((self.height - fill) / self.height)
    }

    fn compute_column(&mut self, n: usize, is_last: bool) -> bool {
        //                                        measure:
        let mut num_lines_before_end = 0;      // - lines before the break; reset between paragraphs
        let mut num_lines_at_last_break = 0;   // - lines after the previous break; count until the last paragraph starts
        let mut is_last_paragraph = true;
        let mut height = Length::zero();
        let mut last = n;
        let mut found = false;
        
        loop {
            let last_node = self.para.nodes[last].unwrap();
                        
            if last > 0 {
                match self.para.items[last-1] {
                    Entry::Linebreak(_) => {
                        is_last_paragraph = false;
                        num_lines_before_end = 0;
                    },
                    Entry::Space { .. } => {
                        num_lines_before_end += 1;

                        if is_last_paragraph {
                            num_lines_at_last_break += 1;
                        }
                    }
                    ref e => panic!("found: {:?}", e)
                }
                
                height += last_node.height;

                if height > self.height {
                    break;
                }
            }

            if let Some(column) = self.nodes_col[last] {
                let mut score = column.score
                    + self.num_lines_penalty(num_lines_at_last_break)
                    + self.num_lines_penalty(num_lines_before_end);
                
                if !is_last {
                    score += self.fill_penalty(height);
                }
            
                match self.nodes_col[n] {
                    Some(column) if column.score > score => {},
                    _ => {
                        self.nodes_col[n] = Some(ColumnBreak {
                            prev: last,
                            score: score
                        });
                        
                        found = true;
                    }
                }
            }

            if last == 0 {
                break;
            }
            last = last_node.prev;
        }
        
        found
    }
}

#[derive(Debug)]
pub struct Columns {
    layout:     ColumnLayout,
    columns:    Vec<usize>
}
impl Columns {
    fn new(layout: ColumnLayout) -> Self {
        let mut columns = Vec::new();
        let mut last = layout.para.last;
        while last > 0 {
            columns.push(last);
            last = layout.nodes_col[last].unwrap().prev;
        }
        Columns {
            layout: layout,
            columns: columns
        }
    }
    pub fn get_column(&self, n: usize) -> Column {
        let len = self.columns.len();
        assert!(n < len);
        let last = self.columns[len - 1 - n];
        Column::new(
            self.layout.nodes_col[last].unwrap().prev,
            last,
            &self.layout.para
        )
    }
    pub fn columns(&self) -> impl Iterator<Item=Column> {
        self.columns.iter().rev().map(move |&last| Column::new(
            self.layout.nodes_col[last].unwrap().prev,
            last,
            &self.layout.para
        ))
    }
    pub fn len(&self) -> usize {
        self.columns.len()
    }
}

#[derive(Debug)]
pub struct Column<'a> {
    lines:      Vec<usize>, // points to the end of each line
    layout:     &'a ParagraphLayout,
    y:          Length
}
impl<'a> Column<'a> {
    fn new(first: usize, mut last: usize, layout: &'a ParagraphLayout) -> Self {
        let mut lines = Vec::new();
        while last > first {
            lines.push(last);
            last = layout.nodes[last].unwrap().prev;
        }
        
        Column {
            lines: lines,
            layout: layout,
            y: Length::zero()
        }
    }
}
impl<'a> Iterator for Column<'a> {
    type Item = (Length, Line<'a>);
    
    fn next(&mut self) -> Option<Self::Item> {
        self.lines.pop().map(|last| {
            let b = self.layout.nodes[last].unwrap();
            self.y += b.height;
            
            (self.y, Line {
                layout:   self.layout,
                pos:      b.prev,
                branches: 0,
                measure:  FlexMeasure::zero(),
                line:     b,
                end:      last-1,
            })
        })
    }
}

#[derive(Debug)]
pub struct Line<'a> {
    layout:     &'a ParagraphLayout,
    pos:        usize,
    end:        usize,
    branches:   usize,
    measure:    FlexMeasure,
    line:       LineBreak,
}
impl<'a> Line<'a> {
    pub fn height(&self) -> Length {
        self.line.height
    }
}
impl<'a> Iterator for Line<'a> {
    type Item = (Length, Size, RenderItem, Tag);
    fn next(&mut self) -> Option<Self::Item> {
        while self.pos < self.end {
            let pos = self.pos;
            self.pos += 1;
            let is_first = self.pos != self.line.prev;

            match self.layout.items[pos] {
                Entry::Item(m, item, tag) => {
                    if !is_first {
                        self.measure += m.left;
                    }
                    let x = self.measure.at(self.line.factor);
                    self.measure += m.content + m.right;
                    let total = m.left + m.content + m.right;
                    let size = Size::new(total.at(self.line.factor), total.height);
                    return Some((x, size, item, tag));
                },
                Entry::Space(s, _) => {
                    self.measure += s;
                },
                Entry::BranchEntry(len) => {
                    if self.line.path & (1<<self.branches) == 0 {
                        // not taken
                        self.pos += len;
                    }
                    self.branches += 1;
                },
                Entry::BranchExit(skip) => self.pos += skip,
                Entry::Linebreak(_) => unreachable!(),
            }
        }
        
        None
    }
}
