//! Markdown parsing into a renderer-friendly document model.
//!
//! The model keeps source byte ranges for every block and every appended text fragment so
//! highlights and bookmarks can be anchored back to the original `.md` file. Markdown itself
//! is never rewritten or converted into a proprietary format.

use std::collections::BTreeMap;
use std::ops::Range;

use pulldown_cmark::{
    Alignment as CmAlignment, BlockQuoteKind as CmQuoteKind, CodeBlockKind, Event, HeadingLevel,
    MetadataBlockKind, Options, Parser, Tag, TagEnd,
};
use serde::{Deserialize, Serialize};

use crate::direction::Direction;
use crate::direction::detect_direction;
use crate::util::{slugify, word_count};

pub type BlockId = usize;

/// Inline emphasis and behaviour flags for a run of text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub superscript: bool,
    pub subscript: bool,
    pub math: bool,
    /// Destination URL/anchor for link spans.
    pub link: Option<String>,
    /// Footnote label for footnote reference spans.
    pub footnote: Option<String>,
}

/// A contiguous run of text with a single style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyledSpan {
    pub start: usize,
    pub end: usize,
    pub style: SpanStyle,
}

/// A reference to an image that should become its own block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageRef {
    pub src: String,
    pub alt: String,
    pub title: Option<String>,
    pub source_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InlineObject {
    Image(ImageRef),
}

/// Text plus inline styling and a byte map back into the Markdown source.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RichText {
    pub text: String,
    pub spans: Vec<StyledSpan>,
    /// `source_offsets[i]` is the Markdown byte offset where the display byte `i` starts.
    /// The vector always has `text.len() + 1` entries.
    pub source_offsets: Vec<usize>,
    /// `source_ends[i]` is the Markdown byte offset just past display byte `i`.
    #[serde(default)]
    pub source_ends: Vec<usize>,
    pub objects: Vec<(usize, InlineObject)>,
    /// Objects are placed at display byte offsets. This marker is unused by rendering but
    /// kept for forward-compatible serialisation.
    #[serde(default)]
    pub source_range: Range<usize>,
}

impl RichText {
    pub fn new(source_range: Range<usize>) -> Self {
        Self {
            source_offsets: vec![source_range.start],
            source_ends: vec![source_range.start],
            source_range,
            ..Default::default()
        }
    }

    pub fn from_plain(text: impl Into<String>) -> Self {
        let text = text.into();
        let mut rt = Self::new(0..text.len());
        rt.push(&text, &SpanStyle::default(), 0..text.len());
        rt
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    pub fn push(&mut self, text: &str, style: &SpanStyle, source: Range<usize>) {
        if text.is_empty() {
            return;
        }
        let base = self.text.len();
        if self.source_offsets.is_empty() {
            self.source_offsets.push(source.start);
            self.source_ends.push(source.start);
        }
        if self.source_ends.len() != self.source_offsets.len() {
            self.source_ends.resize(self.source_offsets.len(), source.start);
        }
        self.text.push_str(text);
        let src_len = source.end.saturating_sub(source.start);
        let mut wrote_first = false;
        for (offset, ch) in text.char_indices() {
            let n = ch.len_utf8();
            let src_start = source.start.saturating_add(offset.min(src_len));
            let src_end = source
                .start
                .saturating_add(offset.saturating_add(n).min(src_len))
                .max(src_start);
            for _ in 0..n {
                if !wrote_first {
                    // Reuse the boundary placeholder left by the previous fragment.
                    self.source_offsets[base] = src_start;
                    self.source_ends[base] = src_end;
                    wrote_first = true;
                } else {
                    self.source_offsets.push(src_start);
                    self.source_ends.push(src_end);
                }
            }
        }
        if wrote_first {
            self.source_offsets.push(source.end);
            self.source_ends.push(source.end);
        }
        if let Some(last_span) = self.spans.last_mut() {
            if last_span.end == base && last_span.style == *style {
                last_span.end = self.text.len();
                return;
            }
        }
        if !text.trim().is_empty() || style.code {
            self.spans.push(StyledSpan {
                start: base,
                end: self.text.len(),
                style: style.clone(),
            });
        }
    }

    pub fn push_object(&mut self, object: InlineObject) {
        self.objects.push((self.text.len(), object));
    }

    pub fn style_at(&self, byte: usize) -> SpanStyle {
        self.spans
            .iter()
            .rev()
            .find(|s| s.start <= byte && byte < s.end)
            .map(|s| s.style.clone())
            .unwrap_or_default()
    }

    pub fn source_offset_at(&self, display_byte: usize) -> usize {
        let idx = display_byte.min(self.source_offsets.len().saturating_sub(1));
        self.source_offsets.get(idx).copied().unwrap_or(0)
    }

    /// Exclusive source offset for the end of a display range.
    pub fn source_offset_at_end(&self, display_byte: usize) -> usize {
        if display_byte == 0 {
            return self.source_offset_at(0);
        }
        let idx = (display_byte - 1).min(self.source_ends.len().saturating_sub(1));
        self.source_ends
            .get(idx)
            .copied()
            .unwrap_or_else(|| self.source_offset_at(display_byte))
    }

    /// Display-byte range corresponding to a source byte range. Falls back to a search for
    /// the source slice, which handles selections crossing formatting markers.
    pub fn display_range_for_source(&self, source: Range<usize>) -> Option<Range<usize>> {
        if self.source_offsets.is_empty() || self.text.is_empty() {
            return None;
        }
        let start = self
            .source_offsets
            .iter()
            .position(|&src| src >= source.start)?;
        let end = (1..=self.text.len())
            .rev()
            .find(|&i| self.source_ends.get(i - 1).is_some_and(|&e| e <= source.end))
            .map(|i| i.min(self.text.len()))?;
        if start < end {
            Some(start..end)
        } else {
            None
        }
    }

    /// Trim leading/trailing whitespace, preserving source mapping.
    pub fn trim(&mut self) {
        let trimmed_start = self.text.len() - self.text.trim_start().len();
        let trimmed_end = self.text.trim_end().len();
        if trimmed_start >= trimmed_end {
            self.text.clear();
            self.source_offsets = vec![self.source_range.start];
            self.source_ends = vec![self.source_range.start];
            self.spans.clear();
            // Keep inline objects (images) that sit in otherwise whitespace-only text.
            self.objects.retain(|(at, _)| *at <= trimmed_end);
            return;
        }
        if trimmed_start == 0 && trimmed_end == self.text.len() {
            return;
        }
        let new_text = self.text[trimmed_start..trimmed_end].to_string();
        let new_offsets = self.source_offsets[trimmed_start..=trimmed_end].to_vec();
        let new_ends = self.source_ends[trimmed_start..=trimmed_end].to_vec();
        self.spans = self
            .spans
            .iter()
            .filter_map(|span| {
                let start = span.start.max(trimmed_start);
                let end = span.end.min(trimmed_end);
                if start < end {
                    Some(StyledSpan {
                        start: start - trimmed_start,
                        end: end - trimmed_start,
                        style: span.style.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();
        self.objects = self
            .objects
            .iter()
            .filter(|(at, _)| *at >= trimmed_start && *at <= trimmed_end)
            .map(|(at, obj)| (at - trimmed_start, obj.clone()))
            .collect();
        self.text = new_text;
        self.source_offsets = new_offsets;
        self.source_ends = new_ends;
    }

    fn slice(&self, range: Range<usize>) -> RichText {
        if range.start >= range.end {
            return RichText::new(range);
        }
        let start = range.start.min(self.text.len());
        let end = range.end.min(self.text.len());
        let mut rt = RichText::new(self.source_offset_at(start)..self.source_offset_at(end));
        rt.text = self.text[start..end].to_string();
        rt.source_offsets = self.source_offsets[start..=end].to_vec();
        rt.source_ends = self.source_ends[start..=end].to_vec();
        rt.spans = self
            .spans
            .iter()
            .filter_map(|span| {
                let s = span.start.max(start);
                let e = span.end.min(end);
                (s < e).then(|| StyledSpan {
                    start: s - start,
                    end: e - start,
                    style: span.style.clone(),
                })
            })
            .collect();
        rt
    }

    /// Detect and remove a `[ ]`/`[x]` task prefix when the parser did not emit a marker.
    fn take_task_prefix(&mut self) -> Option<bool> {
        let t = self.text.trim_start();
        let leading = self.text.len() - t.len();
        let (checked, used) = if t.starts_with("[ ] ") || t.starts_with("[ ]\t") {
            (false, 4)
        } else if t.starts_with("[x] ") || t.starts_with("[X] ") || t.starts_with("[x]\t") {
            (true, 4)
        } else {
            return None;
        };
        let cut_start = leading;
        let cut_end = leading + used;
        let rest = self.text[cut_end..].to_string();
        let rest_offsets = self.source_offsets[cut_end..].to_vec();
        let rest_ends = self.source_ends[cut_end..].to_vec();
        self.spans = self
            .spans
            .iter()
            .filter_map(|span| {
                let s = span.start.max(cut_end);
                let e = span.end;
                (s < e).then(|| StyledSpan {
                    start: s - cut_end,
                    end: e - cut_end,
                    style: span.style.clone(),
                })
            })
            .collect();
        self.text = rest;
        self.source_offsets = rest_offsets;
        self.source_ends = rest_ends;
        let _ = cut_start;
        Some(checked)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

impl From<CmAlignment> for Align {
    fn from(value: CmAlignment) -> Self {
        match value {
            CmAlignment::None => Align::None,
            CmAlignment::Left => Align::Left,
            CmAlignment::Center => Align::Center,
            CmAlignment::Right => Align::Right,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl QuoteKind {
    pub fn label(self) -> &'static str {
        match self {
            QuoteKind::Note => "Note",
            QuoteKind::Tip => "Tip",
            QuoteKind::Important => "Important",
            QuoteKind::Warning => "Warning",
            QuoteKind::Caution => "Caution",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
    pub source_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Heading {
        level: u8,
        id: String,
        text: RichText,
        source_range: Range<usize>,
    },
    Paragraph {
        text: RichText,
        source_range: Range<usize>,
    },
    List {
        ordered: bool,
        start: u64,
        items: Vec<ListItem>,
        source_range: Range<usize>,
    },
    Code {
        language: Option<String>,
        code: String,
        source_range: Range<usize>,
    },
    Quote {
        kind: Option<QuoteKind>,
        blocks: Vec<Block>,
        source_range: Range<usize>,
    },
    Table {
        align: Vec<Align>,
        header: Vec<RichText>,
        rows: Vec<Vec<RichText>>,
        source_range: Range<usize>,
    },
    Rule {
        source_range: Range<usize>,
    },
    Image {
        image: ImageRef,
        source_range: Range<usize>,
    },
    Math {
        tex: String,
        source_range: Range<usize>,
    },
    Html {
        html: String,
        source_range: Range<usize>,
    },
}

impl Block {
    pub fn source_range(&self) -> Range<usize> {
        match self {
            Block::Heading { source_range, .. }
            | Block::Paragraph { source_range, .. }
            | Block::List { source_range, .. }
            | Block::Code { source_range, .. }
            | Block::Quote { source_range, .. }
            | Block::Table { source_range, .. }
            | Block::Rule { source_range }
            | Block::Image { source_range, .. }
            | Block::Math { source_range, .. }
            | Block::Html { source_range, .. } => source_range.clone(),
        }
    }

    /// Walk nested blocks (quotes and list items) in display order.
    pub fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a Block)) {
        visit(self);
        match self {
            Block::Quote { blocks, .. } => {
                for block in blocks {
                    block.walk(visit);
                }
            }
            Block::List { items, .. } => {
                for item in items {
                    for block in &item.blocks {
                        block.walk(visit);
                    }
                }
            }
            _ => {}
        }
    }

    fn text_content(&self, out: &mut String) {
        match self {
            Block::Heading { text, .. } | Block::Paragraph { text, .. } => {
                out.push_str(&text.text);
                out.push('\n');
            }
            Block::List { items, .. } => {
                for item in items {
                    for block in &item.blocks {
                        block.text_content(out);
                    }
                }
            }
            Block::Code { code, .. } => {
                out.push_str(code);
                out.push('\n');
            }
            Block::Quote { blocks, .. } => {
                for block in blocks {
                    block.text_content(out);
                }
            }
            Block::Table { header, rows, .. } => {
                for cell in header {
                    out.push_str(&cell.text);
                    out.push(' ');
                }
                out.push('\n');
                for row in rows {
                    for cell in row {
                        out.push_str(&cell.text);
                        out.push(' ');
                    }
                    out.push('\n');
                }
            }
            Block::Rule { .. } => {}
            Block::Image { image, .. } => {
                out.push_str(&image.alt);
                out.push('\n');
            }
            Block::Math { tex, .. } => {
                out.push_str(tex);
                out.push('\n');
            }
            Block::Html { html, .. } => {
                out.push_str(html);
                out.push('\n');
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Footnote {
    pub label: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TocEntry {
    pub level: u8,
    pub id: String,
    pub title: String,
    pub source_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkTarget {
    pub dest: String,
    pub text: String,
    pub source_offset: usize,
}

/// A fully parsed Markdown document, ready for layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MdDocument {
    pub blocks: Vec<Block>,
    pub toc: Vec<TocEntry>,
    pub title: Option<String>,
    pub word_count: usize,
    pub reading_minutes: u32,
    pub footnotes: Vec<Footnote>,
    pub links: Vec<LinkTarget>,
    pub frontmatter: BTreeMap<String, String>,
    pub direction_hint: Option<Direction>,
    pub detected_direction: Direction,
    pub total_source_len: usize,
}

impl MdDocument {
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for block in &self.blocks {
            block.text_content(&mut out);
        }
        out
    }

    pub fn heading_by_id(&self, id: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| match b {
            Block::Heading { id: hid, .. } => hid == id,
            _ => false,
        })
    }

    pub fn source_offset_for_block(&self, index: usize) -> Option<usize> {
        self.blocks.get(index).map(|b| b.source_range().start)
    }
}

/// Parse Markdown with GitHub-flavoured extensions, footnotes, math markers and metadata blocks.
pub fn parse_markdown(source: &str) -> MdDocument {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
        | Options::ENABLE_SUPERSCRIPT
        | Options::ENABLE_SUBSCRIPT;

    let events: Vec<(Event<'_>, Range<usize>)> =
        Parser::new_ext(source, options).into_offset_iter().collect();
    let mut parser = EventParser {
        source,
        events,
        pos: 0,
        slugs: BTreeMap::new(),
        toc: Vec::new(),
        footnotes: Vec::new(),
        links: Vec::new(),
        frontmatter: BTreeMap::new(),
        first_h1: None,
        last_task_marker: None,
    };

    let blocks = parser.parse_blocks(&[]);
    let mut title = parser.first_h1.clone();
    if title.is_none() {
        title = parser.frontmatter.get("title").cloned().filter(|s| !s.is_empty());
    }
    let direction_hint = parser
        .frontmatter
        .get("direction")
        .or_else(|| parser.frontmatter.get("dir"))
        .map(|v| match v.trim().to_lowercase().as_str() {
            "rtl" => Direction::Rtl,
            "ltr" => Direction::Ltr,
            _ => Direction::Auto,
        });

    let mut plain = String::new();
    for block in &blocks {
        block.text_content(&mut plain);
    }
    let words = word_count(&plain);
    let reading_minutes = ((words as f32 / 220.0).ceil() as u32).max(1);
    let detected = detect_direction(&plain);
    let direction = match detected {
        crate::direction::ResolvedDirection::Rtl => Direction::Rtl,
        crate::direction::ResolvedDirection::Ltr => Direction::Ltr,
    };

    MdDocument {
        blocks,
        toc: parser.toc,
        title,
        word_count: words,
        reading_minutes,
        footnotes: parser.footnotes,
        links: parser.links,
        frontmatter: parser.frontmatter,
        direction_hint,
        detected_direction: direction,
        total_source_len: source.len(),
    }
}

struct EventParser<'a> {
    source: &'a str,
    events: Vec<(Event<'a>, Range<usize>)>,
    pos: usize,
    slugs: BTreeMap<String, usize>,
    toc: Vec<TocEntry>,
    footnotes: Vec<Footnote>,
    links: Vec<LinkTarget>,
    frontmatter: BTreeMap<String, String>,
    first_h1: Option<String>,
    last_task_marker: Option<bool>,
}

impl<'a> EventParser<'a> {
    fn peek(&self) -> Option<&(Event<'a>, Range<usize>)> {
        self.events.get(self.pos)
    }

    fn advance(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }

    fn parse_blocks(&mut self, stop: &[TagEnd]) -> Vec<Block> {
        let mut blocks = Vec::new();
        while let Some((event, range)) = self.peek().cloned() {
            if let Event::End(end) = &event {
                if stop.contains(end) {
                    self.advance();
                    break;
                }
            }
            match event {
                Event::Start(Tag::Paragraph) => {
                    self.advance();
                    let mut text = self.parse_inlines(Some(TagEnd::Paragraph), false);
                    text.trim();
                    if let Some(math) = display_math_block(&text, &range, self.source) {
                        blocks.push(math);
                    } else if !text.is_empty() || !text.objects.is_empty() {
                        blocks.extend(paragraph_blocks(text, range));
                    }
                }
                Event::Start(Tag::Heading { level, id, .. }) => {
                    self.advance();
                    let mut text = self.parse_inlines(Some(TagEnd::Heading(level)), false);
                    text.trim();
                    let title = text.text.trim().to_string();
                    let mut slug = id
                        .map(|s| s.to_string())
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| slugify(&title));
                    let count = self.slugs.entry(slug.clone()).or_insert(0);
                    *count += 1;
                    if *count > 1 {
                        slug = format!("{slug}-{}", *count);
                    }
                    let lvl = heading_level(level);
                    if lvl == 1 && self.first_h1.is_none() && !title.is_empty() {
                        self.first_h1 = Some(title.clone());
                    }
                    if !title.is_empty() {
                        self.toc.push(TocEntry {
                            level: lvl,
                            id: slug.clone(),
                            title: title.clone(),
                            source_offset: range.start,
                        });
                    }
                    blocks.push(Block::Heading {
                        level: lvl,
                        id: slug,
                        text,
                        source_range: range,
                    });
                }
                Event::Start(Tag::BlockQuote(kind)) => {
                    self.advance();
                    let inner = self.parse_blocks(&[TagEnd::BlockQuote(kind)]);
                    blocks.push(Block::Quote {
                        kind: kind.map(|k| quote_kind(k)),
                        blocks: inner,
                        source_range: range,
                    });
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    self.advance();
                    let language = match kind {
                        CodeBlockKind::Fenced(info) => {
                            info.split_whitespace().next().map(|s| s.to_string())
                        }
                        CodeBlockKind::Indented => None,
                    };
                    let mut code = String::new();
                    while let Some((event, _)) = self.peek().cloned() {
                        match event {
                            Event::Text(text) => {
                                code.push_str(&text);
                                self.advance();
                            }
                            Event::Code(text) => {
                                code.push_str(&text);
                                self.advance();
                            }
                            Event::End(TagEnd::CodeBlock) => {
                                self.advance();
                                break;
                            }
                            _ => self.advance(),
                        }
                    }
                    blocks.push(Block::Code {
                        language,
                        code,
                        source_range: range,
                    });
                }
                Event::Start(Tag::List(start)) => {
                    self.advance();
                    let mut items = Vec::new();
                    while let Some((event, _)) = self.peek().cloned() {
                        match event {
                            Event::Start(Tag::Item) => {
                                let item_range = self.events[self.pos].1.clone();
                                self.advance();
                                self.last_task_marker = None;
                                let mut item_blocks = self.parse_blocks(&[TagEnd::Item]);
                                let mut task = self.last_task_marker.take();
                                if task.is_none() {
                                    if let Some(Block::Paragraph { text, .. }) = item_blocks.first_mut()
                                    {
                                        task = text.take_task_prefix();
                                    }
                                }
                                items.push(ListItem {
                                    task,
                                    blocks: item_blocks,
                                    source_range: item_range,
                                });
                            }
                            Event::End(TagEnd::List(_)) => {
                                self.advance();
                                break;
                            }
                            _ => self.advance(),
                        }
                    }
                    blocks.push(Block::List {
                        ordered: start.is_some(),
                        start: start.unwrap_or(1),
                        items,
                        source_range: range,
                    });
                }
                Event::Start(Tag::Table(alignments)) => {
                    self.advance();
                    let align: Vec<Align> = alignments.into_iter().map(Align::from).collect();
                    let mut header = Vec::new();
                    let mut rows = Vec::new();
                    while let Some((event, _)) = self.peek().cloned() {
                        match event {
                            Event::Start(Tag::TableHead) => {
                                self.advance();
                                while let Some((inner, _)) = self.peek().cloned() {
                                    match inner {
                                        Event::Start(Tag::TableCell) => {
                                            self.advance();
                                            let mut cell =
                                                self.parse_inlines(Some(TagEnd::TableCell), false);
                                            cell.trim();
                                            header.push(cell);
                                        }
                                        Event::End(TagEnd::TableHead) => {
                                            self.advance();
                                            break;
                                        }
                                        _ => self.advance(),
                                    }
                                }
                            }
                            Event::Start(Tag::TableRow) => {
                                self.advance();
                                let mut row = Vec::new();
                                while let Some((inner, _)) = self.peek().cloned() {
                                    match inner {
                                        Event::Start(Tag::TableCell) => {
                                            self.advance();
                                            let mut cell =
                                                self.parse_inlines(Some(TagEnd::TableCell), false);
                                            cell.trim();
                                            row.push(cell);
                                        }
                                        Event::End(TagEnd::TableRow) => {
                                            self.advance();
                                            break;
                                        }
                                        _ => self.advance(),
                                    }
                                }
                                if !row.is_empty() {
                                    rows.push(row);
                                }
                            }
                            Event::End(TagEnd::Table) => {
                                self.advance();
                                break;
                            }
                            _ => self.advance(),
                        }
                    }
                    if !header.is_empty() || !rows.is_empty() {
                        blocks.push(Block::Table {
                            align,
                            header,
                            rows,
                            source_range: range,
                        });
                    }
                }
                Event::Start(Tag::FootnoteDefinition(label)) => {
                    self.advance();
                    let inner = self.parse_blocks(&[TagEnd::FootnoteDefinition]);
                    self.footnotes.push(Footnote {
                        label: label.to_string(),
                        blocks: inner,
                    });
                }
                Event::Start(Tag::HtmlBlock) => {
                    self.advance();
                    let mut html = String::new();
                    while let Some((inner, _)) = self.peek().cloned() {
                        match inner {
                            Event::Text(t) | Event::Html(t) => {
                                html.push_str(&t);
                                self.advance();
                            }
                            Event::End(TagEnd::HtmlBlock) => {
                                self.advance();
                                break;
                            }
                            _ => self.advance(),
                        }
                    }
                    if !html.trim().is_empty() {
                        blocks.push(Block::Html {
                            html,
                            source_range: range,
                        });
                    }
                }
                Event::Start(Tag::MetadataBlock(kind)) => {
                    self.advance();
                    let mut meta = String::new();
                    while let Some((inner, _)) = self.peek().cloned() {
                        match inner {
                            Event::Text(t) | Event::Html(t) => {
                                meta.push_str(&t);
                                self.advance();
                            }
                            Event::End(TagEnd::MetadataBlock(_)) => {
                                self.advance();
                                break;
                            }
                            _ => self.advance(),
                        }
                    }
                    parse_frontmatter(kind, &meta, &mut self.frontmatter);
                }
                Event::DisplayMath(tex) => {
                    self.advance();
                    blocks.push(Block::Math {
                        tex: tex.to_string(),
                        source_range: range,
                    });
                }
                Event::Rule => {
                    self.advance();
                    blocks.push(Block::Rule {
                        source_range: range,
                    });
                }
                Event::Start(Tag::DefinitionList)
                | Event::Start(Tag::DefinitionListTitle)
                | Event::Start(Tag::DefinitionListDefinition) => {
                    self.advance();
                }
                Event::End(TagEnd::DefinitionList)
                | Event::End(TagEnd::DefinitionListTitle)
                | Event::End(TagEnd::DefinitionListDefinition) => {
                    self.advance();
                }
                Event::Start(_) | Event::End(_) => {
                    self.advance();
                }
                Event::Text(_)
                | Event::Code(_)
                | Event::InlineMath(_)
                | Event::Html(_)
                | Event::InlineHtml(_)
                | Event::FootnoteReference(_)
                | Event::SoftBreak
                | Event::HardBreak
                | Event::TaskListMarker(_) => {
                    let mut text = self.parse_inlines(None, true);
                    text.trim();
                    if !text.is_empty() || !text.objects.is_empty() {
                        blocks.extend(paragraph_blocks(text, range));
                    }
                }
            }
        }
        blocks
    }

    /// Parse inline events until `end` is reached or, when `stop_at_block`, until a block-level
    /// container starts (used for tight list items that do not emit a Paragraph tag).
    fn parse_inlines(&mut self, end: Option<TagEnd>, stop_at_block: bool) -> RichText {
        let mut rt = RichText::new(
            self.peek()
                .map(|(_, r)| r.clone())
                .unwrap_or_else(|| 0..0),
        );
        let mut style = SpanStyle::default();
        let mut stack: Vec<(TagEnd, SpanStyle)> = Vec::new();

        while let Some((event, range)) = self.peek().cloned() {
            if let Event::End(tag_end) = &event {
                if end.as_ref() == Some(tag_end) {
                    self.advance();
                    break;
                }
                if end.is_none() {
                    // End of the parent container: do not consume it.
                    break;
                }
                // Pop inline styles.
                if let Some(pos) = stack.iter().rposition(|(t, _)| t == tag_end) {
                    style = stack[pos].1.clone();
                    stack.truncate(pos);
                    self.advance();
                    continue;
                }
                self.advance();
                continue;
            }

            match event {
                Event::Start(tag) => {
                    match tag {
                        Tag::Emphasis => {
                            stack.push((TagEnd::Emphasis, style.clone()));
                            style.italic = true;
                            self.advance();
                        }
                        Tag::Strong => {
                            stack.push((TagEnd::Strong, style.clone()));
                            style.bold = true;
                            self.advance();
                        }
                        Tag::Strikethrough => {
                            stack.push((TagEnd::Strikethrough, style.clone()));
                            style.strikethrough = true;
                            self.advance();
                        }
                        Tag::Superscript => {
                            stack.push((TagEnd::Superscript, style.clone()));
                            style.superscript = true;
                            self.advance();
                        }
                        Tag::Subscript => {
                            stack.push((TagEnd::Subscript, style.clone()));
                            style.subscript = true;
                            self.advance();
                        }
                        Tag::Link { dest_url, .. } => {
                            stack.push((TagEnd::Link, style.clone()));
                            style.link = Some(dest_url.to_string());
                            self.links.push(LinkTarget {
                                dest: dest_url.to_string(),
                                text: String::new(),
                                source_offset: range.start,
                            });
                            self.advance();
                        }
                        Tag::Image { dest_url, title, .. } => {
                            self.advance();
                            let mut alt = RichText::new(range.clone());
                            let mut alt_style = style.clone();
                            let mut alt_stack: Vec<SpanStyle> = Vec::new();
                            while let Some((inner, inner_range)) = self.peek().cloned() {
                                match inner {
                                    Event::End(TagEnd::Image) => {
                                        self.advance();
                                        break;
                                    }
                                    Event::Text(t) => {
                                        alt.push(&t, &alt_style, inner_range);
                                        self.advance();
                                    }
                                    Event::Code(t) => {
                                        let mut code_style = alt_style.clone();
                                        code_style.code = true;
                                        alt.push(&t, &code_style, inner_range);
                                        self.advance();
                                    }
                                    Event::Start(Tag::Emphasis) => {
                                        alt_stack.push(alt_style.clone());
                                        alt_style.italic = true;
                                        self.advance();
                                    }
                                    Event::Start(Tag::Strong) => {
                                        alt_stack.push(alt_style.clone());
                                        alt_style.bold = true;
                                        self.advance();
                                    }
                                    Event::End(TagEnd::Emphasis) | Event::End(TagEnd::Strong) => {
                                        if let Some(prev) = alt_stack.pop() {
                                            alt_style = prev;
                                        }
                                        self.advance();
                                    }
                                    Event::SoftBreak | Event::HardBreak => {
                                        alt.push(" ", &alt_style, inner_range);
                                        self.advance();
                                    }
                                    _ => self.advance(),
                                }
                            }
                            rt.push_object(InlineObject::Image(ImageRef {
                                src: dest_url.to_string(),
                                alt: alt.text.trim().to_string(),
                                title: (!title.is_empty()).then(|| title.to_string()),
                                source_range: range.clone(),
                            }));
                        }
                        // Block-level starts encountered here: stop loose text collection.
                        _ if stop_at_block => break,
                        _ => self.advance(),
                    }
                }
                Event::Text(text) => {
                    rt.push(&text, &style, range);
                    self.advance();
                }
                Event::Code(code) => {
                    let mut code_style = style.clone();
                    code_style.code = true;
                    rt.push(&code, &code_style, range);
                    self.advance();
                }
                Event::InlineMath(tex) => {
                    let mut math_style = style.clone();
                    math_style.math = true;
                    let display = crate::util::tex_to_unicode(&tex);
                    rt.push(&display, &math_style, range);
                    self.advance();
                }
                Event::DisplayMath(tex) => {
                    let mut math_style = style.clone();
                    math_style.math = true;
                    rt.push(&tex, &math_style, range);
                    self.advance();
                }
                Event::InlineHtml(html) => {
                    let lower = html.to_ascii_lowercase();
                    if lower.starts_with("<br") {
                        rt.push("\n", &style, range);
                    }
                    self.advance();
                }
                Event::Html(html) => {
                    rt.push(&html, &style, range);
                    self.advance();
                }
                Event::FootnoteReference(label) => {
                    let mut note_style = style.clone();
                    note_style.superscript = true;
                    note_style.footnote = Some(label.to_string());
                    note_style.link = Some(format!("#fn-{label}"));
                    rt.push(&label, &note_style, range);
                    self.advance();
                }
                Event::SoftBreak => {
                    rt.push(" ", &style, range);
                    self.advance();
                }
                Event::HardBreak => {
                    rt.push("\n", &style, range);
                    self.advance();
                }
                Event::TaskListMarker(checked) => {
                    self.last_task_marker = Some(checked);
                    self.advance();
                }
                Event::Rule => {
                    self.advance();
                }
                Event::End(_) => {
                    self.advance();
                }
            }
        }
        rt
    }
}

fn display_math_block(text: &RichText, range: &Range<usize>, source: &str) -> Option<Block> {
    let trimmed = text.text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let is_pure_math = !text.spans.is_empty()
        && text.spans.iter().all(|s| s.style.math)
        && text.objects.is_empty();
    if !is_pure_math {
        return None;
    }
    let raw = source.get(range.clone()).unwrap_or("");
    if raw.trim_start().starts_with("$$") {
        Some(Block::Math {
            tex: trimmed.to_string(),
            source_range: range.clone(),
        })
    } else {
        None
    }
}

fn paragraph_blocks(mut text: RichText, source_range: Range<usize>) -> Vec<Block> {
    if text.objects.is_empty() {
        return vec![Block::Paragraph {
            text,
            source_range,
        }];
    }
    let mut images = std::mem::take(&mut text.objects);
    images.sort_by_key(|(at, _)| *at);
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for (at, object) in images {
        let at = at.min(text.text.len());
        if at > cursor {
            let mut piece = text.slice(cursor..at);
            piece.trim();
            if !piece.is_empty() {
                let r = piece.source_range.clone();
                out.push(Block::Paragraph {
                    text: piece,
                    source_range: r,
                });
            }
        }
        match object {
            InlineObject::Image(image) => {
                let r = image.source_range.clone();
                out.push(Block::Image {
                    image,
                    source_range: r,
                });
            }
        }
        cursor = at;
    }
    if cursor < text.text.len() {
        let mut piece = text.slice(cursor..text.text.len());
        piece.trim();
        if !piece.is_empty() {
            let r = piece.source_range.clone();
            out.push(Block::Paragraph {
                text: piece,
                source_range: r,
            });
        }
    }
    if out.is_empty() {
        out.push(Block::Paragraph {
            text,
            source_range,
        });
    }
    out
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn quote_kind(kind: CmQuoteKind) -> QuoteKind {
    match kind {
        CmQuoteKind::Note => QuoteKind::Note,
        CmQuoteKind::Tip => QuoteKind::Tip,
        CmQuoteKind::Important => QuoteKind::Important,
        CmQuoteKind::Warning => QuoteKind::Warning,
        CmQuoteKind::Caution => QuoteKind::Caution,
    }
}

fn parse_frontmatter(
    kind: MetadataBlockKind,
    meta: &str,
    out: &mut BTreeMap<String, String>,
) {
    let _ = kind;
    for line in meta.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().trim_start_matches('-').trim().to_lowercase();
        let value = value.trim().trim_matches('"').trim_matches('\'').to_string();
        if !key.is_empty() {
            out.insert(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_paragraph(doc: &MdDocument) -> &RichText {
        doc.blocks
            .iter()
            .find_map(|b| match b {
                Block::Paragraph { text, .. } => Some(text),
                _ => None,
            })
            .expect("paragraph")
    }

    #[test]
    fn parses_headings_and_toc() {
        let doc = parse_markdown("# Title\n\n## Section one\n\nText\n\n### Deep\n");
        assert_eq!(doc.title.as_deref(), Some("Title"));
        assert_eq!(doc.toc.len(), 3);
        assert_eq!(doc.toc[1].level, 2);
        assert_eq!(doc.toc[1].title, "Section one");
    }

    #[test]
    fn produces_unique_slugs() {
        let doc = parse_markdown("# A\n\n# A\n\n# A\n");
        assert_eq!(doc.toc[0].id, "a");
        assert_eq!(doc.toc[1].id, "a-2");
        assert_eq!(doc.toc[2].id, "a-3");
    }

    #[test]
    fn preserves_inline_styles_and_source_offsets() {
        let doc = parse_markdown("Hello **bold** and *italic* and `code`.\n");
        let p = first_paragraph(&doc);
        assert_eq!(p.text, "Hello bold and italic and code.");
        let bold_span = p
            .spans
            .iter()
            .find(|s| s.style.bold)
            .expect("bold span");
        assert_eq!(&p.text[bold_span.start..bold_span.end], "bold");
        let italic_span = p
            .spans
            .iter()
            .find(|s| s.style.italic)
            .expect("italic span");
        assert_eq!(&p.text[italic_span.start..italic_span.end], "italic");
        let code_span = p.spans.iter().find(|s| s.style.code).expect("code span");
        assert_eq!(&p.text[code_span.start..code_span.end], "code");
        // Display byte -> source byte maps into the Markdown source.
        let src = "Hello **bold** and *italic* and `code`.\n";
        let idx = p.text.find("bold").unwrap();
        let src_off = p.source_offset_at(idx);
        assert!(src[src_off..].starts_with("bold"));
    }

    #[test]
    fn parses_links() {
        let doc = parse_markdown("Read [the docs](https://example.com) now.\n");
        assert_eq!(doc.links.len(), 1);
        assert_eq!(doc.links[0].dest, "https://example.com");
        let p = first_paragraph(&doc);
        let link_span = p
            .spans
            .iter()
            .find(|s| s.style.link.is_some())
            .expect("link span");
        assert_eq!(&p.text[link_span.start..link_span.end], "the docs");
    }

    #[test]
    fn parses_persian_text_and_direction() {
        let doc = parse_markdown("# سلام دنیا\n\nاین یک متن فارسی است که از راست به چپ خوانده می‌شود.\n");
        assert_eq!(doc.title.as_deref(), Some("سلام دنیا"));
        assert_eq!(doc.detected_direction, Direction::Rtl);
        assert!(doc.toc[0].id.contains("سلام") || !doc.toc[0].id.is_empty());
    }

    #[test]
    fn respects_frontmatter_direction_and_title() {
        let doc = parse_markdown("---\ntitle: My Doc\ndirection: rtl\n---\n\n# Heading\n");
        assert_eq!(doc.direction_hint, Some(Direction::Rtl));
        assert_eq!(doc.title.as_deref(), Some("Heading"));
        assert_eq!(doc.frontmatter.get("title").map(String::as_str), Some("My Doc"));
    }

    #[test]
    fn parses_task_lists() {
        let doc = parse_markdown("- [x] done\n- [ ] todo\n");
        let Block::List { items, .. } = &doc.blocks[0] else {
            panic!("list expected");
        };
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].task, Some(true));
        assert_eq!(items[1].task, Some(false));
        let Block::Paragraph { text, .. } = &items[0].blocks[0] else {
            panic!("paragraph expected");
        };
        assert_eq!(text.text.trim(), "done");
    }

    #[test]
    fn parses_nested_lists() {
        let doc = parse_markdown("- one\n  - nested\n- two\n");
        let Block::List { items, .. } = &doc.blocks[0] else {
            panic!("list");
        };
        assert_eq!(items.len(), 2);
        assert!(items[0]
            .blocks
            .iter()
            .any(|b| matches!(b, Block::List { .. })));
    }

    #[test]
    fn parses_tables() {
        let md = "| Name | Value |\n|:-----|------:|\n| a | 1 |\n| b | 2 |\n";
        let doc = parse_markdown(md);
        let Block::Table { header, rows, align, .. } = &doc.blocks[0] else {
            panic!("table");
        };
        assert_eq!(header.len(), 2);
        assert_eq!(rows.len(), 2);
        assert_eq!(align[0], Align::Left);
        assert_eq!(align[1], Align::Right);
        assert_eq!(header[0].text, "Name");
        assert_eq!(rows[1][1].text, "2");
    }

    #[test]
    fn parses_code_fence_language() {
        let doc = parse_markdown("```rust\nfn main() {}\n```\n");
        let Block::Code { language, code, .. } = &doc.blocks[0] else {
            panic!("code");
        };
        assert_eq!(language.as_deref(), Some("rust"));
        assert_eq!(code, "fn main() {}\n");
    }

    #[test]
    fn detects_mermaid_as_code_block() {
        let doc = parse_markdown("```mermaid\ngraph TD\nA-->B\n```\n");
        let Block::Code { language, .. } = &doc.blocks[0] else {
            panic!("code");
        };
        assert_eq!(language.as_deref(), Some("mermaid"));
    }

    #[test]
    fn standalone_image_becomes_image_block() {
        let doc = parse_markdown("![Alt text](images/pic.png \"Title\")\n");
        let Block::Image { image, .. } = &doc.blocks[0] else {
            panic!("image block");
        };
        assert_eq!(image.src, "images/pic.png");
        assert_eq!(image.alt, "Alt text");
        assert_eq!(image.title.as_deref(), Some("Title"));
    }

    #[test]
    fn inline_image_splits_paragraph() {
        let doc = parse_markdown("before ![x](a.png) after\n");
        assert!(matches!(doc.blocks[0], Block::Paragraph { .. }));
        assert!(matches!(doc.blocks[1], Block::Image { .. }));
        assert!(matches!(doc.blocks[2], Block::Paragraph { .. }));
    }

    #[test]
    fn parses_footnotes() {
        let doc = parse_markdown("Text with note.[^1]\n\n[^1]: Footnote body.\n");
        assert_eq!(doc.footnotes.len(), 1);
        assert_eq!(doc.footnotes[0].label, "1");
        let p = first_paragraph(&doc);
        let span = p
            .spans
            .iter()
            .find(|s| s.style.footnote.is_some())
            .expect("footnote span");
        assert_eq!(span.style.footnote.as_deref(), Some("1"));
    }

    #[test]
    fn parses_display_math() {
        let doc = parse_markdown("$$\nx^2 + y^2\n$$\n");
        assert!(matches!(doc.blocks[0], Block::Math { .. }));
    }

    #[test]
    fn inline_math_is_marked() {
        let doc = parse_markdown("Euler: $e^{i\\pi}+1=0$.\n");
        let p = first_paragraph(&doc);
        assert!(p.spans.iter().any(|s| s.style.math));
    }

    #[test]
    fn blockquote_kinds_are_parsed() {
        let doc = parse_markdown("> [!WARNING]\n> Careful.\n");
        let Block::Quote { kind, .. } = &doc.blocks[0] else {
            panic!("quote");
        };
        assert_eq!(*kind, Some(QuoteKind::Warning));
    }

    #[test]
    fn word_count_and_reading_time() {
        let md = "one two three four five\n".repeat(100);
        let doc = parse_markdown(&md);
        assert_eq!(doc.word_count, 500);
        assert!(doc.reading_minutes >= 2);
    }

    #[test]
    fn does_not_panic_on_malformed_markdown() {
        for md in [
            "**unclosed",
            "| a | b\n|---|\n| 1 |",
            "```\nunclosed code",
            "[link](",
            "# ",
            "",
            "> > > deep",
            "- ",
            "<!-- comment -->\n",
        ] {
            let _ = parse_markdown(md);
        }
    }

    #[test]
    fn anchor_mapping_can_be_relocated() {
        let src = "# Doc\n\nSome **important** text here.\n";
        let doc = parse_markdown(src);
        let p = first_paragraph(&doc);
        let start = p.text.find("important").unwrap();
        let source_start = p.source_offset_at(start);
        let source_end = p.source_offset_at_end(start + "important".len());
        let anchor = crate::anchor::TextAnchor::create(src, source_start..source_end);
        assert_eq!(anchor.quote, "important");
        let relocated = crate::anchor::relocate_anchor(src, &anchor);
        assert_eq!(relocated.range, Some(source_start..source_end));
    }
}