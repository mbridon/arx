mod builder;
mod entry;
mod entry_type;
mod properties;

pub use builder::Builder;
pub(crate) use builder::{FullBuilder, RealBuilder};
pub use entry::{Entry, EntryDef};
pub use entry_type::EntryType;
use jbk::reader::builder::{BuilderTrait, PropertyBuilderTrait};
use jbk::reader::Range;
use jubako as jbk;
pub use properties::AllProperties;
use std::rc::Rc;

pub type EntryResult<T> = Result<T, EntryType>;
pub use jbk::SubReader as Reader;

pub struct Comparator {
    store: Rc<jbk::reader::EntryStore>,
    path_property: jbk::reader::builder::ArrayProperty,
}

impl Comparator {
    pub fn new(properties: &AllProperties) -> Self {
        Self {
            store: Rc::clone(&properties.store),
            path_property: properties.path_property.clone(),
        }
    }

    pub fn compare_with<'a>(&'a self, component: &'a [u8]) -> EntryCompare {
        EntryCompare {
            comparator: self,
            path_value: component,
        }
    }
}

pub struct EntryCompare<'a> {
    comparator: &'a Comparator,
    path_value: &'a [u8],
}

impl jbk::reader::CompareTrait for EntryCompare<'_> {
    fn compare_entry(&self, idx: jbk::EntryIdx) -> jbk::Result<std::cmp::Ordering> {
        let reader = self.comparator.store.get_entry_reader(idx);
        let entry_path = self.comparator.path_property.create(&reader)?;
        match entry_path.partial_cmp(self.path_value) {
            Some(c) => Ok(c),
            None => Err("Cannot compare".into()),
        }
    }
    fn ordered(&self) -> bool {
        true
    }
}

pub struct ReadEntry<'builder, Builder: BuilderTrait> {
    builder: &'builder Builder,
    current: jbk::EntryIdx,
    end: jbk::EntryIdx,
}

impl<'builder, Builder: BuilderTrait> ReadEntry<'builder, Builder> {
    pub fn new<R: Range>(range: &R, builder: &'builder Builder) -> Self {
        let end = range.offset() + range.count();
        Self {
            builder,
            current: range.offset(),
            end,
        }
    }

    pub fn skip(&mut self, to_skip: jbk::EntryCount) {
        self.current += to_skip;
    }
}

impl<'builder, Builder: BuilderTrait> Iterator for ReadEntry<'builder, Builder> {
    type Item = jbk::Result<Builder::Entry>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.end {
            None
        } else {
            let entry = self.builder.create_entry(self.current);
            self.current += 1;
            Some(entry)
        }
    }
}
