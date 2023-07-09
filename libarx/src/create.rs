use jubako as jbk;

use crate::common::{EntryType, Property};
use jbk::creator::schema;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

const VENDOR_ID: u32 = 0x41_52_58_00;

type EntryStore = jbk::creator::EntryStore<
    Property,
    EntryType,
    Box<jbk::creator::BasicEntry<Property, EntryType>>,
>;

pub enum ConcatMode {
    OneFile,
    TwoFiles,
    NoConcat,
}

pub enum EntryKind {
    Dir(Box<dyn Iterator<Item = jbk::Result<Box<dyn EntryTrait>>>>),
    File(jbk::Reader),
    Link(OsString),
}

pub trait EntryTrait {
    /// The kind of the entry
    fn kind(self: Box<Self>) -> jbk::Result<EntryKind>;

    /// Under which name the entry will be stored
    fn name(&self) -> &OsStr;

    fn uid(&self) -> u64;
    fn gid(&self) -> u64;
    fn mode(&self) -> u64;
    fn mtime(&self) -> u64;
}

type DirCache = HashMap<OsString, DirEntry>;
type EntryIdx = jbk::Bound<jbk::EntryIdx>;
pub type Void = jbk::Result<()>;

/// A DirEntry structure to keep track of added direcotry in the archive.
/// This is needed as we may adde file without recursion, and so we need
/// to find the parent of "foo/bar/baz.txt" ("foo/bar") when we add it.
struct DirEntry {
    idx: Option<EntryIdx>,
    dir_children: Rc<DirCache>,
    file_children: Rc<Vec<EntryIdx>>,
}

impl DirEntry {
    fn new_root() -> Self {
        Self {
            idx: None,
            dir_children: Default::default(),
            file_children: Default::default(),
        }
    }
    fn new(idx: EntryIdx) -> Self {
        Self {
            idx: Some(idx),
            dir_children: Default::default(),
            file_children: Default::default(),
        }
    }

    fn first_entry_generator(&self) -> Box<dyn Fn() -> u64> {
        let dir_children = Rc::clone(&self.dir_children);
        let file_children = Rc::clone(&self.file_children);
        Box::new(move || {
            if dir_children.is_empty() && file_children.is_empty() {
                0
            } else {
                std::cmp::min(
                    file_children
                        .iter()
                        .map(|i| i.get().into_u64())
                        .min()
                        .unwrap_or(u64::MAX),
                    dir_children
                        .values()
                        // Unwrap is safe because children are not root, and idx is Some
                        .map(|i| i.idx.as_ref().unwrap().get().into_u64())
                        .min()
                        .unwrap_or(u64::MAX),
                )
            }
        })
    }

    fn entry_count_generator(&self) -> Box<dyn Fn() -> u64> {
        let dir_children = Rc::clone(&self.dir_children);
        let file_children = Rc::clone(&self.file_children);
        Box::new(move || (dir_children.len() + file_children.len()) as u64)
    }

    fn as_parent_idx_generator(&self) -> Box<dyn Fn() -> u64> {
        match &self.idx {
            Some(idx) => {
                let idx = idx.clone();
                Box::new(move || idx.get().into_u64() + 1)
            }
            None => Box::new(|| 0),
        }
    }

    fn add<E, Adder>(
        &mut self,
        entry: Box<E>,
        entry_store: &mut EntryStore,
        add_content: &mut Adder,
    ) -> Void
    where
        E: EntryTrait + ?Sized,
        Adder: FnMut(jbk::Reader) -> jbk::Result<jbk::ContentIdx>,
    {
        let entry_name = entry.name().to_os_string();
        let mut values = HashMap::from([
            (
                Property::Name,
                jbk::Value::Array(entry_name.clone().into_vec()),
            ),
            (
                Property::Parent,
                jbk::Value::Unsigned(self.as_parent_idx_generator().into()),
            ),
            (Property::Owner, jbk::Value::Unsigned(entry.uid().into())),
            (Property::Group, jbk::Value::Unsigned(entry.gid().into())),
            (Property::Rights, jbk::Value::Unsigned(entry.mode().into())),
            (Property::Mtime, jbk::Value::Unsigned(entry.mtime().into())),
        ]);

        match entry.kind()? {
            EntryKind::Dir(children) => {
                if self.dir_children.contains_key(&entry_name) {
                    return Ok(());
                }
                let entry_idx = jbk::Vow::new(jbk::EntryIdx::from(0));
                let mut dir_entry = DirEntry::new(entry_idx.bind());

                {
                    values.insert(
                        Property::FirstChild,
                        jbk::Value::Unsigned(dir_entry.first_entry_generator().into()),
                    );
                    values.insert(
                        Property::NbChildren,
                        jbk::Value::Unsigned(dir_entry.entry_count_generator().into()),
                    );
                    let entry = Box::new(jbk::creator::BasicEntry::new_from_schema_idx(
                        &entry_store.schema,
                        entry_idx,
                        Some(EntryType::Dir),
                        values,
                    ));
                    entry_store.add_entry(entry);
                }
                for sub_entry in children {
                    dir_entry.add(sub_entry?, entry_store, add_content)?;
                }

                /* SAFETY: We already have Rc on `self.dir_children` but it is only used
                  in a second step to get entry_count and min entry_idx.
                  So while we borrow `self.dir_children` we never read it otherwise.
                */
                unsafe { Rc::get_mut_unchecked(&mut self.dir_children) }
                    .entry(entry_name)
                    .or_insert(dir_entry);
                Ok(())
            }
            EntryKind::File(reader) => {
                let size = reader.size();
                let content_id = add_content(reader)?;
                values.insert(
                    Property::Content,
                    jbk::Value::Content(jbk::ContentAddress::new(jbk::PackId::from(1), content_id)),
                );
                values.insert(Property::Size, jbk::Value::Unsigned(size.into_u64().into()));
                let entry = Box::new(jbk::creator::BasicEntry::new_from_schema(
                    &entry_store.schema,
                    Some(EntryType::File),
                    values,
                ));
                let current_idx = entry_store.add_entry(entry);
                /* SAFETY: We already have Rc on `self.file_children` but it is only used
                  in a second step to get entry_count and min entry_idx.
                  So while we borrow `self.file_children` we never read it otherwise.
                */
                unsafe { Rc::get_mut_unchecked(&mut self.file_children) }.push(current_idx);
                Ok(())
            }
            EntryKind::Link(target) => {
                values.insert(Property::Target, jbk::Value::Array(target.into_vec()));
                let entry = Box::new(jbk::creator::BasicEntry::new_from_schema(
                    &entry_store.schema,
                    Some(EntryType::Link),
                    values,
                ));
                let current_idx = entry_store.add_entry(entry);
                /* SAFETY: We already have Rc on `self.file_children` but it is only used
                  in a second step to get entry_count and min entry_idx.
                  So while we borrow `self.file_children` we never read it otherwise.
                */
                unsafe { Rc::get_mut_unchecked(&mut self.file_children) }.push(current_idx);
                Ok(())
            }
        }
    }
}

pub struct Creator {
    content_pack: jbk::creator::CachedContentPackCreator,
    directory_pack: jbk::creator::DirectoryPackCreator,
    entry_store: Box<EntryStore>,
    dir_cache: DirEntry,
    concat_mode: ConcatMode,
    tmp_path_content_pack: tempfile::TempPath,
    tmp_path_directory_pack: tempfile::TempPath,
}

impl Creator {
    pub fn new<P: AsRef<Path>>(
        outfile: P,
        concat_mode: ConcatMode,
        progress: Arc<dyn jbk::creator::Progress>,
        cache_progress: Rc<dyn jbk::creator::CacheProgress>,
    ) -> jbk::Result<Self> {
        let outfile = outfile.as_ref();
        let out_dir = outfile.parent().unwrap();

        let (tmp_content_pack, tmp_path_content_pack) =
            tempfile::NamedTempFile::new_in(out_dir)?.into_parts();
        let content_pack = jbk::creator::ContentPackCreator::new_from_file_with_progress(
            tmp_content_pack,
            jbk::PackId::from(1),
            VENDOR_ID,
            jbk::FreeData40::clone_from_slice(&[0x00; 40]),
            jbk::CompressionType::Zstd,
            progress,
        )?;

        let (_, tmp_path_directory_pack) = tempfile::NamedTempFile::new_in(out_dir)?.into_parts();
        let mut directory_pack = jbk::creator::DirectoryPackCreator::new(
            &tmp_path_directory_pack,
            jbk::PackId::from(0),
            VENDOR_ID,
            jbk::FreeData31::clone_from_slice(&[0x00; 31]),
        );

        let path_store = directory_pack.create_value_store(jbk::creator::ValueStoreKind::Plain);

        let entry_def = schema::Schema::new(
            // Common part
            schema::CommonProperties::new(vec![
                schema::Property::new_array(1, Rc::clone(&path_store), Property::Name), // the path
                schema::Property::new_uint(Property::Parent), // index of the parent entry
                schema::Property::new_uint(Property::Owner),  // owner
                schema::Property::new_uint(Property::Group),  // group
                schema::Property::new_uint(Property::Rights), // rights
                schema::Property::new_uint(Property::Mtime),  // modification time
            ]),
            vec![
                // File
                (
                    EntryType::File,
                    schema::VariantProperties::new(vec![
                        schema::Property::new_content_address(Property::Content),
                        schema::Property::new_uint(Property::Size), // Size
                    ]),
                ),
                // Directory
                (
                    EntryType::Dir,
                    schema::VariantProperties::new(vec![
                        schema::Property::new_uint(Property::FirstChild), // index of the first entry
                        schema::Property::new_uint(Property::NbChildren), // nb entries in the directory
                    ]),
                ),
                // Link
                (
                    EntryType::Link,
                    schema::VariantProperties::new(vec![
                        schema::Property::new_array(1, Rc::clone(&path_store), Property::Target), // Id of the linked entry
                    ]),
                ),
            ],
            Some(vec![Property::Parent, Property::Name]),
        );

        let entry_store = Box::new(EntryStore::new(entry_def));

        let root_entry = DirEntry::new_root();

        Ok(Self {
            content_pack: jbk::creator::CachedContentPackCreator::new(content_pack, cache_progress),
            directory_pack,
            entry_store,
            dir_cache: root_entry,
            concat_mode,
            tmp_path_content_pack,
            tmp_path_directory_pack,
        })
    }

    pub fn finalize(mut self, outfile: &Path) -> Void {
        let entry_count = self.entry_store.len();
        let entry_store_id = self.directory_pack.add_entry_store(self.entry_store);
        self.directory_pack.create_index(
            "arx_entries",
            jubako::ContentAddress::new(0.into(), 0.into()),
            jbk::PropertyIdx::from(0),
            entry_store_id,
            jbk::EntryCount::from(entry_count as u32),
            jubako::EntryIdx::from(0).into(),
        );
        self.directory_pack.create_index(
            "arx_root",
            jubako::ContentAddress::new(0.into(), 0.into()),
            jbk::PropertyIdx::from(0),
            entry_store_id,
            jbk::EntryCount::from(self.dir_cache.entry_count_generator()() as u32),
            jubako::EntryIdx::from(0).into(),
        );

        let directory_pack_info = match self.concat_mode {
            ConcatMode::NoConcat => {
                let mut outfilename = outfile.file_name().unwrap().to_os_string();
                outfilename.push(".jbkd");
                let mut directory_pack_path = PathBuf::new();
                directory_pack_path.push(outfile);
                directory_pack_path.set_file_name(outfilename);
                let directory_pack_info = self
                    .directory_pack
                    .finalize(Some(directory_pack_path.clone()))?;
                if let Err(e) = self.tmp_path_directory_pack.persist(&directory_pack_path) {
                    return Err(e.error.into());
                };
                directory_pack_info
            }
            _ => self.directory_pack.finalize(None)?,
        };

        let content_pack_info = match self.concat_mode {
            ConcatMode::OneFile => self.content_pack.into_inner().finalize(None)?,
            _ => {
                let mut outfilename = outfile.file_name().unwrap().to_os_string();
                outfilename.push(".jbkc");
                let mut content_pack_path = PathBuf::new();
                content_pack_path.push(outfile);
                content_pack_path.set_file_name(outfilename);
                let content_pack_info = self
                    .content_pack
                    .into_inner()
                    .finalize(Some(content_pack_path.clone()))?;
                if let Err(e) = self.tmp_path_content_pack.persist(&content_pack_path) {
                    return Err(e.error.into());
                }
                content_pack_info
            }
        };

        let mut manifest_creator = jbk::creator::ManifestPackCreator::new(
            outfile,
            VENDOR_ID,
            jbk::FreeData63::clone_from_slice(&[0x00; 63]),
        );

        manifest_creator.add_pack(directory_pack_info);
        manifest_creator.add_pack(content_pack_info);
        manifest_creator.finalize()?;
        Ok(())
    }

    pub fn add_entry<E>(&mut self, entry: Box<E>) -> Void
    where
        E: EntryTrait,
    {
        self.dir_cache.add(entry, &mut self.entry_store, &mut |r| {
            self.content_pack.add_content(r)
        })
    }
}
