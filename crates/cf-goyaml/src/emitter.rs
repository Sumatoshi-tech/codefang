//! The block YAML emitter over [`cf_gojson::GoValue`] (frozen algorithm).
//!
//! Rather than queueing a full event list, we walk the value tree in the exact
//! order the reference emitter would process events and reproduce its mutable
//! state (`column`, `whitespace`, `indention`, `indent`, the `indents` stack
//! and enough of the `states` stack to drive `increase_indent`). Indent
//! rounding, indicator spacing and the plain/single/double/literal writers are
//! all part of the byte contract pinned by the differential gate; treat them
//! as frozen.

use crate::float;
use crate::resolve;
use crate::scalar::{self, ScalarData};
use cf_gojson::GoValue;

/// Scalar styles the encoder may request.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Style {
    Plain,
    SingleQuoted,
    DoubleQuoted,
    Literal,
}

/// Which kind of parent state sits on top of the `states` stack — the only
/// distinction `increase_indent` needs (the "is the top a block-sequence item"
/// test).
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    BlockSequenceItem,
    Other,
}

pub struct Emitter {
    out: Vec<u8>,
    column: i32,
    whitespace: bool,
    indention: bool,
    indent: i32,
    best_indent: i32,
    /// The marshalling configuration never sets a line width (it resolves to
    /// `i32::MAX`), so plain / single / double scalars are **never** folded;
    /// the `column > best_width` guards thus never fire. They are kept so the
    /// writer logic matches the reference emitter audit-for-audit.
    best_width: i32,
    indents: Vec<i32>,
    states: Vec<State>,
    root_context: bool,
    open_ended: bool,
}

impl Emitter {
    pub fn new(indent: i32) -> Self {
        let best_indent = if !(2..=9).contains(&indent) { 2 } else { indent };
        Emitter {
            out: Vec::new(),
            column: 0,
            whitespace: true,
            indention: true,
            indent: -1,
            best_indent,
            best_width: i32::MAX,
            indents: Vec::new(),
            states: Vec::new(),
            root_context: false,
            open_ended: false,
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.out
    }

    /// Emits the single document body with no `---` and exactly one trailing
    /// newline.
    pub fn marshal_document(&mut self, value: &GoValue) {
        self.root_context = true;
        self.emit_node(value, true, false, false, false);
        // Document end (implicit, no `...`): one write_indent, which appends
        // exactly one break because the root node left indention=false and
        // column > 0. Indent is back to -1 (treated as 0).
        self.indent = -1;
        self.write_indent();
    }

    // --- low-level byte writers (put / write / put_break) ---

    fn put(&mut self, b: u8) {
        self.out.push(b);
        self.column += 1;
    }

    /// Write one UTF-8 codepoint starting at `value[*i]`, advancing `*i`.
    fn write(&mut self, value: &[u8], i: &mut usize) {
        let w = utf8_width(value[*i]);
        self.out.extend_from_slice(&value[*i..*i + w]);
        self.column += 1;
        *i += w;
    }

    /// Write a line break (LF), reset column.
    fn put_break(&mut self) {
        self.out.push(b'\n');
        self.column = 0;
        self.indention = true;
    }

    /// For '\n' emit a break; advance past the break char(s).
    fn write_break(&mut self, value: &[u8], i: &mut usize) {
        if value[*i] == b'\n' {
            self.put_break();
            *i += 1;
        } else {
            // Multi-byte break (CR / NEL / LS / PS): emit '\n' to normalize.
            let w = utf8_width(value[*i]);
            self.put_break();
            *i += w;
        }
    }

    fn write_all(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.put(b);
        }
    }

    // --- indent management ---

    fn increase_indent(&mut self, flow: bool, indentless: bool) {
        self.indents.push(self.indent);
        if self.indent < 0 {
            self.indent = if flow { self.best_indent } else { 0 };
        } else if !indentless {
            let top_is_seq_item = matches!(self.states.last(), Some(State::BlockSequenceItem));
            if top_is_seq_item {
                self.indent += 2;
            } else {
                self.indent =
                    self.best_indent * ((self.indent + self.best_indent) / self.best_indent);
            }
        }
    }

    fn pop_indent(&mut self) {
        self.indent = self.indents.pop().unwrap_or(-1);
    }

    fn write_indent(&mut self) {
        let indent = self.indent.max(0);
        if !self.indention || self.column > indent || (self.column == indent && !self.whitespace) {
            self.put_break();
        }
        while self.column < indent {
            self.put(b' ');
        }
        self.whitespace = true;
    }

    fn write_indicator(
        &mut self,
        indicator: &[u8],
        need_whitespace: bool,
        is_whitespace: bool,
        is_indention: bool,
    ) {
        if need_whitespace && !self.whitespace {
            self.put(b' ');
        }
        self.write_all(indicator);
        self.whitespace = is_whitespace;
        self.indention = self.indention && is_indention;
        self.open_ended = false;
    }

    // --- node dispatch ---

    fn emit_node(
        &mut self,
        value: &GoValue,
        root: bool,
        _sequence: bool,
        _mapping: bool,
        simple_key: bool,
    ) {
        self.root_context = root;
        match value {
            GoValue::Array(items) if !items.is_empty() => self.emit_block_sequence(items),
            GoValue::Map(m) if !m.is_empty() => self.emit_block_mapping(m),
            // Empty collections and scalars.
            _ => self.emit_scalar_or_empty(value, simple_key),
        }
    }

    /// Emits scalars, and empty collections (routed through the flow path,
    /// producing `[]` / `{}`).
    fn emit_scalar_or_empty(&mut self, value: &GoValue, simple_key: bool) {
        match value {
            // A nil slice renders `[]` in YAML, identical to an empty slice
            // (the JSON encoder renders it `null` instead).
            GoValue::Array(_) | GoValue::NilSlice => self.emit_flow_empty(b'[', b']'),
            GoValue::Map(_) => self.emit_flow_empty(b'{', b'}'),
            _ => {
                let (text, requested) = scalar_text_and_style(value);
                self.emit_scalar(&text, requested, simple_key);
            }
        }
    }

    /// Empty flow collection: `[` immediately followed by `]` (or `{}`) —
    /// the immediate-end shape of the flow item path.
    fn emit_flow_empty(&mut self, open: u8, close: u8) {
        self.write_indicator(&[open], true, true, false);
        // increase_indent(flow=true) then immediate end; column != 0 so no
        // write_indent before the close indicator.
        self.write_indicator(&[close], false, false, false);
        self.whitespace = false;
        self.indention = false;
    }

    // --- block sequence ---

    fn emit_block_sequence(&mut self, items: &[GoValue]) {
        // First item: increase_indent(false, false) with the CURRENT top state.
        self.increase_indent(false, false);
        for item in items {
            self.write_indent();
            self.write_indicator(b"-", true, false, true);
            self.states.push(State::BlockSequenceItem);
            self.emit_node(item, false, false, false, false);
            self.states.pop();
        }
        self.pop_indent();
    }

    // --- block mapping ---

    fn emit_block_mapping(&mut self, m: &cf_gojson::GoMap) {
        self.increase_indent(false, false);
        let int_keys = m.origin() == cf_gojson::MapOrigin::IntMap;
        for (k, v) in yaml_key_order(m) {
            self.write_indent();
            // String-origin keys emit as `!!str` scalars (quoted when they
            // would otherwise resolve to another tag). Int-origin keys emit as
            // plain `!!int` scalars (unquoted) — reference-emitter behavior.
            let key_val = if int_keys {
                GoValue::Int(k.parse::<i64>().unwrap_or(0))
            } else {
                GoValue::Str(k.clone())
            };
            let simple = !key_is_multiline(k);
            if simple {
                self.states.push(State::Other);
                self.emit_node(&key_val, false, false, true, true);
                self.states.pop();
                self.emit_block_mapping_value(v, true);
            } else {
                self.write_indicator(b"?", true, false, true);
                self.states.push(State::Other);
                self.emit_node(&key_val, false, false, true, false);
                self.states.pop();
                self.emit_block_mapping_value(v, false);
            }
        }
        self.pop_indent();
    }

    fn emit_block_mapping_value(&mut self, v: &GoValue, simple: bool) {
        if simple {
            // Simple value: ':' with no preceding whitespace.
            self.write_indicator(b":", false, false, false);
        } else {
            // Complex (explicit) key: ':' on its own indented line.
            self.write_indent();
            self.write_indicator(b":", true, false, true);
        }
        self.states.push(State::Other);
        self.emit_node(v, false, false, true, false);
        self.states.pop();
    }

    // --- scalar emission (select style + increase_indent + process_scalar) ---

    fn emit_scalar(&mut self, text: &str, requested: Style, simple_key: bool) {
        let bytes = text.as_bytes();
        let sd = scalar::analyze(bytes);
        let style = self.select_scalar_style(requested, &sd, simple_key, bytes.is_empty());
        // emit_scalar: increase_indent(true, false), process_scalar, restore.
        self.increase_indent(true, false);
        let allow_breaks = !simple_key;
        match style {
            Style::Plain => self.write_plain_scalar(bytes, allow_breaks),
            Style::SingleQuoted => self.write_single_quoted_scalar(bytes, allow_breaks),
            Style::DoubleQuoted => self.write_double_quoted_scalar(bytes, allow_breaks),
            Style::Literal => self.write_literal_scalar(bytes),
        }
        self.pop_indent();
    }

    fn select_scalar_style(
        &self,
        requested: Style,
        sd: &ScalarData,
        simple_key: bool,
        is_empty: bool,
    ) -> Style {
        let mut style = requested;
        // simple_key_context && multiline -> double quoted.
        if simple_key && sd.multiline {
            style = Style::DoubleQuoted;
        }
        if style == Style::Plain {
            // flow_level == 0 here (no flow), so use block_plain_allowed.
            if !sd.block_plain_allowed {
                style = Style::SingleQuoted;
            }
            if is_empty && simple_key {
                style = Style::SingleQuoted;
            }
            // no_tag && !event.implicit: implicit is true for our scalars, so skip.
        }
        if style == Style::SingleQuoted && !sd.single_quoted_allowed {
            style = Style::DoubleQuoted;
        }
        if style == Style::Literal && (!sd.block_allowed || simple_key) {
            style = Style::DoubleQuoted;
        }
        style
    }

    // --- the four scalar writers (frozen byte-contract logic) ---

    fn write_plain_scalar(&mut self, value: &[u8], allow_breaks: bool) {
        if !value.is_empty() && !self.whitespace {
            self.put(b' ');
        }
        let mut spaces = false;
        let mut breaks = false;
        let mut i = 0;
        while i < value.len() {
            if is_space(value, i) {
                if allow_breaks
                    && !spaces
                    && self.column > self.best_width
                    && !is_space(value, i + 1)
                {
                    self.write_indent();
                    i += 1;
                } else {
                    self.write(value, &mut i);
                }
                spaces = true;
            } else if is_break(value, i) {
                if !breaks && value[i] == b'\n' {
                    self.put_break();
                }
                self.write_break(value, &mut i);
                breaks = true;
            } else {
                if breaks {
                    self.write_indent();
                }
                self.write(value, &mut i);
                self.indention = false;
                spaces = false;
                breaks = false;
            }
        }
        if !value.is_empty() {
            self.whitespace = false;
        }
        self.indention = false;
        if self.root_context {
            self.open_ended = true;
        }
    }

    fn write_single_quoted_scalar(&mut self, value: &[u8], allow_breaks: bool) {
        self.write_indicator(b"\'", true, false, false);
        let mut spaces = false;
        let mut breaks = false;
        let mut i = 0;
        while i < value.len() {
            if is_space(value, i) {
                if allow_breaks
                    && !spaces
                    && self.column > self.best_width
                    && i > 0
                    && i < value.len() - 1
                    && !is_space(value, i + 1)
                {
                    self.write_indent();
                    i += 1;
                } else {
                    self.write(value, &mut i);
                }
                spaces = true;
            } else if is_break(value, i) {
                if !breaks && value[i] == b'\n' {
                    self.put_break();
                }
                self.write_break(value, &mut i);
                breaks = true;
            } else {
                if breaks {
                    self.write_indent();
                }
                if value[i] == b'\'' {
                    self.put(b'\'');
                }
                self.write(value, &mut i);
                self.indention = false;
                spaces = false;
                breaks = false;
            }
        }
        self.write_indicator(b"\'", false, false, false);
        self.whitespace = false;
        self.indention = false;
    }

    fn write_double_quoted_scalar(&mut self, value: &[u8], allow_breaks: bool) {
        let mut spaces = false;
        self.write_indicator(b"\"", true, false, false);
        let mut i = 0;
        while i < value.len() {
            let needs_escape = !is_printable(value, i)
                || is_bom(value, i)
                || is_break(value, i)
                || value[i] == b'"'
                || value[i] == b'\\';
            if needs_escape {
                let (v, w) = decode_utf8(value, i);
                i += w;
                self.put(b'\\');
                self.write_escape(v);
                spaces = false;
            } else if is_space(value, i) {
                if allow_breaks
                    && !spaces
                    && self.column > self.best_width
                    && i > 0
                    && i < value.len() - 1
                {
                    self.write_indent();
                    if is_space(value, i + 1) {
                        self.put(b'\\');
                    }
                    i += 1;
                } else {
                    self.write(value, &mut i);
                }
                spaces = true;
            } else {
                self.write(value, &mut i);
                spaces = false;
            }
        }
        self.write_indicator(b"\"", false, false, false);
        self.whitespace = false;
        self.indention = false;
    }

    fn write_escape(&mut self, v: u32) {
        match v {
            0x00 => self.put(b'0'),
            0x07 => self.put(b'a'),
            0x08 => self.put(b'b'),
            0x09 => self.put(b't'),
            0x0A => self.put(b'n'),
            0x0B => self.put(b'v'),
            0x0C => self.put(b'f'),
            0x0D => self.put(b'r'),
            0x1B => self.put(b'e'),
            0x22 => self.put(b'"'),
            0x5C => self.put(b'\\'),
            0x85 => self.put(b'N'),
            0xA0 => self.put(b'_'),
            0x2028 => self.put(b'L'),
            0x2029 => self.put(b'P'),
            _ => {
                let w: u32 = if v <= 0xFF {
                    self.put(b'x');
                    2
                } else if v <= 0xFFFF {
                    self.put(b'u');
                    4
                } else {
                    self.put(b'U');
                    8
                };
                let mut k: i32 = ((w - 1) * 4) as i32;
                while k >= 0 {
                    let digit = ((v >> (k as u32)) & 0x0F) as u8;
                    if digit < 10 {
                        self.put(digit + b'0');
                    } else {
                        self.put(digit + b'A' - 10);
                    }
                    k -= 4;
                }
            }
        }
    }

    fn write_literal_scalar(&mut self, value: &[u8]) {
        self.write_indicator(b"|", true, false, false);
        self.write_block_scalar_hints(value);
        self.whitespace = true;
        let mut breaks = true;
        let mut i = 0;
        while i < value.len() {
            if is_break(value, i) {
                self.write_break(value, &mut i);
                breaks = true;
            } else {
                if breaks {
                    self.write_indent();
                }
                self.write(value, &mut i);
                self.indention = false;
                breaks = false;
            }
        }
    }

    fn write_block_scalar_hints(&mut self, value: &[u8]) {
        if !value.is_empty() && (is_space(value, 0) || is_break(value, 0)) {
            let indent_hint = [b'0' + self.best_indent as u8];
            self.write_indicator(&indent_hint, false, false, false);
        }
        self.open_ended = false;
        let mut chomp: u8 = 0;
        if value.is_empty() {
            chomp = b'-';
        } else {
            // Find last codepoint start.
            let mut i = value.len() - 1;
            while value[i] & 0xC0 == 0x80 {
                i -= 1;
            }
            if !is_break(value, i) {
                chomp = b'-';
            } else if i == 0 {
                chomp = b'+';
                self.open_ended = true;
            } else {
                i -= 1;
                while value[i] & 0xC0 == 0x80 {
                    i -= 1;
                }
                if is_break(value, i) {
                    chomp = b'+';
                    self.open_ended = true;
                }
            }
        }
        if chomp != 0 {
            self.write_indicator(&[chomp], false, false, false);
        }
    }
}

/// Maps a non-collection [`GoValue`] to its scalar text and the style the
/// encoder *requests* (before the emitter downgrades plain→single/double).
fn scalar_text_and_style(value: &GoValue) -> (String, Style) {
    match value {
        GoValue::Null => ("null".to_string(), Style::Plain),
        GoValue::Bool(b) => ((if *b { "true" } else { "false" }).to_string(), Style::Plain),
        GoValue::Int(i) => (i.to_string(), Style::Plain),
        GoValue::Uint(u) => (u.to_string(), Style::Plain),
        GoValue::Float(f) => (float::format_g(*f), Style::Plain),
        GoValue::Str(s) => string_style(s),
        // Collections are handled before reaching here. A nil slice renders
        // `[]`, identical to an empty slice.
        GoValue::Array(_) | GoValue::NilSlice => ("[]".to_string(), Style::Plain),
        GoValue::Map(_) => ("{}".to_string(), Style::Plain),
    }
}

/// Chooses a string's requested style: Literal (contains a newline), Plain
/// (resolves to str), or DoubleQuoted (would resolve to a non-str tag).
fn string_style(s: &str) -> (String, Style) {
    // A Rust &str is always valid UTF-8, so the reference encoder's
    // invalid-UTF-8 base64 branch is unreachable here.
    let can_use_plain = resolve::string_can_use_plain(s);
    let style = if s.contains('\n') {
        // not in flow -> literal block scalar
        Style::Literal
    } else if can_use_plain {
        Style::Plain
    } else {
        Style::DoubleQuoted
    };
    (s.to_string(), style)
}

// --- byte helpers shared with the writers ---

/// A string map key is rendered as a complex (`?`) key exactly when it fails
/// the simple-key check, i.e. the scalar is multiline (the analyzer set
/// `multiline`).
fn key_is_multiline(k: &str) -> bool {
    scalar::analyze(k.as_bytes()).multiline
}

/// Orders a block-mapping's entries for emission (YAML key order contract).
///
/// Struct-origin maps keep declaration order; map-origin maps sort with
/// [`yaml_key_less`], the reference emitter's key comparator, which orders
/// embedded digit runs *numerically* (e.g. `a2` < `a10`). Note this differs
/// from the JSON byte-sort. Keys here are always strings, so the comparator's
/// numeric/bool fast path never applies and only the rune-comparison branch is
/// implemented.
fn yaml_key_order(m: &cf_gojson::GoMap) -> Vec<&(String, GoValue)> {
    let mut refs: Vec<&(String, GoValue)> = m.entries().iter().collect();
    match m.origin() {
        cf_gojson::MapOrigin::Map => {
            // `yaml_key_less` is a total order over distinct map keys (keys
            // are unique), so sort stability is irrelevant here.
            refs.sort_by(|a, b| {
                if yaml_key_less(&a.0, &b.0) {
                    std::cmp::Ordering::Less
                } else if yaml_key_less(&b.0, &a.0) {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            });
        }
        // Integer map keys sort by integer value (the key comparator's
        // numeric fast path).
        cf_gojson::MapOrigin::IntMap => {
            refs.sort_by(|a, b| {
                match (a.0.parse::<i64>(), b.0.parse::<i64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    _ => a.0.as_bytes().cmp(b.0.as_bytes()),
                }
            });
        }
        cf_gojson::MapOrigin::Struct => {}
    }
    refs
}

/// The string branch of the reference emitter's key comparator (frozen).
///
/// Both inputs are compared as sequences of `char`. Equal prefixes are skipped (tracking
/// whether the last matched rune was a digit); at the first differing position:
/// letters compare by code point; a letter vs non-letter is ordered by the
/// `digits` context; otherwise the maximal digit runs starting at that position
/// are compared as base-10 integers (with leading-zero handling), then by run
/// length, then by raw rune. Equal-prefix strings order by length.
fn yaml_key_less(a: &str, b: &str) -> bool {
    let ar: Vec<char> = a.chars().collect();
    let br: Vec<char> = b.chars().collect();
    let mut digits = false;
    let n = ar.len().min(br.len());
    let mut i = 0;
    while i < n {
        if ar[i] == br[i] {
            digits = ar[i].is_ascii_digit();
            i += 1;
            continue;
        }
        let al = is_letter(ar[i]);
        let bl = is_letter(br[i]);
        if al && bl {
            return ar[i] < br[i];
        }
        if al || bl {
            return if digits { al } else { bl };
        }
        // Both differing runes are non-letters: compare digit runs numerically.
        let mut an: i64 = 0;
        let mut bn: i64 = 0;
        if ar[i] == '0' || br[i] == '0' {
            let mut j = i as isize - 1;
            while j >= 0 && ar[j as usize].is_ascii_digit() {
                if ar[j as usize] != '0' {
                    an = 1;
                    bn = 1;
                    break;
                }
                j -= 1;
            }
        }
        let mut ai = i;
        while ai < ar.len() && ar[ai].is_ascii_digit() {
            an = an * 10 + (ar[ai] as i64 - '0' as i64);
            ai += 1;
        }
        let mut bi = i;
        while bi < br.len() && br[bi].is_ascii_digit() {
            bn = bn * 10 + (br[bi] as i64 - '0' as i64);
            bi += 1;
        }
        if an != bn {
            return an < bn;
        }
        if ai != bi {
            return ai < bi;
        }
        return ar[i] < br[i];
    }
    ar.len() < br.len()
}

/// Letter test for the key comparator: the Unicode "Letter" supercategory,
/// which is exactly `char::is_alphabetic`.
fn is_letter(c: char) -> bool {
    c.is_alphabetic()
}

fn utf8_width(b: u8) -> usize {
    if b & 0x80 == 0x00 {
        1
    } else if b & 0xE0 == 0xC0 {
        2
    } else if b & 0xF0 == 0xE0 {
        3
    } else {
        4
    }
}

fn is_space(v: &[u8], i: usize) -> bool {
    i < v.len() && v[i] == b' '
}

fn is_break(v: &[u8], i: usize) -> bool {
    if i >= v.len() {
        return false;
    }
    let b = v[i];
    if b == b'\n' || b == b'\r' {
        return true;
    }
    if b == 0xC2 && i + 1 < v.len() && v[i + 1] == 0x85 {
        return true;
    }
    if b == 0xE2 && i + 2 < v.len() && v[i + 1] == 0x80 && (v[i + 2] == 0xA8 || v[i + 2] == 0xA9) {
        return true;
    }
    false
}

/// BOM test — deliberately tests the buffer **start**, not position `i`
/// (reference-emitter behavior, kept for parity).
fn is_bom(v: &[u8], _i: usize) -> bool {
    v.len() >= 3 && v[0] == 0xEF && v[1] == 0xBB && v[2] == 0xBF
}

/// Byte-prefix printability test (the reference emitter's table). Note this
/// returns **false** for 4-byte (astral-plane) UTF-8, so emoji are escaped.
fn is_printable(b: &[u8], i: usize) -> bool {
    let g = |j: usize| -> u8 {
        if i + j < b.len() {
            b[i + j]
        } else {
            0
        }
    };
    (b[i] == 0x0A)
        || (b[i] >= 0x20 && b[i] <= 0x7E)
        || (b[i] == 0xC2 && g(1) >= 0xA0)
        || (b[i] > 0xC2 && b[i] < 0xED)
        || (b[i] == 0xED && g(1) < 0xA0)
        || (b[i] == 0xEE)
        || (b[i] == 0xEF
            && !(g(1) == 0xBB && g(2) == 0xBF)
            && !(g(1) == 0xBF && (g(2) == 0xBE || g(2) == 0xBF)))
}

/// Decodes the UTF-8 codepoint at `v[i]`, returning `(codepoint, width)`.
fn decode_utf8(v: &[u8], i: usize) -> (u32, usize) {
    let b = v[i];
    let w = utf8_width(b);
    let mut cp: u32 = match w {
        1 => return (b as u32 & 0x7F, 1),
        2 => b as u32 & 0x1F,
        3 => b as u32 & 0x0F,
        _ => b as u32 & 0x07,
    };
    for k in 1..w {
        if i + k < v.len() {
            cp = (cp << 6) | (v[i + k] as u32 & 0x3F);
        }
    }
    (cp, w)
}
