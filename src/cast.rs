//! Cast (`.cast`): the open container for models and animations that
//! Animora uses for game assets. A file is a tree of nodes; each node has an id,
//! a unique hash, typed properties and child nodes. Little endian.

pub const ID_ROOT: u32 = 0x746F_6F72;
pub const ID_MODEL: u32 = 0x6C64_6F6D;
pub const ID_MESH: u32 = 0x6873_656D;
pub const ID_SKELETON: u32 = 0x6C65_6B73;
pub const ID_BONE: u32 = 0x656E_6F62;
pub const ID_ANIMATION: u32 = 0x6D69_6E61;
pub const ID_CURVE: u32 = 0x7672_7563;
pub const ID_CURVE_OVERRIDE: u32 = 0x564F_4D43;
pub const ID_NOTIFICATION: u32 = 0x6669_746E;
pub const ID_MATERIAL: u32 = 0x6C74_616D;
pub const ID_FILE: u32 = 0x656C_6966;
pub const ID_COLOR: u32 = 0x726C_6F63;
pub const ID_METADATA: u32 = 0x6174_656D;

const MAGIC: u32 = 0x7473_6163; // "cast"

/// Property values; vectors are stored flattened.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Ints(Vec<u64>),
    Floats(Vec<f64>),
    Str(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    /// Type id as stored: b"b\0", b"h\0", b"i\0", b"l\0", b"f\0", b"d\0", b"s\0", b"2v", b"3v", b"4v".
    pub ty: [u8; 2],
    pub val: Val,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CNode {
    pub id: u32,
    pub hash: u64,
    pub props: Vec<(String, Prop)>,
    pub children: Vec<CNode>,
}

fn type_size(ty: [u8; 2]) -> Option<(usize, usize, bool)> {
    // (bytes per component, components per element, is float)
    Some(match &ty {
        b"b\0" => (1, 1, false),
        b"h\0" => (2, 1, false),
        b"i\0" => (4, 1, false),
        b"l\0" => (8, 1, false),
        b"f\0" => (4, 1, true),
        b"d\0" => (8, 1, true),
        b"2v" => (4, 2, true),
        b"3v" => (4, 3, true),
        b"4v" => (4, 4, true),
        _ => return None,
    })
}

/// Smallest integer type that can hold all values (for index buffers).
pub fn int_type_for(values: &[u64]) -> [u8; 2] {
    let max = values.iter().copied().max().unwrap_or(0);
    if max <= 0xFF {
        *b"b\0"
    } else if max <= 0xFFFF {
        *b"h\0"
    } else {
        *b"i\0"
    }
}

static NEXT_HASH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0x534E_4957_5254_5250);

pub fn next_hash() -> u64 {
    NEXT_HASH.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl CNode {
    pub fn new(id: u32) -> CNode {
        CNode { id, hash: next_hash(), props: Vec::new(), children: Vec::new() }
    }

    pub fn prop(&self, name: &str) -> Option<&Prop> {
        self.props.iter().find(|(n, _)| n == name).map(|(_, p)| p)
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        match &self.prop(name)?.val {
            Val::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn ints(&self, name: &str) -> Option<&[u64]> {
        match &self.prop(name)?.val {
            Val::Ints(v) => Some(v),
            _ => None,
        }
    }

    pub fn int(&self, name: &str) -> Option<u64> {
        self.ints(name)?.first().copied()
    }

    /// Numbers of any type as f32 (vectors flattened).
    pub fn floats(&self, name: &str) -> Option<Vec<f32>> {
        match &self.prop(name)?.val {
            Val::Floats(v) => Some(v.iter().map(|x| *x as f32).collect()),
            Val::Ints(v) => Some(v.iter().map(|x| *x as f32).collect()),
            Val::Str(_) => None,
        }
    }

    pub fn kids(&self, id: u32) -> impl Iterator<Item = &CNode> {
        self.children.iter().filter(move |c| c.id == id)
    }

    fn set(&mut self, name: &str, ty: [u8; 2], val: Val) -> &mut Self {
        self.props.retain(|(n, _)| n != name);
        self.props.push((name.to_string(), Prop { ty, val }));
        self
    }

    pub fn set_str(&mut self, name: &str, s: &str) -> &mut Self {
        self.set(name, *b"s\0", Val::Str(s.to_string()))
    }

    pub fn set_ints(&mut self, name: &str, ty: [u8; 2], v: Vec<u64>) -> &mut Self {
        self.set(name, ty, Val::Ints(v))
    }

    pub fn set_floats(&mut self, name: &str, ty: [u8; 2], v: Vec<f32>) -> &mut Self {
        self.set(name, ty, Val::Floats(v.into_iter().map(|x| x as f64).collect()))
    }

    fn prop_len(name: &str, p: &Prop) -> usize {
        8 + name.len()
            + match &p.val {
                Val::Str(s) => s.len() + 1,
                Val::Ints(v) => v.len() * type_size(p.ty).map(|t| t.0).unwrap_or(4),
                Val::Floats(v) => v.len() * type_size(p.ty).map(|t| t.0).unwrap_or(4),
            }
    }

    pub fn len(&self) -> usize {
        24 + self.props.iter().map(|(n, p)| CNode::prop_len(n, p)).sum::<usize>() + self.children.iter().map(|c| c.len()).sum::<usize>()
    }

    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.id.to_le_bytes());
        out.extend_from_slice(&(self.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.hash.to_le_bytes());
        out.extend_from_slice(&(self.props.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.children.len() as u32).to_le_bytes());
        for (name, p) in &self.props {
            let (size, comps, _) = type_size(p.ty).unwrap_or((0, 1, false));
            let count = match &p.val {
                Val::Str(_) => 1,
                Val::Ints(v) => v.len() / comps,
                Val::Floats(v) => v.len() / comps,
            };
            out.extend_from_slice(&p.ty);
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&(count as u32).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            match &p.val {
                Val::Str(s) => {
                    out.extend_from_slice(s.as_bytes());
                    out.push(0);
                }
                Val::Ints(v) => {
                    for x in v {
                        out.extend_from_slice(&x.to_le_bytes()[..size]);
                    }
                }
                Val::Floats(v) => {
                    for x in v {
                        if size == 8 {
                            out.extend_from_slice(&x.to_le_bytes());
                        } else {
                            out.extend_from_slice(&(*x as f32).to_le_bytes());
                        }
                    }
                }
            }
        }
        for c in &self.children {
            c.write(out);
        }
    }
}

struct Rd<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.p + n > self.d.len() {
            return Err(crate::tr!("Datei ist zu kurz oder beschädigt.", "File is too short or damaged.").into());
        }
        let s = &self.d[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self) -> Result<u64, String> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    fn node(&mut self, depth: usize) -> Result<CNode, String> {
        if depth > 64 {
            return Err(crate::tr!("Zu tief verschachtelte Datei.", "File is nested too deeply.").into());
        }
        let start = self.p;
        let id = self.u32()?;
        let size = self.u32()? as usize;
        let hash = self.u64()?;
        let nprops = self.u32()? as usize;
        let nkids = self.u32()? as usize;
        if size < 24 || start + size > self.d.len() {
            return Err(crate::tr!("Ungültige Knotengröße.", "Invalid node size.").into());
        }
        let mut node = CNode { id, hash, props: Vec::new(), children: Vec::new() };
        for _ in 0..nprops {
            let t = self.take(2)?;
            let ty = [t[0], t[1]];
            let name_len = self.u16()? as usize;
            let count = self.u32()? as usize;
            let name = String::from_utf8_lossy(self.take(name_len)?).to_string();
            let val = if &ty == b"s\0" {
                let s0 = self.p;
                while self.p < self.d.len() && self.d[self.p] != 0 {
                    self.p += 1;
                }
                let s = String::from_utf8_lossy(&self.d[s0..self.p.min(self.d.len())]).to_string();
                self.take(1)?;
                Val::Str(s)
            } else {
                let Some((size, comps, is_float)) = type_size(ty) else {
                    return Err(crate::trf!("Unbekannter Eigenschaftstyp {:?}", "Unknown property type {:?}", String::from_utf8_lossy(&ty)));
                };
                let n = count * comps;
                let bytes = self.take(n.checked_mul(size).ok_or_else(|| "overflow".to_string())?)?;
                if is_float {
                    Val::Floats(
                        bytes
                            .chunks_exact(size)
                            .map(|c| if size == 8 { f64::from_le_bytes(c.try_into().unwrap()) } else { f32::from_le_bytes(c.try_into().unwrap()) as f64 })
                            .collect(),
                    )
                } else {
                    Val::Ints(
                        bytes
                            .chunks_exact(size)
                            .map(|c| {
                                let mut b = [0u8; 8];
                                b[..size].copy_from_slice(c);
                                u64::from_le_bytes(b)
                            })
                            .collect(),
                    )
                }
            };
            node.props.push((name, Prop { ty, val }));
        }
        for _ in 0..nkids {
            node.children.push(self.node(depth + 1)?);
        }
        // trust the stored size (skips data we do not understand)
        self.p = start + size;
        Ok(node)
    }
}

/// Reads all root nodes of a cast file.
pub fn parse_cast(data: &[u8]) -> Result<Vec<CNode>, String> {
    let mut r = Rd { d: data, p: 0 };
    if r.u32()? != MAGIC {
        return Err(crate::tr!("Keine Cast-Datei (Kennung fehlt).", "Not a Cast file (magic missing).").into());
    }
    let version = r.u32()?;
    if version != 1 {
        return Err(crate::trf!("Cast-Version {version} wird nicht unterstützt.", "Cast version {version} is not supported."));
    }
    let roots = r.u32()? as usize;
    let _flags = r.u32()?;
    let mut out = Vec::new();
    for _ in 0..roots {
        out.push(r.node(0)?);
    }
    Ok(out)
}

pub fn write_cast(roots: &[CNode]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(roots.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for r in roots {
        r.write(&mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cast_roundtrip() {
        let mut root = CNode::new(ID_ROOT);
        let mut model = CNode::new(ID_MODEL);
        model.set_str("n", "arms");
        let mut mesh = CNode::new(ID_MESH);
        mesh.set_floats("vp", *b"3v", vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
        mesh.set_floats("u0", *b"2v", vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
        mesh.set_ints("f", int_type_for(&[0, 1, 2]), vec![0, 1, 2]);
        mesh.set_ints("m", *b"l\0", vec![0x1234_5678_9ABC_DEF0]);
        model.children.push(mesh);
        root.children.push(model);
        let mut anim = CNode::new(ID_ANIMATION);
        anim.set_floats("fr", *b"f\0", vec![30.0]);
        anim.set_ints("lo", *b"b\0", vec![1]);
        let mut curve = CNode::new(ID_CURVE);
        curve.set_str("nn", "j_gun").set_str("kp", "rq").set_str("m", "absolute");
        curve.set_ints("kb", *b"h\0", vec![0, 300]);
        curve.set_floats("kv", *b"4v", vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.7071, 0.0, 0.7071]);
        anim.children.push(curve);
        root.children.push(anim);
        let bytes = write_cast(&[root.clone()]);
        let back = parse_cast(&bytes).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0], root);
        let c = back[0].kids(ID_ANIMATION).next().unwrap().kids(ID_CURVE).next().unwrap();
        assert_eq!(c.str("kp"), Some("rq"));
        assert_eq!(c.ints("kb"), Some(&[0u64, 300][..]));
        assert_eq!(c.floats("kv").unwrap().len(), 8);
        assert_eq!(back[0].len(), bytes.len() - 16);
    }

    #[test]
    fn cast_rejects_garbage() {
        assert!(parse_cast(b"nope").is_err());
        assert!(parse_cast(&[0x63, 0x61, 0x73, 0x74, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 2]).is_err());
    }
}
