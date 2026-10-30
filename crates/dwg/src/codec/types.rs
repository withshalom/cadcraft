//! Object type codes (ODA specification §20.3): the fixed codes below 500, and how a code of 500 or
//! more resolves through the class list.

use super::classes::Classes;

/// The DXF name of a fixed object type code, or `None` for codes the format doesn't fix.
pub fn fixed_name(code: u16) -> Option<&'static str> {
    Some(match code {
        0x01 => "TEXT",
        0x02 => "ATTRIB",
        0x03 => "ATTDEF",
        0x04 => "BLOCK",
        0x05 => "ENDBLK",
        0x06 => "SEQEND",
        0x07 => "INSERT",
        0x08 => "MINSERT",
        0x0A => "VERTEX_2D",
        0x0B => "VERTEX_3D",
        0x0C => "VERTEX_MESH",
        0x0D => "VERTEX_PFACE",
        0x0E => "VERTEX_PFACE_FACE",
        0x0F => "POLYLINE_2D",
        0x10 => "POLYLINE_3D",
        0x11 => "ARC",
        0x12 => "CIRCLE",
        0x13 => "LINE",
        0x14 => "DIMENSION_ORDINATE",
        0x15 => "DIMENSION_LINEAR",
        0x16 => "DIMENSION_ALIGNED",
        0x17 => "DIMENSION_ANG3PT",
        0x18 => "DIMENSION_ANG2LN",
        0x19 => "DIMENSION_RADIUS",
        0x1A => "DIMENSION_DIAMETER",
        0x1B => "POINT",
        0x1C => "3DFACE",
        0x1D => "POLYLINE_PFACE",
        0x1E => "POLYLINE_MESH",
        0x1F => "SOLID",
        0x20 => "TRACE",
        0x21 => "SHAPE",
        0x22 => "VIEWPORT",
        0x23 => "ELLIPSE",
        0x24 => "SPLINE",
        0x25 => "REGION",
        0x26 => "3DSOLID",
        0x27 => "BODY",
        0x28 => "RAY",
        0x29 => "XLINE",
        0x2A => "DICTIONARY",
        0x2B => "OLEFRAME",
        0x2C => "MTEXT",
        0x2D => "LEADER",
        0x2E => "TOLERANCE",
        0x2F => "MLINE",
        0x30 => "BLOCK_CONTROL",
        0x31 => "BLOCK_HEADER",
        0x32 => "LAYER_CONTROL",
        0x33 => "LAYER",
        0x34 => "STYLE_CONTROL",
        0x35 => "STYLE",
        0x38 => "LTYPE_CONTROL",
        0x39 => "LTYPE",
        0x3C => "VIEW_CONTROL",
        0x3D => "VIEW",
        0x3E => "UCS_CONTROL",
        0x3F => "UCS",
        0x40 => "VPORT_CONTROL",
        0x41 => "VPORT",
        0x42 => "APPID_CONTROL",
        0x43 => "APPID",
        0x44 => "DIMSTYLE_CONTROL",
        0x45 => "DIMSTYLE",
        0x46 => "VX_CONTROL",
        0x47 => "VX_TABLE_RECORD",
        0x48 => "GROUP",
        0x49 => "MLINESTYLE",
        0x4A => "OLE2FRAME",
        0x4B => "DUMMY",
        0x4C => "LONG_TRANSACTION",
        0x4D => "LWPOLYLINE",
        0x4E => "HATCH",
        0x4F => "XRECORD",
        0x50 => "ACDBPLACEHOLDER",
        0x51 => "VBA_PROJECT",
        0x52 => "LAYOUT",
        0x1F2 => "ACAD_PROXY_ENTITY",
        0x1F3 => "ACAD_PROXY_OBJECT",
        _ => return None,
    })
}

/// Fixed codes of entities (graphical objects), which carry the common entity data (§20.2).
pub fn fixed_is_entity(code: u16) -> bool {
    matches!(code, 0x01..=0x08 | 0x0A..=0x29 | 0x2B..=0x2F | 0x4A | 0x4D | 0x4E | 0x1F2)
}

/// The name of any type code: fixed, or the DXF name of its class.
pub fn name(code: u16, classes: &Classes) -> String {
    match fixed_name(code) {
        Some(n) => n.to_string(),
        None if code >= 500 => classes.get(code).map_or_else(|| format!("class {code}"), |c| c.dxf_name.clone()),
        None => format!("type {code}"),
    }
}

/// Whether objects of `code` are entities.
pub fn is_entity(code: u16, classes: &Classes) -> bool {
    if code >= 500 && code != 0x1F2 && code != 0x1F3 {
        return classes.get(code).is_some_and(|c| c.is_entity());
    }
    fixed_is_entity(code)
}
