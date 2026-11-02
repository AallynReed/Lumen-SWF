//! Port of afe.jar `com.adobe.fontengine.font.opentype.OpticalSizeData`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpticalSizeData {
    pub design_size: i32,
    pub subfamily_id: i32,
    pub name_id: i32,
    pub min_size: i32,
    pub max_size: i32,
}

impl OpticalSizeData {
    pub fn new(design_size: i32, subfamily_id: i32, name_id: i32, min_size: i32, max_size: i32) -> OpticalSizeData {
        OpticalSizeData { design_size, subfamily_id, name_id, min_size, max_size }
    }
}
