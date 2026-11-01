//! Excel workbooks (Office Open XML SpreadsheetML): cells with fonts, fills, borders, and
//! alignment; merged cells; column widths and row heights; pictures; and charts that read the
//! cells beside them.

use std::collections::BTreeMap;

use crate::zip::ZipWriter;

pub fn column_letter(column: u32) -> String {
    let mut column = column;
    let mut out = Vec::new();
    while column > 0 {
        let rest = (column - 1) % 26;
        out.push(b'A' + rest as u8);
        column = (column - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

pub fn cell_ref(row: u32, column: u32) -> String {
    format!("{}{row}", column_letter(column))
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(ch),
            // Characters XML 1.0 cannot carry.
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => {}
            c => out.push(c),
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
#[derive(Default)]
pub enum CellValue {
    #[default]
    Empty,
    Text(String),
    Number(f64),
}

/// One side of a cell's border: line style ("thin", "medium", "thick") and colour.
pub type Side = Option<(&'static str, String)>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Border {
    pub left: Option<(&'static str, String)>,
    pub right: Option<(&'static str, String)>,
    pub top: Option<(&'static str, String)>,
    pub bottom: Option<(&'static str, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Font {
    /// Size in hundredths of a point.
    pub size: u32,
    pub bold: bool,
    pub colour: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Alignment {
    pub horizontal: &'static str,
    pub vertical: &'static str,
    pub wrap: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Cell {
    pub value: CellValue,
    pub font: Option<Font>,
    pub fill: Option<String>,
    pub border: Border,
    pub alignment: Option<Alignment>,
}


/// A cell range on a sheet, for chart data: rows and columns are 1-based and inclusive.
#[derive(Debug, Clone)]
pub struct Range {
    pub first_row: u32,
    pub last_row: u32,
    pub first_column: u32,
    pub last_column: u32,
}

impl Range {
    pub fn column(column: u32, first_row: u32, last_row: u32) -> Range {
        Range { first_row, last_row, first_column: column, last_column: column }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChartKind {
    /// Horizontal bars.
    Bar,
    /// Vertical columns.
    Column,
    Doughnut,
    Line,
}

#[derive(Debug, Clone)]
pub struct Series {
    /// The cell holding the series' name (its header), if any.
    pub title: Option<(u32, u32)>,
    pub values: Range,
    pub categories: Option<Range>,
    pub fill: Option<String>,
    pub line: Option<String>,
    pub line_width: Option<u32>,
    pub dashed: bool,
    pub marker: bool,
    pub points: Vec<(usize, String)>,
    /// A white outline between doughnut slices.
    pub point_outline: bool,
    pub labels: bool,
}

impl Series {
    pub fn new(values: Range) -> Series {
        Series { title: None, values, categories: None, fill: None, line: None, line_width: None, dashed: false, marker: false, points: Vec::new(), point_outline: false, labels: false }
    }
}

#[derive(Debug, Clone)]
pub struct Chart {
    pub kind: ChartKind,
    pub title: Option<String>,
    pub series: Vec<Series>,
    pub stacked: bool,
    pub overlap: Option<i32>,
    pub hole: u32,
    pub legend: Option<&'static str>,
    pub value_min: Option<f64>,
    pub value_max: Option<f64>,
    /// Categories from the top down (axis orientation maxMin).
    pub reverse_categories: bool,
    pub value_title: Option<String>,
    /// A line chart drawn over this one against its own axis on the right (e.g. scores over counts).
    pub overlay: Option<Box<Chart>>,
    /// Where it sits: top-left cell (1-based) and size in centimetres.
    pub anchor: (u32, u32),
    pub size_cm: (f64, f64),
}

impl Chart {
    pub fn new(kind: ChartKind) -> Chart {
        Chart {
            kind,
            title: None,
            series: Vec::new(),
            stacked: false,
            overlap: None,
            hole: 50,
            legend: Some("r"),
            value_min: None,
            value_max: None,
            reverse_categories: false,
            value_title: None,
            overlay: None,
            anchor: (1, 1),
            size_cm: (15.0, 7.5),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Picture {
    /// Top-left cell, 1-based (row, column).
    pub anchor: (u32, u32),
    pub width_px: u32,
    pub height_px: u32,
    pub data: Vec<u8>,
    /// "jpeg" or "png".
    pub extension: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct Sheet {
    pub title: String,
    cells: BTreeMap<(u32, u32), Cell>,
    merges: Vec<(u32, u32, u32)>,
    pub column_widths: BTreeMap<u32, f64>,
    pub row_heights: BTreeMap<u32, f64>,
    pub show_grid: bool,
    pub freeze: Option<(u32, u32)>,
    pub auto_filter: Option<String>,
    pub fit_to_width: bool,
    pub margins: Option<f64>,
    pub pictures: Vec<Picture>,
    pub charts: Vec<Chart>,
}

impl Sheet {
    pub fn new(title: &str) -> Sheet {
        Sheet { title: title.to_string(), show_grid: true, ..Sheet::default() }
    }

    pub fn cell(&mut self, row: u32, column: u32) -> &mut Cell {
        self.cells.entry((row, column)).or_default()
    }

    pub fn get(&self, row: u32, column: u32) -> Option<&Cell> {
        self.cells.get(&(row, column))
    }

    pub fn merge(&mut self, row: u32, first: u32, last: u32) {
        if last > first {
            self.merges.push((row, first, last));
        }
    }

    pub fn max_row(&self) -> u32 {
        self.cells.keys().map(|(row, _)| *row).max().unwrap_or(1)
    }

    pub fn max_column(&self) -> u32 {
        self.cells.keys().map(|(_, column)| *column).max().unwrap_or(1)
    }

    /// The cells' values by row, for column widths and chart caches.
    fn text(&self, row: u32, column: u32) -> Option<String> {
        match &self.cells.get(&(row, column))?.value {
            CellValue::Text(text) => Some(text.clone()),
            CellValue::Number(number) => Some(number_text(*number)),
            CellValue::Empty => None,
        }
    }

    fn number(&self, row: u32, column: u32) -> Option<f64> {
        match &self.cells.get(&(row, column))?.value {
            CellValue::Number(number) => Some(*number),
            CellValue::Text(text) => text.trim().parse().ok(),
            CellValue::Empty => None,
        }
    }
}

fn number_text(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

#[derive(Default)]
struct Styles {
    fonts: Vec<Font>,
    fills: Vec<String>,
    borders: Vec<Border>,
    formats: Vec<(usize, usize, usize, Option<Alignment>)>,
}

impl Styles {
    fn index<T: PartialEq + Clone>(list: &mut Vec<T>, item: &T) -> usize {
        match list.iter().position(|known| known == item) {
            Some(index) => index,
            None => {
                list.push(item.clone());
                list.len() - 1
            }
        }
    }

    fn format_of(&mut self, cell: &Cell, default_font: &Font) -> usize {
        let font = Self::index(&mut self.fonts, cell.font.as_ref().unwrap_or(default_font));
        // Fills 0 and 1 are the reserved "none" and "gray125" patterns.
        let fill = match &cell.fill {
            Some(colour) => 2 + Self::index(&mut self.fills, colour),
            None => 0,
        };
        let border = Self::index(&mut self.borders, &cell.border);
        let key = (font, fill, border, cell.alignment.clone());
        Self::index(&mut self.formats, &key)
    }

    fn xml(&self, font_name: &str) -> String {
        let mut out = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#);
        out.push_str(&format!("<fonts count=\"{}\">", self.fonts.len()));
        for font in &self.fonts {
            out.push_str(&format!(
                "<font>{}<sz val=\"{}\"/><color rgb=\"FF{}\"/><name val=\"{}\"/><family val=\"2\"/></font>",
                if font.bold { "<b/>" } else { "" },
                font.size as f64 / 100.0,
                font.colour,
                escape(font_name)
            ));
        }
        out.push_str("</fonts>");
        out.push_str(&format!("<fills count=\"{}\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill>", self.fills.len() + 2));
        for colour in &self.fills {
            out.push_str(&format!("<fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF{colour}\"/><bgColor rgb=\"FF{colour}\"/></patternFill></fill>"));
        }
        out.push_str("</fills>");
        out.push_str(&format!("<borders count=\"{}\">", self.borders.len()));
        for border in &self.borders {
            out.push_str("<border>");
            for (name, side) in [("left", &border.left), ("right", &border.right), ("top", &border.top), ("bottom", &border.bottom)] {
                match side {
                    Some((style, colour)) => out.push_str(&format!("<{name} style=\"{style}\"><color rgb=\"FF{colour}\"/></{name}>")),
                    None => out.push_str(&format!("<{name}/>")),
                }
            }
            out.push_str("<diagonal/></border>");
        }
        out.push_str("</borders>");
        out.push_str(r#"<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>"#);
        out.push_str(&format!("<cellXfs count=\"{}\">", self.formats.len()));
        for (font, fill, border, alignment) in &self.formats {
            out.push_str(&format!(
                "<xf numFmtId=\"0\" fontId=\"{font}\" fillId=\"{fill}\" borderId=\"{border}\" xfId=\"0\" applyFont=\"1\"{}{}{}>",
                if *fill > 0 { " applyFill=\"1\"" } else { "" },
                if *border > 0 { " applyBorder=\"1\"" } else { "" },
                if alignment.is_some() { " applyAlignment=\"1\"" } else { "" }
            ));
            if let Some(alignment) = alignment {
                out.push_str(&format!("<alignment horizontal=\"{}\" vertical=\"{}\"{}/>", alignment.horizontal, alignment.vertical, if alignment.wrap { " wrapText=\"1\"" } else { "" }));
            }
            out.push_str("</xf>");
        }
        out.push_str(r#"</cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#);
        out
    }
}

fn sheet_ref(title: &str) -> String {
    format!("'{}'", title.replace('\'', "''"))
}

fn range_ref(title: &str, range: &Range) -> String {
    format!(
        "{}!${}${}:${}${}",
        sheet_ref(title),
        column_letter(range.first_column),
        range.first_row,
        column_letter(range.last_column),
        range.last_row
    )
}

fn colour_fill(colour: &Option<String>) -> String {
    colour.as_ref().map(|colour| format!("<a:solidFill><a:srgbClr val=\"{colour}\"/></a:solidFill>")).unwrap_or_default()
}

/// The chart XML of one chart (DrawingML chart part).
fn chart_xml(chart: &Chart, sheet: &Sheet) -> String {
    let mut out = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><c:roundedCorners val="0"/><c:chart>"#);
    if let Some(title) = &chart.title {
        out.push_str(&format!(
            "<c:title><c:tx><c:rich><a:bodyPr/><a:p><a:pPr><a:defRPr sz=\"1100\" b=\"1\"/></a:pPr><a:r><a:rPr lang=\"en-US\" sz=\"1100\" b=\"1\"/><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>",
            escape(title)
        ));
    }
    out.push_str("<c:autoTitleDeleted val=\"");
    out.push_str(if chart.title.is_some() { "0" } else { "1" });
    out.push_str("\"/><c:plotArea><c:layout/>");
    out.push_str(&plot_xml(chart, sheet, 10, 100));
    if let Some(overlay) = &chart.overlay {
        out.push_str(&plot_xml(overlay, sheet, 30, 200));
    }
    if chart.kind != ChartKind::Doughnut {
        out.push_str(&axes_xml(chart, 10, 100, false));
        if let Some(overlay) = &chart.overlay {
            out.push_str(&axes_xml(overlay, 30, 200, true));
        }
    }
    out.push_str("</c:plotArea>");
    if let Some(position) = chart.legend {
        out.push_str(&format!("<c:legend><c:legendPos val=\"{position}\"/><c:overlay val=\"0\"/></c:legend>"));
    }
    out.push_str(r#"<c:plotVisOnly val="1"/><c:dispBlanksAs val="gap"/></c:chart><c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr></c:chartSpace>"#);
    out
}

fn cache_strings(sheet: &Sheet, range: &Range) -> String {
    let mut points = String::new();
    let mut count = 0;
    for row in range.first_row..=range.last_row {
        for column in range.first_column..=range.last_column {
            if let Some(text) = sheet.text(row, column) {
                points.push_str(&format!("<c:pt idx=\"{count}\"><c:v>{}</c:v></c:pt>", escape(&text)));
            }
            count += 1;
        }
    }
    format!("<c:strCache><c:ptCount val=\"{count}\"/>{points}</c:strCache>")
}

fn cache_numbers(sheet: &Sheet, range: &Range) -> String {
    let mut points = String::new();
    let mut count = 0;
    for row in range.first_row..=range.last_row {
        for column in range.first_column..=range.last_column {
            if let Some(number) = sheet.number(row, column) {
                points.push_str(&format!("<c:pt idx=\"{count}\"><c:v>{}</c:v></c:pt>", number_text(number)));
            }
            count += 1;
        }
    }
    format!("<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{count}\"/>{points}</c:numCache>")
}

fn plot_xml(chart: &Chart, sheet: &Sheet, axis_base: u32, _id_base: u32) -> String {
    let mut out = String::new();
    let (open, close) = match chart.kind {
        ChartKind::Bar | ChartKind::Column => ("<c:barChart>", "</c:barChart>"),
        ChartKind::Doughnut => ("<c:doughnutChart>", "</c:doughnutChart>"),
        ChartKind::Line => ("<c:lineChart>", "</c:lineChart>"),
    };
    out.push_str(open);
    match chart.kind {
        ChartKind::Bar | ChartKind::Column => {
            out.push_str(&format!(
                "<c:barDir val=\"{}\"/><c:grouping val=\"{}\"/><c:varyColors val=\"0\"/>",
                if chart.kind == ChartKind::Bar { "bar" } else { "col" },
                if chart.stacked { "stacked" } else { "clustered" }
            ));
        }
        ChartKind::Doughnut => out.push_str("<c:varyColors val=\"1\"/>"),
        ChartKind::Line => out.push_str("<c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>"),
    }
    for (index, series) in chart.series.iter().enumerate() {
        let order = index + axis_base as usize;
        out.push_str(&format!("<c:ser><c:idx val=\"{order}\"/><c:order val=\"{order}\"/>"));
        if let Some((row, column)) = series.title {
            out.push_str(&format!(
                "<c:tx><c:strRef><c:f>{}!${}${}</c:f>{}</c:strRef></c:tx>",
                sheet_ref(&sheet.title),
                column_letter(column),
                row,
                cache_strings(sheet, &Range::column(column, row, row))
            ));
        }
        let mut shape = String::new();
        if chart.kind == ChartKind::Line {
            let colour = series.line.clone().or(series.fill.clone()).unwrap_or_else(|| "17222B".into());
            shape.push_str(&format!(
                "<c:spPr><a:ln w=\"{}\" cap=\"rnd\"><a:solidFill><a:srgbClr val=\"{colour}\"/></a:solidFill>{}</a:ln></c:spPr>",
                series.line_width.unwrap_or(22_000),
                if series.dashed { "<a:prstDash val=\"dash\"/>" } else { "" }
            ));
            if series.marker {
                shape.push_str(&format!(
                    "<c:marker><c:symbol val=\"circle\"/><c:size val=\"7\"/><c:spPr><a:solidFill><a:srgbClr val=\"{colour}\"/></a:solidFill><a:ln><a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill></a:ln></c:spPr></c:marker>"
                ));
            } else {
                shape.push_str("<c:marker><c:symbol val=\"none\"/></c:marker>");
            }
        } else if series.fill.is_some() {
            shape.push_str(&format!("<c:spPr>{}<a:ln><a:noFill/></a:ln></c:spPr>", colour_fill(&series.fill)));
        } else if chart.kind != ChartKind::Doughnut {
            shape.push_str("<c:spPr><a:ln><a:noFill/></a:ln></c:spPr>");
        }
        out.push_str(&shape);
        if chart.kind == ChartKind::Bar || chart.kind == ChartKind::Column {
            out.push_str("<c:invertIfNegative val=\"0\"/>");
        }
        for (point, colour) in &series.points {
            out.push_str(&format!(
                "<c:dPt><c:idx val=\"{point}\"/>{}<c:spPr><a:solidFill><a:srgbClr val=\"{colour}\"/></a:solidFill>{}</c:spPr></c:dPt>",
                if chart.kind == ChartKind::Bar || chart.kind == ChartKind::Column { "<c:invertIfNegative val=\"0\"/><c:bubble3D val=\"0\"/>" } else { "<c:bubble3D val=\"0\"/>" },
                if series.point_outline { "<a:ln><a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill></a:ln>" } else { "" }
            ));
        }
        if series.labels {
            out.push_str("<c:dLbls><c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr><c:showLegendKey val=\"0\"/><c:showVal val=\"1\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbls>");
        }
        if let Some(categories) = &series.categories {
            out.push_str(&format!("<c:cat><c:strRef><c:f>{}</c:f>{}</c:strRef></c:cat>", range_ref(&sheet.title, categories), cache_strings(sheet, categories)));
        }
        out.push_str(&format!("<c:val><c:numRef><c:f>{}</c:f>{}</c:numRef></c:val>", range_ref(&sheet.title, &series.values), cache_numbers(sheet, &series.values)));
        if chart.kind == ChartKind::Line {
            out.push_str("<c:smooth val=\"0\"/>");
        }
        out.push_str("</c:ser>");
    }
    match chart.kind {
        ChartKind::Bar | ChartKind::Column => {
            out.push_str("<c:gapWidth val=\"150\"/>");
            if let Some(overlap) = chart.overlap.or(if chart.stacked { Some(100) } else { None }) {
                out.push_str(&format!("<c:overlap val=\"{overlap}\"/>"));
            }
            out.push_str(&format!("<c:axId val=\"{axis_base}\"/><c:axId val=\"{}\"/>", axis_base + 90));
        }
        ChartKind::Line => {
            out.push_str("<c:marker val=\"1\"/>");
            out.push_str(&format!("<c:axId val=\"{axis_base}\"/><c:axId val=\"{}\"/>", axis_base + 90));
        }
        ChartKind::Doughnut => out.push_str(&format!("<c:firstSliceAng val=\"0\"/><c:holeSize val=\"{}\"/>", chart.hole)),
    }
    out.push_str(close);
    out
}

fn axes_xml(chart: &Chart, category_id: u32, _id_base: u32, secondary: bool) -> String {
    let value_id = category_id + 90;
    let horizontal = chart.kind == ChartKind::Bar;
    let (category_position, value_position) = if horizontal { ("l", "b") } else { ("b", if secondary { "r" } else { "l" }) };
    let mut scaling = String::from("<c:scaling>");
    scaling.push_str(&format!("<c:orientation val=\"{}\"/>", if chart.reverse_categories { "maxMin" } else { "minMax" }));
    scaling.push_str("</c:scaling>");
    let mut value_scaling = String::from("<c:scaling><c:orientation val=\"minMax\"/>");
    if let Some(max) = chart.value_max {
        value_scaling.push_str(&format!("<c:max val=\"{max}\"/>"));
    }
    if let Some(min) = chart.value_min {
        value_scaling.push_str(&format!("<c:min val=\"{min}\"/>"));
    }
    value_scaling.push_str("</c:scaling>");
    let text = "<c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"800\"><a:solidFill><a:srgbClr val=\"5F6C66\"/></a:solidFill></a:defRPr></a:pPr><a:endParaRPr lang=\"en-US\"/></a:p></c:txPr>";
    let line = "<c:spPr><a:ln w=\"6350\"><a:solidFill><a:srgbClr val=\"D3D9D5\"/></a:solidFill></a:ln></c:spPr>";
    let title = chart.value_title.as_ref().map(|title| format!(
        "<c:title><c:tx><c:rich><a:bodyPr rot=\"-5400000\" vert=\"horz\"/><a:p><a:pPr><a:defRPr sz=\"800\"/></a:pPr><a:r><a:rPr lang=\"en-US\" sz=\"800\"/><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>",
        escape(title)
    )).unwrap_or_default();
    let mut out = String::new();
    out.push_str(&format!(
        "<c:catAx><c:axId val=\"{category_id}\"/>{scaling}<c:delete val=\"{}\"/><c:axPos val=\"{category_position}\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{line}{text}<c:crossAx val=\"{value_id}\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>",
        if secondary { 1 } else { 0 }
    ));
    out.push_str(&format!(
        "<c:valAx><c:axId val=\"{value_id}\"/>{value_scaling}<c:delete val=\"0\"/><c:axPos val=\"{value_position}\"/>{title}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{line}{text}<c:crossAx val=\"{category_id}\"/><c:crosses val=\"{}\"/><c:crossBetween val=\"between\"/></c:valAx>",
        if secondary { "max" } else { "autoZero" }
    ));
    out
}

/// EMUs per pixel at 96 dpi, and per centimetre.
const EMU_PER_PIXEL: f64 = 9525.0;
const EMU_PER_CM: f64 = 360_000.0;

pub struct Workbook {
    pub sheets: Vec<Sheet>,
    pub font_name: String,
    pub default_font: Font,
}

impl Workbook {
    pub fn new(font_name: &str) -> Workbook {
        Workbook { sheets: Vec::new(), font_name: font_name.into(), default_font: Font { size: 1100, bold: false, colour: "000000".into() } }
    }

    pub fn save(&self) -> Vec<u8> {
        let mut styles = Styles::default();
        styles.fonts.push(self.default_font.clone());
        styles.borders.push(Border::default());
        styles.formats.push((0, 0, 0, None));
        let mut zip = ZipWriter::new();
        let mut content_types = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="jpeg" ContentType="image/jpeg"/><Default Extension="png" ContentType="image/png"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>"#);
        let mut workbook_sheets = String::new();
        let mut workbook_rels = String::new();
        let mut defined_names = String::new();
        let (mut chart_number, mut image_number) = (0usize, 0usize);
        for (index, sheet) in self.sheets.iter().enumerate() {
            let number = index + 1;
            content_types.push_str(&format!("<Override PartName=\"/xl/worksheets/sheet{number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"));
            workbook_sheets.push_str(&format!("<sheet name=\"{}\" sheetId=\"{number}\" r:id=\"rId{number}\"/>", escape(&sheet.title)));
            workbook_rels.push_str(&format!("<Relationship Id=\"rId{number}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{number}.xml\"/>"));
            if let Some(filter) = &sheet.auto_filter {
                let absolute: String = filter.split(':').map(|part| {
                    let split = part.find(|c: char| c.is_ascii_digit()).unwrap_or(part.len());
                    format!("${}${}", &part[..split], &part[split..])
                }).collect::<Vec<_>>().join(":");
                defined_names.push_str(&format!("<definedName name=\"_xlnm._FilterDatabase\" localSheetId=\"{index}\" hidden=\"1\">{}!{absolute}</definedName>", sheet_ref(&sheet.title)));
            }
            let has_drawing = !sheet.pictures.is_empty() || !sheet.charts.is_empty();
            zip.add(&format!("xl/worksheets/sheet{number}.xml"), self.sheet_xml(sheet, &mut styles, has_drawing).as_bytes());
            if has_drawing {
                content_types.push_str(&format!("<Override PartName=\"/xl/drawings/drawing{number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/>"));
                zip.add(
                    &format!("xl/worksheets/_rels/sheet{number}.xml.rels"),
                    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing{number}.xml"/></Relationships>"#).as_bytes(),
                );
                let mut drawing = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart">"#);
                let mut rels = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
                let mut relation = 0;
                for picture in &sheet.pictures {
                    relation += 1;
                    image_number += 1;
                    let name = format!("image{image_number}.{}", picture.extension);
                    zip.add(&format!("xl/media/{name}"), &picture.data);
                    rels.push_str(&format!("<Relationship Id=\"rId{relation}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/{name}\"/>"));
                    let (cx, cy) = ((picture.width_px as f64 * EMU_PER_PIXEL) as u64, (picture.height_px as f64 * EMU_PER_PIXEL) as u64);
                    drawing.push_str(&format!(
                        "<xdr:oneCellAnchor><xdr:from><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx=\"{cx}\" cy=\"{cy}\"/><xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{}\" name=\"Picture {relation}\"/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"rId{relation}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:oneCellAnchor>",
                        picture.anchor.1 - 1,
                        picture.anchor.0 - 1,
                        relation + 1
                    ));
                }
                for chart in &sheet.charts {
                    relation += 1;
                    chart_number += 1;
                    content_types.push_str(&format!("<Override PartName=\"/xl/charts/chart{chart_number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>"));
                    zip.add(&format!("xl/charts/chart{chart_number}.xml"), chart_xml(chart, sheet).as_bytes());
                    rels.push_str(&format!("<Relationship Id=\"rId{relation}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" Target=\"../charts/chart{chart_number}.xml\"/>"));
                    let (cx, cy) = ((chart.size_cm.0 * EMU_PER_CM) as u64, (chart.size_cm.1 * EMU_PER_CM) as u64);
                    drawing.push_str(&format!(
                        "<xdr:oneCellAnchor><xdr:from><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx=\"{cx}\" cy=\"{cy}\"/><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{}\" name=\"Chart {relation}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart r:id=\"rId{relation}\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:oneCellAnchor>",
                        chart.anchor.1 - 1,
                        chart.anchor.0 - 1,
                        relation + 1
                    ));
                }
                drawing.push_str("</xdr:wsDr>");
                rels.push_str("</Relationships>");
                zip.add(&format!("xl/drawings/drawing{number}.xml"), drawing.as_bytes());
                zip.add(&format!("xl/drawings/_rels/drawing{number}.xml.rels"), rels.as_bytes());
            }
        }
        content_types.push_str("</Types>");
        zip.add("[Content_Types].xml", content_types.as_bytes());
        zip.add("_rels/.rels", br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#);
        let created = crate::clock::utc_now();
        zip.add(
            "docProps/core.xml",
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:creator>Audit App</dc:creator><dcterms:created xsi:type="dcterms:W3CDTF">{}T{:02}:{:02}:{:02}Z</dcterms:created></cp:coreProperties>"#,
                created.date.iso(),
                created.hour,
                created.minute,
                created.second
            )
            .as_bytes(),
        );
        zip.add("docProps/app.xml", br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Audit App</Application></Properties>"#);
        zip.add(
            "xl/workbook.xml",
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><bookViews><workbookView activeTab="0"/></bookViews><sheets>{workbook_sheets}</sheets>{}</workbook>"#,
                if defined_names.is_empty() { String::new() } else { format!("<definedNames>{defined_names}</definedNames>") }
            )
            .as_bytes(),
        );
        let count = self.sheets.len();
        workbook_rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>", count + 1));
        zip.add(
            "xl/_rels/workbook.xml.rels",
            format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{workbook_rels}</Relationships>"#).as_bytes(),
        );
        zip.add("xl/styles.xml", styles.xml(&self.font_name).as_bytes());
        zip.finish()
    }

    fn sheet_xml(&self, sheet: &Sheet, styles: &mut Styles, has_drawing: bool) -> String {
        let mut out = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#);
        if sheet.fit_to_width {
            out.push_str("<sheetPr><pageSetUpPr fitToPage=\"1\"/></sheetPr>");
        }
        out.push_str(&format!("<dimension ref=\"A1:{}\"/>", cell_ref(sheet.max_row(), sheet.max_column())));
        out.push_str(&format!("<sheetViews><sheetView workbookViewId=\"0\"{}>", if sheet.show_grid { "" } else { " showGridLines=\"0\"" }));
        if let Some((row, column)) = sheet.freeze {
            let mut pane = String::from("<pane");
            if column > 1 {
                pane.push_str(&format!(" xSplit=\"{}\"", column - 1));
            }
            if row > 1 {
                pane.push_str(&format!(" ySplit=\"{}\"", row - 1));
            }
            pane.push_str(&format!(" topLeftCell=\"{}\" activePane=\"{}\" state=\"frozen\"/>", cell_ref(row, column), if column > 1 { "bottomRight" } else { "bottomLeft" }));
            out.push_str(&pane);
        }
        out.push_str("</sheetView></sheetViews><sheetFormatPr defaultRowHeight=\"15\"/>");
        if !sheet.column_widths.is_empty() {
            out.push_str("<cols>");
            for (column, width) in &sheet.column_widths {
                out.push_str(&format!("<col min=\"{column}\" max=\"{column}\" width=\"{width}\" customWidth=\"1\"/>"));
            }
            out.push_str("</cols>");
        }
        out.push_str("<sheetData>");
        let mut rows: BTreeMap<u32, Vec<(u32, &Cell)>> = BTreeMap::new();
        for ((row, column), cell) in &sheet.cells {
            rows.entry(*row).or_default().push((*column, cell));
        }
        for row in sheet.row_heights.keys() {
            rows.entry(*row).or_default();
        }
        for (row, cells) in rows {
            match sheet.row_heights.get(&row) {
                Some(height) => out.push_str(&format!("<row r=\"{row}\" ht=\"{height}\" customHeight=\"1\">")),
                None => out.push_str(&format!("<row r=\"{row}\">")),
            }
            for (column, cell) in cells {
                let style = styles.format_of(cell, &self.default_font);
                let reference = cell_ref(row, column);
                let style_attribute = if style > 0 { format!(" s=\"{style}\"") } else { String::new() };
                match &cell.value {
                    CellValue::Empty => out.push_str(&format!("<c r=\"{reference}\"{style_attribute}/>")),
                    CellValue::Number(number) if number.is_finite() => out.push_str(&format!("<c r=\"{reference}\"{style_attribute}><v>{}</v></c>", number_text(*number))),
                    CellValue::Number(_) => out.push_str(&format!("<c r=\"{reference}\"{style_attribute}/>")),
                    // Inline text: never read as a formula, whatever it starts with.
                    CellValue::Text(text) => out.push_str(&format!("<c r=\"{reference}\"{style_attribute} t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>", escape(text))),
                }
            }
            out.push_str("</row>");
        }
        out.push_str("</sheetData>");
        if let Some(filter) = &sheet.auto_filter {
            out.push_str(&format!("<autoFilter ref=\"{filter}\"/>"));
        }
        if !sheet.merges.is_empty() {
            out.push_str(&format!("<mergeCells count=\"{}\">", sheet.merges.len()));
            for (row, first, last) in &sheet.merges {
                out.push_str(&format!("<mergeCell ref=\"{}:{}\"/>", cell_ref(*row, *first), cell_ref(*row, *last)));
            }
            out.push_str("</mergeCells>");
        }
        let margin = sheet.margins.unwrap_or(0.75);
        out.push_str(&format!("<pageMargins left=\"{margin}\" right=\"{margin}\" top=\"1\" bottom=\"1\" header=\"0.5\" footer=\"0.5\"/>"));
        out.push_str(&format!("<pageSetup paperSize=\"9\"{}/>", if sheet.fit_to_width { " fitToWidth=\"1\" fitToHeight=\"0\"" } else { "" }));
        if has_drawing {
            out.push_str("<drawing r:id=\"rId1\"/>");
        }
        out.push_str("</worksheet>");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_archive() {
        assert_eq!(column_letter(1), "A");
        assert_eq!(column_letter(26), "Z");
        assert_eq!(column_letter(27), "AA");
        let mut book = Workbook::new("Noto Sans");
        let mut sheet = Sheet::new("O'Brien");
        sheet.cell(1, 1).value = CellValue::Text("=1+1 & <x>".into());
        sheet.cell(2, 2).value = CellValue::Number(3.5);
        sheet.merge(1, 1, 3);
        let mut chart = Chart::new(ChartKind::Bar);
        let mut series = Series::new(Range::column(2, 2, 2));
        series.categories = Some(Range::column(1, 2, 2));
        chart.series.push(series);
        sheet.charts.push(chart);
        book.sheets.push(sheet);
        let files = crate::zip::read_archive(&book.save(), 1 << 22).unwrap();
        let sheet_xml = files.iter().find(|(name, _)| name == "xl/worksheets/sheet1.xml").unwrap();
        let text = String::from_utf8(sheet_xml.1.clone()).unwrap();
        assert!(text.contains("=1+1 &amp; &lt;x&gt;"));
        assert!(text.contains("<mergeCell ref=\"A1:C1\"/>"));
        let chart = files.iter().find(|(name, _)| name == "xl/charts/chart1.xml").unwrap();
        assert!(String::from_utf8(chart.1.clone()).unwrap().contains("'O''Brien'!$B$2:$B$2"));
    }
}
