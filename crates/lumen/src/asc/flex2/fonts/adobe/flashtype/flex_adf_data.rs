//! Port of flex-fontkit.jar `flash.fonts.flashtype.FlexADFData`.

use crate::asc::flex2::fonts::adobe::flashtype::adf_type_system::ADFPath;

#[derive(Clone, Debug, Default)]
pub struct FlexADFData {
    pub m_path: ADFPath,
}
