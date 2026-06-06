//! Port of yaml.v3 `yaml_emitter_analyze_scalar` (emitterc.go).
//!
//! Computes the structural properties of a scalar's *characters* that decide
//! which quoting styles are legal. The encoder operates on UTF-8 bytes;
//! `width`/`is_printable`/`is_break` etc. are byte-oriented, matching libyaml.

/// Per-scalar analysis flags used by the style selector.
#[derive(Debug, Default, Clone, Copy)]
pub struct ScalarData {
    pub multiline: bool,
    pub flow_plain_allowed: bool,
    pub block_plain_allowed: bool,
    pub single_quoted_allowed: bool,
    pub block_allowed: bool,
}

/// UTF-8 lead-byte width.
fn width(b: u8) -> usize {
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

fn is_space_at(v: &[u8], i: usize) -> bool {
    v[i] == b' '
}

fn is_break_at(v: &[u8], i: usize) -> bool {
    // is_break: '\n' | '\r' | NEL(U+0085) | LS(U+2028) | PS(U+2029)
    let b = v[i];
    if b == b'\n' || b == b'\r' {
        return true;
    }
    if b == 0xC2 && i + 1 < v.len() && v[i + 1] == 0x85 {
        return true; // U+0085
    }
    if b == 0xE2 && i + 2 < v.len() && v[i + 1] == 0x80 && (v[i + 2] == 0xA8 || v[i + 2] == 0xA9) {
        return true; // U+2028 / U+2029
    }
    false
}

fn is_blank_at(v: &[u8], i: usize) -> bool {
    i < v.len() && (v[i] == b' ' || v[i] == b'\t')
}

/// `is_blankz`: blank, break, or end-of-input (`z`).
fn is_blankz_at(v: &[u8], i: usize) -> bool {
    i >= v.len() || is_blank_at(v, i) || is_break_at(v, i)
}

fn is_ascii_at(v: &[u8], i: usize) -> bool {
    v[i] < 0x80
}

/// Byte-prefix `is_printable`, identical to yaml.v3 `yamlprivateh.go`.
/// Returns **false** for 4-byte (astral-plane) UTF-8, so emoji are escaped.
fn is_printable_at(v: &[u8], i: usize) -> bool {
    let g = |j: usize| -> u8 {
        if i + j < v.len() {
            v[i + j]
        } else {
            0
        }
    };
    (v[i] == 0x0A)
        || (v[i] >= 0x20 && v[i] <= 0x7E)
        || (v[i] == 0xC2 && g(1) >= 0xA0)
        || (v[i] > 0xC2 && v[i] < 0xED)
        || (v[i] == 0xED && g(1) < 0xA0)
        || (v[i] == 0xEE)
        || (v[i] == 0xEF
            && !(g(1) == 0xBB && g(2) == 0xBF)
            && !(g(1) == 0xBF && (g(2) == 0xBE || g(2) == 0xBF)))
}

/// Analyzes `value` (UTF-8 bytes) exactly as `yaml_emitter_analyze_scalar`.
/// `unicode` is always true for yaml.v3 Marshal (`set_unicode(true)`).
#[must_use]
pub fn analyze(value: &[u8]) -> ScalarData {
    let mut block_indicators = false;
    let mut flow_indicators = false;
    let mut line_breaks = false;
    let mut special_characters = false;
    let mut tab_characters = false;

    let mut leading_space = false;
    let mut leading_break = false;
    let mut trailing_space = false;
    let mut trailing_break = false;
    let mut break_space = false;
    let mut space_break = false;

    let mut preceded_by_whitespace;
    let mut followed_by_whitespace;
    let mut previous_space = false;
    let mut previous_break = false;

    if value.is_empty() {
        return ScalarData {
            multiline: false,
            flow_plain_allowed: false,
            block_plain_allowed: true,
            single_quoted_allowed: true,
            block_allowed: false,
        };
    }

    if value.len() >= 3
        && ((value[0] == b'-' && value[1] == b'-' && value[2] == b'-')
            || (value[0] == b'.' && value[1] == b'.' && value[2] == b'.'))
    {
        block_indicators = true;
        flow_indicators = true;
    }

    preceded_by_whitespace = true;
    let mut i = 0;
    while i < value.len() {
        let w = width(value[i]);
        followed_by_whitespace = i + w >= value.len() || is_blank_at(value, i + w);

        if i == 0 {
            match value[i] {
                b'#' | b',' | b'[' | b']' | b'{' | b'}' | b'&' | b'*' | b'!' | b'|' | b'>'
                | b'\'' | b'"' | b'%' | b'@' | b'`' => {
                    flow_indicators = true;
                    block_indicators = true;
                }
                b'?' | b':' => {
                    flow_indicators = true;
                    if followed_by_whitespace {
                        block_indicators = true;
                    }
                }
                b'-' => {
                    if followed_by_whitespace {
                        flow_indicators = true;
                        block_indicators = true;
                    }
                }
                _ => {}
            }
        } else {
            match value[i] {
                b',' | b'?' | b'[' | b']' | b'{' | b'}' => {
                    flow_indicators = true;
                }
                b':' => {
                    flow_indicators = true;
                    if followed_by_whitespace {
                        block_indicators = true;
                    }
                }
                b'#' => {
                    if preceded_by_whitespace {
                        flow_indicators = true;
                        block_indicators = true;
                    }
                }
                _ => {}
            }
        }

        if value[i] == b'\t' {
            tab_characters = true;
        } else if !is_printable_at(value, i) {
            // yaml.v3: `!is_printable || (!is_ascii && !unicode)`. Marshal always
            // sets unicode=true, so the second term is dead and elided here.
            special_characters = true;
        }

        if is_space_at(value, i) {
            if i == 0 {
                leading_space = true;
            }
            if i + width(value[i]) == value.len() {
                trailing_space = true;
            }
            if previous_break {
                break_space = true;
            }
            previous_space = true;
            previous_break = false;
        } else if is_break_at(value, i) {
            line_breaks = true;
            if i == 0 {
                leading_break = true;
            }
            if i + width(value[i]) == value.len() {
                trailing_break = true;
            }
            if previous_space {
                space_break = true;
            }
            previous_space = false;
            previous_break = true;
        } else {
            previous_space = false;
            previous_break = false;
        }

        preceded_by_whitespace = is_blankz_at(value, i);
        i += w;
    }

    let mut sd = ScalarData {
        multiline: line_breaks,
        flow_plain_allowed: true,
        block_plain_allowed: true,
        single_quoted_allowed: true,
        block_allowed: true,
    };

    if leading_space || leading_break || trailing_space || trailing_break {
        sd.flow_plain_allowed = false;
        sd.block_plain_allowed = false;
    }
    if trailing_space {
        sd.block_allowed = false;
    }
    if break_space {
        sd.flow_plain_allowed = false;
        sd.block_plain_allowed = false;
        sd.single_quoted_allowed = false;
    }
    if space_break || tab_characters || special_characters {
        sd.flow_plain_allowed = false;
        sd.block_plain_allowed = false;
        sd.single_quoted_allowed = false;
    }
    if space_break || special_characters {
        sd.block_allowed = false;
    }
    if line_breaks {
        sd.flow_plain_allowed = false;
        sd.block_plain_allowed = false;
    }
    if flow_indicators {
        sd.flow_plain_allowed = false;
    }
    if block_indicators {
        sd.block_plain_allowed = false;
    }
    sd
}
