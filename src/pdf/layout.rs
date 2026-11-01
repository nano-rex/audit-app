//! Page layout: paragraphs, tables, images, and drawings flowed down A4 pages, as reportlab's
//! platypus lays out a story. Tables split between rows (repeating their header), paragraphs
//! between lines; KeepTogether and CondPageBreak keep related parts on one page.

use super::doc::{Canvas, Colour, Document, Face, A4};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VAlign {
    Top,
    Middle,
}

#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub face: Face,
    pub size: f64,
    pub leading: f64,
    pub colour: Colour,
    pub align: Align,
    pub space_before: f64,
    pub space_after: f64,
}

/// A run of text in one face, size, and colour, optionally on a coloured background.
#[derive(Debug, Clone)]
pub struct Span {
    pub text: String,
    pub face: Option<Face>,
    pub size: Option<f64>,
    pub colour: Option<Colour>,
    pub back: Option<Colour>,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Span {
        Span { text: text.into(), face: None, size: None, colour: None, back: None }
    }

    pub fn bold(text: impl Into<String>) -> Span {
        Span { face: Some(Face::Bold), ..Span::plain(text) }
    }

    pub fn coloured(mut self, colour: Colour) -> Span {
        self.colour = Some(colour);
        self
    }

    pub fn sized(mut self, size: f64) -> Span {
        self.size = Some(size);
        self
    }

    pub fn on(mut self, back: Colour) -> Span {
        self.back = Some(back);
        self
    }
}

#[derive(Debug, Clone)]
pub struct Paragraph {
    pub spans: Vec<Span>,
    pub style: Style,
}

/// A piece of a line: text of one span, or a space.
#[derive(Debug, Clone)]
struct Piece {
    text: String,
    face: Face,
    size: f64,
    colour: Colour,
    back: Option<Colour>,
    width: f64,
}

#[derive(Debug, Clone)]
struct Line {
    pieces: Vec<Piece>,
    width: f64,
}

/// Something drawn at fixed coordinates inside a drawing (origin at its bottom left).
#[derive(Debug, Clone)]
pub enum Shape {
    Rect { x: f64, y: f64, width: f64, height: f64, fill: Option<Colour>, stroke: Option<(Colour, f64)> },
    Line { x1: f64, y1: f64, x2: f64, y2: f64, colour: Colour, width: f64, dash: Option<Vec<f64>> },
    Circle { x: f64, y: f64, radius: f64, fill: Colour, stroke: Option<(Colour, f64)> },
    Polygon { points: Vec<(f64, f64)>, fill: Colour, stroke: Option<(Colour, f64)> },
    Text { x: f64, y: f64, text: String, face: Face, size: f64, colour: Colour, anchor: Align },
}

#[derive(Debug, Clone)]
pub struct Drawing {
    pub width: f64,
    pub height: f64,
    pub shapes: Vec<Shape>,
}

impl Drawing {
    pub fn new(width: f64, height: f64) -> Drawing {
        Drawing { width, height, shapes: Vec::new() }
    }

    pub fn add(&mut self, shape: Shape) {
        self.shapes.push(shape);
    }

    pub fn text(&mut self, x: f64, y: f64, text: impl Into<String>, face: Face, size: f64, colour: Colour, anchor: Align) {
        self.shapes.push(Shape::Text { x, y, text: text.into(), face, size, colour, anchor });
    }

    pub fn rect(&mut self, x: f64, y: f64, width: f64, height: f64, fill: Colour) {
        self.shapes.push(Shape::Rect { x, y, width, height, fill: Some(fill), stroke: None });
    }

    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, colour: Colour, width: f64, dash: Option<Vec<f64>>) {
        self.shapes.push(Shape::Line { x1, y1, x2, y2, colour, width, dash });
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Picture {
    pub index: usize,
    pub width: f64,
    pub height: f64,
}

/// A table style command over a range of cells; negative indices count from the end.
#[derive(Debug, Clone)]
pub enum Cmd {
    Background((i32, i32), (i32, i32), Colour),
    LineBelow((i32, i32), (i32, i32), f64, Colour),
    LineBefore((i32, i32), (i32, i32), f64, Colour),
    Valign((i32, i32), (i32, i32), VAlign),
    Align((i32, i32), (i32, i32), Align),
    LeftPadding((i32, i32), (i32, i32), f64),
    RightPadding((i32, i32), (i32, i32), f64),
    TopPadding((i32, i32), (i32, i32), f64),
    BottomPadding((i32, i32), (i32, i32), f64),
    Span((i32, i32), (i32, i32)),
    /// A rule round the range as a whole.
    Box((i32, i32), (i32, i32), f64, Colour),
}

pub const ALL: ((i32, i32), (i32, i32)) = ((0, 0), (-1, -1));

#[derive(Debug, Clone)]
pub struct Table {
    pub rows: Vec<Vec<Vec<Flow>>>,
    pub widths: Vec<Option<f64>>,
    pub repeat_rows: usize,
    pub commands: Vec<Cmd>,
    /// A rule round the whole table, and its corner radius.
    pub outline: Option<(f64, Colour)>,
    pub radius: f64,
    pub h_align: Align,
}

impl Table {
    pub fn new(rows: Vec<Vec<Vec<Flow>>>, widths: Vec<Option<f64>>) -> Table {
        Table { rows, widths, repeat_rows: 0, commands: Vec::new(), outline: None, radius: 0.0, h_align: Align::Center }
    }

    pub fn style(mut self, commands: Vec<Cmd>) -> Table {
        self.commands.extend(commands);
        self
    }
}

#[derive(Debug, Clone)]
pub enum Flow {
    Para(Paragraph),
    Spacer(f64),
    Table(Box<Table>),
    Image(Picture),
    Drawing(Drawing),
    KeepTogether(Vec<Flow>),
    CondPageBreak(f64),
    PageBreak,
}

/// The layout of a table at a given width: column widths, row heights, and each cell's laid-out content.
struct TableLayout {
    columns: Vec<f64>,
    heights: Vec<f64>,
    cells: Vec<Vec<CellLayout>>,
    /// Outlined ranges: first column, first row, last column, last row, width, colour.
    boxes: Vec<(usize, usize, usize, usize, f64, Colour)>,
}

#[derive(Clone)]
struct CellLayout {
    span: usize,
    hidden: bool,
    background: Option<Colour>,
    valign: VAlign,
    align: Align,
    pad: [f64; 4],
    content: Vec<(Laid, f64)>,
    height: f64,
    line_below: Option<(f64, Colour)>,
    line_before: Option<(f64, Colour)>,
}

/// A flowable laid out at a width: its height, ready to draw.
#[derive(Clone)]
enum Laid {
    Lines(Vec<Line>, Style),
    Space,
    Table(Box<Table>, std::rc::Rc<TableLayoutCell>),
    Image(Picture),
    Drawing(Drawing),
    Group(Vec<(Laid, f64)>),
}

struct TableLayoutCell(TableLayout);

pub struct Layout<'a> {
    pub doc: &'a mut Document,
    pub margin: f64,
    pub bottom: f64,
    canvas: Canvas,
    y: f64,
    footer: &'a dyn Fn(&mut Document, &mut Canvas, usize),
}

fn resolve(index: i32, count: usize) -> usize {
    if index < 0 {
        (count as i32 + index).max(0) as usize
    } else {
        (index as usize).min(count.saturating_sub(1))
    }
}

fn in_range(cell: (usize, usize), from: (i32, i32), to: (i32, i32), columns: usize, rows: usize) -> bool {
    let (c0, r0, c1, r1) = (resolve(from.0, columns), resolve(from.1, rows), resolve(to.0, columns), resolve(to.1, rows));
    cell.0 >= c0.min(c1) && cell.0 <= c0.max(c1) && cell.1 >= r0.min(r1) && cell.1 <= r0.max(r1)
}

impl<'a> Layout<'a> {
    pub fn new(doc: &'a mut Document, footer: &'a dyn Fn(&mut Document, &mut Canvas, usize)) -> Layout<'a> {
        Layout { doc, margin: 36.0, bottom: 44.0, canvas: Canvas::default(), y: A4.1 - 36.0, footer }
    }

    pub fn width(&self) -> f64 {
        A4.0 - 2.0 * self.margin
    }

    fn top(&self) -> f64 {
        A4.1 - self.margin
    }

    fn frame_height(&self) -> f64 {
        self.top() - self.bottom
    }

    fn available(&self) -> f64 {
        self.y - self.bottom
    }

    fn at_top(&self) -> bool {
        (self.y - self.top()).abs() < 0.01
    }

    pub fn new_page(&mut self) {
        let mut canvas = std::mem::take(&mut self.canvas);
        let number = self.doc.page_count() + 1;
        (self.footer)(self.doc, &mut canvas, number);
        self.doc.add_page(canvas);
        self.y = self.top();
    }

    pub fn finish(mut self) {
        self.new_page();
    }

    // ---- Measuring ----

    fn pieces(&self, paragraph: &Paragraph) -> Vec<Vec<Piece>> {
        // Words (with their trailing spaces kept separate) per forced line.
        let mut lines: Vec<Vec<Piece>> = vec![Vec::new()];
        for span in &paragraph.spans {
            let face = span.face.unwrap_or(paragraph.style.face);
            let size = span.size.unwrap_or(paragraph.style.size);
            let colour = span.colour.unwrap_or(paragraph.style.colour);
            for (index, part) in span.text.split('\n').enumerate() {
                if index > 0 {
                    lines.push(Vec::new());
                }
                let mut word = String::new();
                let flush = |word: &mut String, lines: &mut Vec<Vec<Piece>>| {
                    if !word.is_empty() {
                        let shown = word.replace('\u{a0}', " ");
                        let width = self.doc.fonts.width(&shown, face, size);
                        lines.last_mut().unwrap().push(Piece { text: shown, face, size, colour, back: span.back, width });
                        word.clear();
                    }
                };
                for ch in part.chars() {
                    if ch == ' ' || ch == '\t' {
                        flush(&mut word, &mut lines);
                        let width = self.doc.fonts.width(" ", face, size);
                        lines.last_mut().unwrap().push(Piece { text: " ".into(), face, size, colour, back: span.back, width });
                    } else {
                        word.push(ch);
                    }
                }
                flush(&mut word, &mut lines);
            }
        }
        lines
    }

    fn wrap_paragraph(&self, paragraph: &Paragraph, width: f64) -> Vec<Line> {
        let mut out = Vec::new();
        for pieces in self.pieces(paragraph) {
            let mut line = Line { pieces: Vec::new(), width: 0.0 };
            for piece in pieces {
                let is_space = piece.text == " ";
                if is_space && line.pieces.is_empty() {
                    continue;
                }
                if !is_space && line.width + piece.width > width + 0.01 && !line.pieces.is_empty() {
                    while line.pieces.last().is_some_and(|last| last.text == " ") {
                        let removed = line.pieces.pop().unwrap();
                        line.width -= removed.width;
                    }
                    out.push(std::mem::replace(&mut line, Line { pieces: Vec::new(), width: 0.0 }));
                }
                if !is_space && piece.width > width + 0.01 {
                    // A word longer than the line is broken between characters.
                    let mut part = String::new();
                    let mut part_width = 0.0;
                    for ch in piece.text.chars() {
                        let ch_width = self.doc.fonts.width(&ch.to_string(), piece.face, piece.size);
                        if part_width + ch_width > width - line.width && !part.is_empty() {
                            line.pieces.push(Piece { text: std::mem::take(&mut part), width: part_width, ..piece.clone() });
                            line.width += part_width;
                            out.push(std::mem::replace(&mut line, Line { pieces: Vec::new(), width: 0.0 }));
                            part_width = 0.0;
                        }
                        part.push(ch);
                        part_width += ch_width;
                    }
                    if !part.is_empty() {
                        line.width += part_width;
                        line.pieces.push(Piece { text: part, width: part_width, ..piece });
                    }
                    continue;
                }
                line.width += piece.width;
                line.pieces.push(piece);
            }
            while line.pieces.last().is_some_and(|last| last.text == " ") {
                let removed = line.pieces.pop().unwrap();
                line.width -= removed.width;
            }
            out.push(line);
        }
        out
    }

    /// Lay a flowable out at a width; returns it and its height.
    fn lay(&self, flow: &Flow, width: f64) -> (Laid, f64) {
        match flow {
            Flow::Para(paragraph) => {
                let lines = self.wrap_paragraph(paragraph, width);
                let height = lines.len() as f64 * paragraph.style.leading;
                (Laid::Lines(lines, paragraph.style), height)
            }
            Flow::Spacer(height) => (Laid::Space, *height),
            Flow::Table(table) => {
                let layout = self.layout_table(table, width);
                let height = layout.heights.iter().sum();
                (Laid::Table(table.clone(), std::rc::Rc::new(TableLayoutCell(layout))), height)
            }
            Flow::Image(picture) => (Laid::Image(*picture), picture.height),
            Flow::Drawing(drawing) => (Laid::Drawing(drawing.clone()), drawing.height),
            Flow::KeepTogether(children) => {
                let laid: Vec<(Laid, f64)> = children.iter().map(|child| self.lay(child, width)).collect();
                let height = laid.iter().map(|(_, height)| height).sum();
                (Laid::Group(laid), height)
            }
            Flow::CondPageBreak(_) | Flow::PageBreak => (Laid::Space, 0.0),
        }
    }

    fn intrinsic_width(&self, flow: &Flow) -> f64 {
        match flow {
            Flow::Image(picture) => picture.width,
            Flow::Drawing(drawing) => drawing.width,
            Flow::Para(paragraph) => self.pieces(paragraph).iter().map(|line| line.iter().map(|piece| piece.width).sum::<f64>()).fold(0.0, f64::max),
            Flow::Table(table) => table.widths.iter().map(|width| width.unwrap_or(0.0)).sum(),
            Flow::KeepTogether(children) => children.iter().map(|child| self.intrinsic_width(child)).fold(0.0, f64::max),
            _ => 0.0,
        }
    }

    fn layout_table(&self, table: &Table, _available: f64) -> TableLayout {
        let rows = table.rows.len();
        let columns = table.widths.len();
        let mut cells: Vec<Vec<CellLayout>> = Vec::with_capacity(rows);
        for row in 0..rows {
            let mut line = Vec::with_capacity(columns);
            for column in 0..columns {
                let mut cell = CellLayout {
                    span: 1,
                    hidden: false,
                    background: None,
                    valign: VAlign::Top,
                    align: Align::Left,
                    pad: [6.0, 6.0, 3.0, 3.0],
                    content: Vec::new(),
                    height: 0.0,
                    line_below: None,
                    line_before: None,
                };
                for command in &table.commands {
                    let here = |from: (i32, i32), to: (i32, i32)| in_range((column, row), from, to, columns, rows);
                    match command {
                        Cmd::Background(from, to, colour) if here(*from, *to) => cell.background = Some(*colour),
                        Cmd::LineBelow(from, to, width, colour) if here(*from, *to) => cell.line_below = Some((*width, *colour)),
                        Cmd::LineBefore(from, to, width, colour) if here(*from, *to) => cell.line_before = Some((*width, *colour)),
                        Cmd::Valign(from, to, valign) if here(*from, *to) => cell.valign = *valign,
                        Cmd::Align(from, to, align) if here(*from, *to) => cell.align = *align,
                        Cmd::LeftPadding(from, to, value) if here(*from, *to) => cell.pad[0] = *value,
                        Cmd::RightPadding(from, to, value) if here(*from, *to) => cell.pad[1] = *value,
                        Cmd::TopPadding(from, to, value) if here(*from, *to) => cell.pad[2] = *value,
                        Cmd::BottomPadding(from, to, value) if here(*from, *to) => cell.pad[3] = *value,
                        _ => {}
                    }
                }
                line.push(cell);
            }
            cells.push(line);
        }
        for command in &table.commands {
            if let Cmd::Span(from, to) = command {
                let (c0, r0, c1) = (resolve(from.0, columns), resolve(from.1, rows), resolve(to.0, columns));
                let r1 = resolve(to.1, rows);
                for row in r0..=r1 {
                    cells[row][c0].span = c1 - c0 + 1;
                    for cell in cells[row].iter_mut().take(c1 + 1).skip(c0 + 1) {
                        cell.hidden = true;
                    }
                }
            }
        }
        let widths: Vec<f64> = (0..columns)
            .map(|column| {
                table.widths[column].unwrap_or_else(|| {
                    (0..rows)
                        .map(|row| {
                            let cell = &cells[row][column];
                            table.rows[row][column].iter().map(|flow| self.intrinsic_width(flow)).fold(0.0, f64::max) + cell.pad[0] + cell.pad[1]
                        })
                        .fold(0.0, f64::max)
                })
            })
            .collect();
        let mut heights = vec![0.0; rows];
        for row in 0..rows {
            for column in 0..columns {
                if cells[row][column].hidden {
                    continue;
                }
                let span = cells[row][column].span;
                let width: f64 = widths[column..(column + span).min(columns)].iter().sum::<f64>() - cells[row][column].pad[0] - cells[row][column].pad[1];
                let content: Vec<(Laid, f64)> = table.rows[row][column].iter().map(|flow| self.lay(flow, width.max(1.0))).collect();
                let height: f64 = content.iter().map(|(_, height)| height).sum();
                let cell = &mut cells[row][column];
                cell.content = content;
                cell.height = height;
                heights[row] = f64::max(heights[row], height + cell.pad[2] + cell.pad[3]);
            }
        }
        let boxes = table
            .commands
            .iter()
            .filter_map(|command| match command {
                Cmd::Box(from, to, width, colour) => {
                    let (c0, r0, c1, r1) = (resolve(from.0, columns), resolve(from.1, rows), resolve(to.0, columns), resolve(to.1, rows));
                    Some((c0.min(c1), r0.min(r1), c0.max(c1), r0.max(r1), *width, *colour))
                }
                _ => None,
            })
            .collect();
        TableLayout { columns: widths, heights, cells, boxes }
    }

    // ---- Drawing ----

    fn draw_lines(&mut self, lines: &[Line], style: &Style, x: f64, top: f64, width: f64) {
        for (index, line) in lines.iter().enumerate() {
            let baseline = top - index as f64 * style.leading - 0.86 * style.size - (style.leading - 1.2 * style.size) / 2.0;
            let mut cursor = match style.align {
                Align::Left => x,
                Align::Right => x + width - line.width,
                Align::Center => x + (width - line.width) / 2.0,
            };
            for piece in &line.pieces {
                if let Some(back) = piece.back {
                    self.canvas.rect(cursor, baseline - 0.25 * piece.size, piece.width, piece.size * 1.2, Some(back), None);
                }
                if piece.text != " " {
                    self.doc.text(&mut self.canvas, cursor, baseline, &piece.text, piece.face, piece.size, piece.colour);
                }
                cursor += piece.width;
            }
        }
    }

    fn draw_drawing(&mut self, drawing: &Drawing, x: f64, y: f64) {
        for shape in &drawing.shapes {
            match shape {
                Shape::Rect { x: sx, y: sy, width, height, fill, stroke } => {
                    if *width > 0.0 || *height > 0.0 {
                        self.canvas.rect(x + sx, y + sy, *width, *height, *fill, *stroke);
                    }
                }
                Shape::Line { x1, y1, x2, y2, colour, width, dash } => self.canvas.line(x + x1, y + y1, x + x2, y + y2, *colour, *width, dash.as_deref()),
                Shape::Circle { x: cx, y: cy, radius, fill, stroke } => self.canvas.circle(x + cx, y + cy, *radius, *fill, *stroke),
                Shape::Polygon { points, fill, stroke } => {
                    let moved: Vec<(f64, f64)> = points.iter().map(|(px, py)| (x + px, y + py)).collect();
                    self.canvas.polygon(&moved, *fill, *stroke);
                }
                Shape::Text { x: tx, y: ty, text, face, size, colour, anchor } => {
                    let width = self.doc.fonts.width(text, *face, *size);
                    let start = match anchor {
                        Align::Left => x + tx,
                        Align::Right => x + tx - width,
                        Align::Center => x + tx - width / 2.0,
                    };
                    self.doc.text(&mut self.canvas, start, y + ty, text, *face, *size, *colour);
                }
            }
        }
    }

    /// Draw a laid-out flowable with its top at `top`, starting at `x` in a box of `width`.
    fn draw_laid(&mut self, laid: &Laid, x: f64, top: f64, width: f64, align: Align) {
        match laid {
            Laid::Lines(lines, style) => self.draw_lines(lines, style, x, top, width),
            Laid::Space => {}
            Laid::Image(picture) => {
                let offset = match align {
                    Align::Left => 0.0,
                    Align::Right => width - picture.width,
                    Align::Center => (width - picture.width) / 2.0,
                };
                self.doc.draw_image(&mut self.canvas, picture.index, x + offset, top - picture.height, picture.width, picture.height);
            }
            Laid::Drawing(drawing) => {
                let offset = match align {
                    Align::Left => 0.0,
                    Align::Right => width - drawing.width,
                    Align::Center => (width - drawing.width) / 2.0,
                };
                self.draw_drawing(drawing, x + offset, top - drawing.height);
            }
            Laid::Table(table, layout) => self.draw_table(table, &layout.0, x, top, width, 0, layout.0.heights.len(), None),
            Laid::Group(children) => {
                let mut cursor = top;
                for (child, height) in children {
                    self.draw_laid(child, x, cursor, width, align);
                    cursor -= height;
                }
            }
        }
    }

    /// Draw rows [first, last) of a table, preceded by its repeated header rows when `header` is set.
    #[allow(clippy::too_many_arguments)]
    fn draw_table(&mut self, table: &Table, layout: &TableLayout, x: f64, top: f64, width: f64, first: usize, last: usize, header: Option<usize>) {
        let total: f64 = layout.columns.iter().sum();
        let left = match table.h_align {
            Align::Left => x,
            Align::Right => x + width - total,
            Align::Center => x + (width - total) / 2.0,
        };
        let mut rows: Vec<usize> = (0..header.unwrap_or(0)).collect();
        rows.extend(first..last);
        let height: f64 = rows.iter().map(|row| layout.heights[*row]).sum();
        let starts: Vec<f64> = (0..layout.columns.len()).map(|column| left + layout.columns[..column].iter().sum::<f64>()).collect();
        // Backgrounds, clipped to the rounded outline.
        self.canvas.save();
        if table.radius > 0.0 {
            self.clip_round(left, top - height, total, height, table.radius);
        }
        let mut cursor = top;
        for row in &rows {
            let row_height = layout.heights[*row];
            for (column, cell) in layout.cells[*row].iter().enumerate() {
                if cell.hidden {
                    continue;
                }
                if let Some(colour) = cell.background {
                    let span_width: f64 = layout.columns[column..(column + cell.span).min(layout.columns.len())].iter().sum();
                    self.canvas.rect(starts[column], cursor - row_height, span_width, row_height, Some(colour), None);
                }
            }
            cursor -= row_height;
        }
        self.canvas.restore();
        // Rules.
        let mut cursor = top;
        for (position, row) in rows.iter().enumerate() {
            let row_height = layout.heights[*row];
            let is_last = position + 1 == rows.len();
            for (column, cell) in layout.cells[*row].iter().enumerate() {
                if cell.hidden {
                    continue;
                }
                let span_width: f64 = layout.columns[column..(column + cell.span).min(layout.columns.len())].iter().sum();
                if let Some((line, colour)) = cell.line_below {
                    if !is_last || table.outline.is_none() {
                        self.canvas.line(starts[column], cursor - row_height, starts[column] + span_width, cursor - row_height, colour, line, None);
                    }
                }
                if let Some((line, colour)) = cell.line_before {
                    self.canvas.line(starts[column], cursor, starts[column], cursor - row_height, colour, line, None);
                }
            }
            cursor -= row_height;
        }
        for (c0, r0, c1, r1, line, colour) in &layout.boxes {
            // The part of the range drawn here.
            let mut box_top = None;
            let mut box_bottom = top;
            let mut cursor = top;
            for row in &rows {
                let row_height = layout.heights[*row];
                if row >= r0 && row <= r1 {
                    box_top.get_or_insert(cursor);
                    box_bottom = cursor - row_height;
                }
                cursor -= row_height;
            }
            if let Some(box_top) = box_top {
                let box_left = starts[*c0];
                let box_width: f64 = layout.columns[*c0..=*c1].iter().sum();
                self.canvas.rect(box_left, box_bottom, box_width, box_top - box_bottom, None, Some((*colour, *line)));
            }
        }
        if let Some((line, colour)) = table.outline {
            self.canvas.round_rect(left, top - height, total, height, table.radius, None, Some((colour, line)));
        }
        // Contents.
        let mut cursor = top;
        for row in &rows {
            let row_height = layout.heights[*row];
            for (column, cell) in layout.cells[*row].iter().enumerate() {
                if cell.hidden {
                    continue;
                }
                let span_width: f64 = layout.columns[column..(column + cell.span).min(layout.columns.len())].iter().sum();
                let inner = span_width - cell.pad[0] - cell.pad[1];
                let mut y = match cell.valign {
                    VAlign::Top => cursor - cell.pad[2],
                    VAlign::Middle => cursor - cell.pad[2] - ((row_height - cell.pad[2] - cell.pad[3]) - cell.height) / 2.0,
                };
                for (laid, height) in &cell.content {
                    self.draw_laid(laid, starts[column] + cell.pad[0], y, inner, cell.align);
                    y -= height;
                }
            }
            cursor -= row_height;
        }
    }

    fn clip_round(&mut self, x: f64, y: f64, width: f64, height: f64, radius: f64) {
        let k = radius * 0.552_284_75;
        let n = super::doc::number;
        let (right, top) = (x + width, y + height);
        self.canvas.ops.push_str(&format!(
            "{} {} m {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c h W n\n",
            n(x + radius), n(y), n(right - radius), n(y),
            n(right - radius + k), n(y), n(right), n(y + radius - k), n(right), n(y + radius),
            n(right), n(top - radius),
            n(right), n(top - radius + k), n(right - radius + k), n(top), n(right - radius), n(top),
            n(x + radius), n(top),
            n(x + radius - k), n(top), n(x), n(top - radius + k), n(x), n(top - radius),
            n(x), n(y + radius),
            n(x), n(y + radius - k), n(x + radius - k), n(y), n(x + radius), n(y)
        ));
    }

    // ---- Flowing ----

    /// Place a list of flowables, breaking pages as needed.
    pub fn flow(&mut self, story: Vec<Flow>) {
        for flow in story {
            self.place(flow);
        }
    }

    fn place(&mut self, flow: Flow) {
        let width = self.width();
        let x = self.margin;
        match flow {
            Flow::PageBreak => {
                if !self.at_top() {
                    self.new_page();
                }
            }
            Flow::CondPageBreak(height) => {
                if self.available() < height && !self.at_top() {
                    self.new_page();
                }
            }
            Flow::Spacer(height) => {
                if !self.at_top() {
                    self.y -= height.min(self.available());
                }
            }
            Flow::KeepTogether(children) => {
                let (_, height) = self.lay(&Flow::KeepTogether(children.clone()), width);
                if height > self.available() && height <= self.frame_height() && !self.at_top() {
                    self.new_page();
                }
                if height <= self.available() {
                    let (laid, height) = self.lay(&Flow::KeepTogether(children), width);
                    self.draw_laid(&laid, x, self.y, width, Align::Left);
                    self.y -= height;
                } else {
                    for child in children {
                        self.place(child);
                    }
                }
            }
            Flow::Para(paragraph) => {
                let before = if self.at_top() { 0.0 } else { paragraph.style.space_before };
                self.y -= before.min(self.available());
                let lines = self.wrap_paragraph(&paragraph, width);
                let mut rest: &[Line] = &lines;
                while !rest.is_empty() {
                    let fit = ((self.available() + 0.01) / paragraph.style.leading).floor() as usize;
                    if fit == 0 {
                        if self.at_top() {
                            // A line taller than the page: draw it anyway.
                            self.draw_lines(&rest[..1], &paragraph.style, x, self.y, width);
                            rest = &rest[1..];
                            self.new_page();
                            continue;
                        }
                        self.new_page();
                        continue;
                    }
                    let count = fit.min(rest.len());
                    self.draw_lines(&rest[..count], &paragraph.style, x, self.y, width);
                    self.y -= count as f64 * paragraph.style.leading;
                    rest = &rest[count..];
                    if !rest.is_empty() {
                        self.new_page();
                    }
                }
                self.y -= paragraph.style.space_after.min(self.available());
            }
            Flow::Table(table) => {
                let layout = self.layout_table(&table, width);
                let header = table.repeat_rows.min(layout.heights.len());
                let header_height: f64 = layout.heights[..header].iter().sum();
                let mut start = 0usize;
                let rows = layout.heights.len();
                while start < rows {
                    let repeat = if start > 0 && header > 0 { Some(header) } else { None };
                    let mut room = self.available() - if repeat.is_some() { header_height } else { 0.0 };
                    let mut end = start;
                    while end < rows && layout.heights[end] <= room + 0.01 {
                        room -= layout.heights[end];
                        end += 1;
                    }
                    // Keep at least one row after the header with it.
                    if start == 0 && header > 0 && end <= header && end < rows {
                        end = 0;
                    }
                    if end == start {
                        if self.at_top() {
                            end = start + 1;
                        } else {
                            self.new_page();
                            continue;
                        }
                    }
                    let height: f64 = layout.heights[start..end].iter().sum::<f64>() + if repeat.is_some() { header_height } else { 0.0 };
                    self.draw_table(&table, &layout, x, self.y, width, start, end, repeat);
                    self.y -= height;
                    start = end;
                    if start < rows {
                        self.new_page();
                    }
                }
            }
            other => {
                let (laid, height) = self.lay(&other, width);
                if height > self.available() && !self.at_top() {
                    self.new_page();
                }
                self.draw_laid(&laid, x, self.y, width, Align::Left);
                self.y -= height;
            }
        }
    }
}
