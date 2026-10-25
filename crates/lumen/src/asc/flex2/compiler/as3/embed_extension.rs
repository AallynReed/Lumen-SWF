//! Port of `flex2.compiler.as3.EmbedExtension`, the compiler extension behind `[Embed]`.

use super::embed_evaluator::EmbedEvaluator;
use super::embed_skin_class_evaluator::EmbedSkinClassEvaluator;
use super::evaluator_adapter;
use super::extension::Extension;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::transcoder::Transcoders;
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::flex2::flash::swf::tags::Body;

pub struct EmbedExtension {
    transcoders: Transcoders,
    generated_output_dir: Option<String>,
    check_deprecation: bool,
}

impl EmbedExtension {
    pub fn new(transcoders: Transcoders, generated_output_dir: Option<String>, check_deprecation: bool) -> EmbedExtension {
        EmbedExtension { transcoders, generated_output_dir, check_deprecation }
    }
}

/// `AbcClass.isSubclassOf(name)` through the superclass chain the symbol table knows.
fn is_subclass_of(symbol_table: &SymbolTable, class_name: &str, base: &str) -> bool {
    let mut name = Some(class_name.to_string());
    let mut guard = 0;
    while let Some(n) = name {
        if n == base {
            return true;
        }
        guard += 1;
        if guard > 256 {
            return false;
        }
        name = symbol_table.get_class(Some(&n)).and_then(|c| c.super_type_name.as_deref().map(name_formatter::to_colon));
    }
    false
}

impl Extension for EmbedExtension {
    fn parse1(&mut self, unit: &UnitRef, _symbol_table: &mut SymbolTable) {
        if unit.borrow().metadata.is_empty() {
            return;
        }
        let (node, cx) = {
            let u = unit.borrow();
            (u.get_syntax_tree().unwrap(), u.context.get_asc_context().unwrap())
        };
        let mut e = EmbedSkinClassEvaluator::new(unit.clone());
        evaluator_adapter::evaluate(&mut e, &cx, &node);
    }

    fn parse2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().metadata.is_empty() {
            return;
        }
        let (node, cx) = {
            let u = unit.borrow();
            (u.get_syntax_tree().unwrap(), u.context.get_asc_context().unwrap())
        };
        let mut e = EmbedEvaluator::new(unit.clone(), symbol_table, self.transcoders.clone(), self.generated_output_dir.clone(), self.check_deprecation);
        evaluator_adapter::evaluate(&mut e, &cx, &node);
    }

    /// `generate`: warns when an asset's class can't carry its tag. `isPublic` is not in the ported type table, so
    /// `NonPublicAssetClass` is not reported.
    fn generate(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let assets: Vec<(String, crate::asc::flex2::flash::swf::tags::TagRef)> = {
            let u = unit.borrow();
            u.assets.iter().map(|(k, v)| (k.clone(), v.define_tag.clone())).collect()
        };
        let reporting = unit.borrow().get_source().borrow().get_name_for_reporting().to_string();
        for (class_name, tag) in assets {
            let colon = name_formatter::to_colon(&class_name);
            let Some(c) = symbol_table.get_class(Some(&colon)) else { continue };
            let sub = |b: &str| is_subclass_of(symbol_table, &c.name, b);
            let check = {
                let t = tag.borrow();
                match &t.body {
                    Body::DefineSprite(_) if !sub("flash.display:Sprite") => Some(("DefineSprite", "flash.display.Sprite")),
                    Body::DefineBits(_) if !sub("flash.display:Bitmap") && !sub("flash.display:BitmapData") => Some(("DefineBits", "flash.display.Bitmap or flash.display.BitmapData")),
                    Body::DefineSound(_) if !sub("flash.media:Sound") => Some(("DefineSound", "flash.media.Sound")),
                    Body::DefineFont1(_) | Body::DefineFont2(_) | Body::DefineFont4(_) if !sub("flash.text:Font") => Some(("DefineFont", "flash.text.Font")),
                    Body::DefineText(_) if !sub("flash.display:StaticText") => Some(("DefineText", "flash.display.StaticText")),
                    Body::DefineEditText(_) if !sub("flash.display:TextField") => Some(("DefineEditText", "flash.display.TextField")),
                    Body::DefineShape(_) if !sub("flash.display:Shape") => Some(("DefineShape", "flash.display.Shape")),
                    Body::DefineButton(_) if !sub("flash.display:SimpleButton") => Some(("DefineButton", "flash.display.SimpleButton")),
                    Body::DefineBinaryData { .. } if !sub("flash.utils:ByteArray") => Some(("DefineBinaryData", "flash.utils.ByteArray")),
                    _ => None,
                }
            };
            if let Some((asset_type, required_base)) = check {
                toolkit::log_for(
                    CompilerMessage::warning(format!("Class '{}' does not extend the '{asset_type}' asset base class '{required_base}'.", c.name)),
                    &reporting,
                );
            }
        }
    }
}
