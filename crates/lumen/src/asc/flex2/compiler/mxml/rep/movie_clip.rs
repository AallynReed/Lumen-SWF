//! Port of `flex2.compiler.mxml.rep.MovieClip`, the [`ModelKind::MovieClip`] of a [`Model`].

use std::cell::RefCell;

use super::deps::Type;
use super::init::initializer::InitializerRef;
use super::init::value_initializer::ValueInitializer;
use super::model::{Model, ModelKind, ModelRef};
use super::mxml_document::MxmlDocumentRef;

#[derive(Default)]
pub struct MovieClipData {
    children: RefCell<Vec<InitializerRef>>,
}

impl MovieClipData {
    /// `children()`: the `VisualChildInitializer`s.
    pub fn children(&self) -> Vec<InitializerRef> {
        self.children.borrow().clone()
    }

    pub fn has_children(&self) -> bool {
        !self.children.borrow().is_empty()
    }

    pub fn get_child_initializer_iterator(&self) -> Vec<InitializerRef> {
        self.children()
    }
}

impl Model {
    /// `new MovieClip(document, type, parent, line)`.
    pub fn new_movie_clip(document: &MxmlDocumentRef, type_: Type, parent: Option<ModelRef>, line: i32) -> ModelRef {
        Model::create(document, type_, parent, line, ModelKind::MovieClip(MovieClipData::default()))
    }

    pub fn as_movie_clip(&self) -> Option<&MovieClipData> {
        match &self.kind {
            ModelKind::MovieClip(m) => Some(m),
            _ => None,
        }
    }

    /// `MovieClip.addChild(child)`.
    pub fn add_child(&self, child: &ModelRef) {
        let m = self.as_movie_clip().expect("not a MovieClip");
        m.children.borrow_mut().push(ValueInitializer::new_visual_child(child));
        if self.standard_defs.is_repeater(child.get_type()) {
            self.get_document().ensure_declaration(&self.self_ref());
        }
    }

    /// `MovieClip.children()`.
    pub fn children(&self) -> Vec<InitializerRef> {
        self.as_movie_clip().map(|m| m.children()).unwrap_or_default()
    }

    pub fn has_children(&self) -> bool {
        self.as_movie_clip().is_some_and(|m| m.has_children())
    }

    pub fn get_child_initializer_iterator(&self) -> Vec<InitializerRef> {
        self.children()
    }
}
