//! Port of swfutils `flash.fonts.FSType`, the OS/2 embedding licence of a font.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FSType {
    pub fs_type: i32,
    pub description: String,
    pub installable: bool,
    pub editable: bool,
    pub preview_and_print: bool,
    pub no_embedding: bool,
    pub usable_by_flex: bool,
}

impl FSType {
    pub fn new(fs_type: i32, description: &str, installable: bool, editable: bool, preview_and_print: bool, no_embedding: bool) -> FSType {
        FSType { fs_type, description: description.to_string(), installable, editable, preview_and_print, no_embedding, usable_by_flex: true }
    }

    /// `getFSType(int)`.
    pub fn get_fs_type(lt: i32) -> FSType {
        let no_embedding_bit = (lt & 0x0002) == 0x0002;
        let preview_and_print_bit = (lt & 0x0004) == 0x0004;
        let editable_bit = (lt & 0x0008) == 0x0008;
        if editable_bit {
            FSType::new(lt, "Editable embedding", false, true, false, false)
        } else if preview_and_print_bit {
            FSType::new(lt, "Preview and Print embedding", false, false, true, false)
        } else if no_embedding_bit {
            FSType::new(lt, "No embedding allowed", false, false, false, true)
        } else {
            FSType::new(lt, "Installable embedding", true, false, false, false)
        }
    }
}
