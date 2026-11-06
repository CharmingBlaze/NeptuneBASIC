//! Native command signatures.
//!
//! Commands are declared with Blitz3D's `rtSym` signature notation so the
//! command tables read like the original runtime:
//!
//! ```text
//! "%CreateCube%parent=0"           returns Int, one Int parameter defaulting to 0
//! "PositionEntity%entity#x#y#z%global=0"
//! "$Left$string%count"
//! "Print?value=\"\""               `?` = any value (Variant)
//! ```
//!
//! The leading tag (if any) is the return type; each parameter is a tag
//! (`%` int, `#` float, `$` string, `?` any) followed by its name and an
//! optional `=default`.

use std::rc::Rc;

/// Value type used by native command parameters and constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTy {
    Int,
    Float,
    Str,
    Any,
}

/// A compile-time constant value.
#[derive(Clone, Debug, PartialEq)]
pub enum ConstVal {
    Int(i32),
    Float(f32),
    Str(Rc<str>),
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: NativeTy,
    pub default: Option<ConstVal>,
}

#[derive(Clone, Debug)]
pub struct CommandSig {
    /// Lower-cased lookup name.
    pub name: String,
    /// Name as declared (for diagnostics and documentation).
    pub display: String,
    pub ret: Option<NativeTy>,
    pub params: Vec<Param>,
}

impl CommandSig {
    /// Number of parameters without a default value.
    pub fn required(&self) -> usize {
        self.params.iter().filter(|p| p.default.is_none()).count()
    }
}

fn tag_ty(c: char) -> Option<NativeTy> {
    match c {
        '%' => Some(NativeTy::Int),
        '#' => Some(NativeTy::Float),
        '$' => Some(NativeTy::Str),
        '?' => Some(NativeTy::Any),
        _ => None,
    }
}

/// Parses a Blitz-style signature string.
///
/// # Panics
/// Panics on a malformed signature; signatures are static tables written by
/// runtime authors, so this is a programming error.
pub fn parse_sig(sig: &str) -> CommandSig {
    let chars: Vec<char> = sig.chars().collect();
    let mut i = 0;
    let ret = tag_ty(chars[0]);
    if ret.is_some() {
        i += 1;
    }
    let name_start = i;
    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
        i += 1;
    }
    let display: String = chars[name_start..i].iter().collect();
    assert!(!display.is_empty(), "bad command signature '{sig}'");
    let mut params = Vec::new();
    while i < chars.len() {
        let ty = tag_ty(chars[i]).unwrap_or_else(|| panic!("bad parameter tag in '{sig}'"));
        i += 1;
        let s = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
        let name: String = chars[s..i].iter().collect();
        let mut default = None;
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            if chars[i] == '"' {
                i += 1;
                let s = i;
                while chars[i] != '"' {
                    i += 1;
                }
                let text: String = chars[s..i].iter().collect();
                i += 1;
                default = Some(ConstVal::Str(text.into()));
            } else {
                let s = i;
                while i < chars.len() && tag_ty(chars[i]).is_none() {
                    i += 1;
                }
                let text: String = chars[s..i].iter().collect();
                default = Some(match ty {
                    NativeTy::Float => ConstVal::Float(text.parse().unwrap_or_else(|_| panic!("bad default in '{sig}'"))),
                    NativeTy::Str => ConstVal::Str(text.into()),
                    _ => {
                        if text.contains('.') {
                            ConstVal::Float(text.parse().unwrap())
                        } else {
                            ConstVal::Int(text.parse().unwrap_or_else(|_| panic!("bad default in '{sig}'")))
                        }
                    }
                });
            }
        }
        params.push(Param { name, ty, default });
    }
    CommandSig { name: display.to_ascii_lowercase(), display, ret, params }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blitz_signatures() {
        let s = parse_sig("%CreateSphere%segments=8%parent=0");
        assert_eq!(s.name, "createsphere");
        assert_eq!(s.ret, Some(NativeTy::Int));
        assert_eq!(s.params.len(), 2);
        assert_eq!(s.params[0].default, Some(ConstVal::Int(8)));
        assert_eq!(s.required(), 0);

        let s = parse_sig("PositionEntity%entity#x#y#z%global=0");
        assert_eq!(s.ret, None);
        assert_eq!(s.required(), 4);
        assert_eq!(s.params[1].ty, NativeTy::Float);

        let s = parse_sig("Print?value=\"\"");
        assert_eq!(s.params[0].default, Some(ConstVal::Str("".into())));
        let s = parse_sig("UpdateWorld#elapsed_time=1");
        assert_eq!(s.params[0].default, Some(ConstVal::Float(1.0)));
    }
}
